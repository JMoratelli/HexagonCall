//! O telefone por tras da janela: um `Ua` do motor por conta, as ligacoes em
//! andamento e a ponte com a tela. Cada evento do motor vira um `Aviso` para a
//! tela (pelo evento "telefone" do Tauri); o que acaba vai para o historico em
//! texto (`guarda`).
//!
//! Uma ligacao por vez, como no prototipo: chegando outra durante uma
//! ligacao, ela e recusada (486) e entra no historico como perdida.

use crate::guarda::{Guarda, ItemConversa, ItemHistorico, OndeASenha, Preferencias, agora};
use crate::notificacao::Notificacoes;
use motor::midia::{Destino, Fonte};
use motor::{Evento, Opcoes, Registro, Ua};
use nucleo::conta::Conta;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

/// O que vai (microfone) e o que volta (alto-falante) numa ligacao, e o
/// medidor de silencio do microfone.
type Audios = (Box<dyn Fonte>, Box<dyn Destino>, som::Medidor);

/// Segundos de silencio absoluto no microfone ate avisar "nao te ouco".
const SILENCIO_PARA_AVISAR: u32 = 8;

pub struct Telefone {
    app: AppHandle,
    guarda: Guarda,
    linhas: Mutex<Vec<Linha>>,
    chamadas: Mutex<HashMap<String, Chamada>>,
    notificacoes: Notificacoes,
    /// O medidor do microfone da ligacao em andamento.
    medidor: Mutex<Option<som::Medidor>>,
}

struct Linha {
    conta: Conta,
    ua: Option<Arc<Ua>>,
    registro: EstadoRegistro,
    detalhe: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EstadoRegistro {
    Iniciando,
    Registrado,
    Falhou,
    /// A conta so liga e manda mensagem (registra = false).
    SemRegistro,
    SemSenha,
    Parada,
}

struct Chamada {
    conta: String,
    entrada: bool,
    ramal: String,
    nome: Option<String>,
    atendida: bool,
    inicio: u64,
    atendida_em: Option<u64>,
    mudo: bool,
    espera: bool,
}

/// Uma conta como a tela mostra.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContaVista {
    pub id: String,
    pub usuario: String,
    pub dominio: String,
    pub porta: u16,
    pub nome: Option<String>,
    pub registra: bool,
    pub registro: EstadoRegistro,
    pub detalhe: Option<String>,
    pub senha: Option<OndeASenha>,
}

/// A ligacao em andamento, para a tela se recompor (janela reaberta).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChamadaVista {
    pub id: String,
    pub conta: String,
    pub entrada: bool,
    pub ramal: String,
    pub nome: Option<String>,
    pub atendida: bool,
    pub desde: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estado {
    pub contas: Vec<ContaVista>,
    pub chamada: Option<ChamadaVista>,
    pub historico: Vec<ItemHistorico>,
    pub conversas: Vec<ItemConversa>,
    pub preferencias: Preferencias,
    pub tem_chaveiro: bool,
    pub pasta: String,
}

/// O que chega na tela.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "kebab-case")]
pub enum Aviso {
    Contas {
        contas: Vec<ContaVista>,
    },
    Recebendo {
        id: String,
        conta: String,
        ramal: String,
        nome: Option<String>,
    },
    Tocando {
        id: String,
    },
    Atendida {
        id: String,
        codec: String,
    },
    Espera {
        id: String,
        sim: bool,
    },
    /// O microfone so manda silencio (`silencio`) ou voltou a mandar som.
    Microfone {
        id: String,
        silencio: bool,
    },
    Encerrada {
        id: String,
        item: ItemHistorico,
    },
    Mensagem {
        item: ItemConversa,
    },
}

/// O cadastro de uma conta, como vem da tela.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormConta {
    /// A conta que esta sendo editada; None e conta nova.
    pub id_antigo: Option<String>,
    pub usuario: String,
    pub dominio: String,
    pub porta: u16,
    pub nome: Option<String>,
    pub registra: bool,
    /// Vazio mantem a senha que ja estava.
    pub senha: Option<String>,
}

fn trava<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Telefone {
    pub fn novo(app: AppHandle, guarda: Guarda) -> Arc<Self> {
        Arc::new(Self {
            app,
            guarda,
            linhas: Mutex::new(Vec::new()),
            chamadas: Mutex::new(HashMap::new()),
            notificacoes: Notificacoes::default(),
            medidor: Mutex::new(None),
        })
    }

    pub fn guarda(&self) -> &Guarda {
        &self.guarda
    }

    /// Le as contas do disco e liga cada uma.
    pub async fn comeca(self: &Arc<Self>) {
        let contas = match self.guarda.contas() {
            Ok(c) => c,
            Err(e) => {
                warn!("contas: {e}");
                Vec::new()
            }
        };
        *trava(&self.linhas) = contas
            .iter()
            .map(|c| Linha {
                conta: c.clone(),
                ua: None,
                registro: EstadoRegistro::Iniciando,
                detalhe: None,
            })
            .collect();
        self.avisa_contas();
        for conta in contas {
            self.liga_conta(conta).await;
        }
    }

    fn avisa(&self, aviso: Aviso) {
        if let Err(e) = self.app.emit("telefone", aviso) {
            warn!("aviso para a tela: {e}");
        }
    }

    fn avisa_contas(&self) {
        self.avisa(Aviso::Contas {
            contas: self.contas_vistas(),
        });
    }

    pub fn contas_vistas(&self) -> Vec<ContaVista> {
        trava(&self.linhas)
            .iter()
            .map(|l| ContaVista {
                id: l.conta.id(),
                usuario: l.conta.usuario.clone(),
                dominio: l.conta.dominio.clone(),
                porta: l.conta.porta,
                nome: l.conta.nome.clone(),
                registra: l.conta.registra,
                registro: l.registro,
                detalhe: l.detalhe.clone(),
                senha: self.guarda.onde_a_senha(&l.conta.id()),
            })
            .collect()
    }

    pub fn estado(&self) -> Estado {
        let chamada = trava(&self.chamadas)
            .iter()
            .next()
            .map(|(id, c)| ChamadaVista {
                id: id.clone(),
                conta: c.conta.clone(),
                entrada: c.entrada,
                ramal: c.ramal.clone(),
                nome: c.nome.clone(),
                atendida: c.atendida,
                desde: c.atendida_em.unwrap_or(c.inicio),
            });
        Estado {
            contas: self.contas_vistas(),
            chamada,
            historico: self.guarda.historico(),
            conversas: self.guarda.conversas(),
            preferencias: self.guarda.preferencias(),
            tem_chaveiro: Guarda::tem_chaveiro(),
            pasta: self.guarda.pasta().display().to_string(),
        }
    }

    fn marca_linha(&self, id: &str, registro: EstadoRegistro, detalhe: Option<String>) {
        if let Some(l) = trava(&self.linhas).iter_mut().find(|l| l.conta.id() == id) {
            l.registro = registro;
            l.detalhe = detalhe;
        }
        self.avisa_contas();
    }

    /// Sobe o `Ua` da conta e fica escutando os eventos dele.
    async fn liga_conta(self: &Arc<Self>, conta: Conta) {
        let id = conta.id();
        let Some(senha) = self.guarda.senha(&id) else {
            self.marca_linha(&id, EstadoRegistro::SemSenha, None);
            return;
        };
        let opcoes = Opcoes {
            porta_sip: 0,
            registrar: conta.registra,
        };
        let registra = conta.registra;
        match Ua::inicia(conta, senha, opcoes).await {
            Ok((ua, eventos)) => {
                if let Some(l) = trava(&self.linhas).iter_mut().find(|l| l.conta.id() == id) {
                    l.ua = Some(Arc::clone(&ua));
                }
                let estado = if registra {
                    EstadoRegistro::Iniciando
                } else {
                    EstadoRegistro::SemRegistro
                };
                self.marca_linha(&id, estado, None);
                let tel = Arc::clone(self);
                tauri::async_runtime::spawn(async move { tel.escuta(id, eventos).await });
            }
            Err(e) => self.marca_linha(&id, EstadoRegistro::Falhou, Some(e.to_string())),
        }
    }

    async fn desliga_conta(&self, id: &str) {
        let ua = trava(&self.linhas)
            .iter_mut()
            .find(|l| l.conta.id() == id)
            .and_then(|l| l.ua.take());
        if let Some(ua) = ua {
            ua.encerra().await;
        }
    }

    /// Desregistra todas (ao sair do app).
    pub async fn encerra(&self) {
        let uas: Vec<Arc<Ua>> = trava(&self.linhas)
            .iter_mut()
            .filter_map(|l| l.ua.take())
            .collect();
        for ua in uas {
            ua.encerra().await;
        }
    }

    async fn escuta(self: Arc<Self>, conta: String, mut eventos: UnboundedReceiver<Evento>) {
        while let Some(e) = eventos.recv().await {
            match e {
                Evento::Registro(Registro::Registrado { .. }) => {
                    self.marca_linha(&conta, EstadoRegistro::Registrado, None);
                }
                Evento::Registro(Registro::Falhou { codigo, motivo }) => {
                    let detalhe = match codigo {
                        Some(c) => format!("{c} {motivo}"),
                        None => motivo,
                    };
                    self.marca_linha(&conta, EstadoRegistro::Falhou, Some(detalhe));
                }
                Evento::Registro(Registro::Desregistrado) => {
                    self.marca_linha(&conta, EstadoRegistro::Parada, None);
                }
                Evento::Mensagem { de, nome, texto } => {
                    let item = ItemConversa {
                        quando: agora(),
                        conta: conta.clone(),
                        ramal: de,
                        recebida: true,
                        nome,
                        texto,
                    };
                    if let Err(e) = self.guarda.acrescenta_conversa(&item) {
                        warn!("conversa: {e}");
                    }
                    self.avisa(Aviso::Mensagem { item });
                }
                Evento::Recebendo { id, de, nome } => self.chegando(&conta, id, de, nome).await,
                Evento::Tocando { id } => self.avisa(Aviso::Tocando { id }),
                Evento::Atendida { id, codec } => {
                    self.notificacoes.fecha(&id);
                    if let Some(c) = trava(&self.chamadas).get_mut(&id) {
                        c.atendida = true;
                        c.atendida_em = Some(agora());
                    }
                    let tel = Arc::clone(&self);
                    let vigiada = id.clone();
                    tauri::async_runtime::spawn(async move { tel.vigia_microfone(vigiada).await });
                    self.avisa(Aviso::Atendida {
                        id,
                        codec: codec.nome().to_string(),
                    });
                }
                Evento::Espera { id, sim } => self.avisa(Aviso::Espera { id, sim }),
                Evento::Encerrada { id, fim, .. } => {
                    self.notificacoes.fecha(&id);
                    let Some(c) = trava(&self.chamadas).remove(&id) else {
                        continue;
                    };
                    let item = ItemHistorico {
                        quando: c.inicio,
                        conta: c.conta,
                        entrada: c.entrada,
                        ramal: c.ramal,
                        nome: c.nome,
                        atendida: c.atendida,
                        duracao: c.atendida_em.map_or(0, |t| agora().saturating_sub(t)),
                        fim: fim.texto().to_string(),
                    };
                    if let Err(e) = self.guarda.acrescenta_historico(&item) {
                        warn!("historico: {e}");
                    }
                    self.avisa(Aviso::Encerrada { id, item });
                }
                Evento::Presenca { .. } | Evento::PresencaEncerrada { .. } => {}
            }
        }
    }

    /// Avisa a tela quando o microfone so manda silencio por um tempo (sem
    /// mudo e sem espera), e quando volta a mandar som.
    async fn vigia_microfone(self: Arc<Self>, id: String) {
        let mut avisado = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let Some((mudo, espera)) = trava(&self.chamadas).get(&id).map(|c| (c.mudo, c.espera))
            else {
                return;
            };
            let segundos = trava(&self.medidor)
                .as_ref()
                .map_or(0, som::Medidor::segundos_em_silencio);
            let silencio = !mudo && !espera && segundos >= SILENCIO_PARA_AVISAR;
            if silencio != avisado {
                avisado = silencio;
                self.avisa(Aviso::Microfone {
                    id: id.clone(),
                    silencio,
                });
            }
        }
    }

    async fn chegando(&self, conta: &str, id: String, ramal: String, nome: Option<String>) {
        let ocupado = !trava(&self.chamadas).is_empty();
        trava(&self.chamadas).insert(
            id.clone(),
            Chamada {
                conta: conta.to_string(),
                entrada: true,
                ramal: ramal.clone(),
                nome: nome.clone(),
                atendida: false,
                inicio: agora(),
                atendida_em: None,
                mudo: false,
                espera: false,
            },
        );
        if ocupado {
            info!(%id, "ja tem ligacao: recusando a que chegou");
            if let Some(ua) = self.ua_da_conta(conta) {
                let _ = ua.desliga(&id).await;
            }
            return;
        }
        let preferencias = self.guarda.preferencias();
        if preferencias.trazer_para_frente {
            crate::mostra(&self.app);
        }
        let quem = nome.clone().unwrap_or_else(|| ramal.clone());
        self.notificacoes.chegando(
            &self.app,
            crate::titulo(preferencias.marca),
            &id,
            &quem,
            &ramal,
        );
        self.avisa(Aviso::Recebendo {
            id,
            conta: conta.to_string(),
            ramal,
            nome,
        });
    }

    fn ua_da_conta(&self, conta: &str) -> Option<Arc<Ua>> {
        trava(&self.linhas)
            .iter()
            .find(|l| l.conta.id() == conta)
            .and_then(|l| l.ua.clone())
    }

    fn ua_da_chamada(&self, id: &str) -> Result<Arc<Ua>, String> {
        let conta = trava(&self.chamadas)
            .get(id)
            .map(|c| c.conta.clone())
            .ok_or("essa ligação já acabou")?;
        self.ua_da_conta(&conta)
            .ok_or_else(|| "a conta dessa ligação parou".into())
    }

    /// A conta pedida, ou a primeira registrada, ou a primeira que estiver no ar.
    fn ua_para_ligar(&self, conta: Option<&str>) -> Result<(String, Arc<Ua>), String> {
        let linhas = trava(&self.linhas);
        let escolhida = match conta {
            Some(id) => linhas.iter().find(|l| l.conta.id() == id),
            None => linhas
                .iter()
                .find(|l| l.registro == EstadoRegistro::Registrado && l.ua.is_some())
                .or_else(|| linhas.iter().find(|l| l.ua.is_some())),
        };
        let l = escolhida.ok_or("nenhuma conta no ar: cadastre uma nas configurações")?;
        let ua = l.ua.clone().ok_or("essa conta não está no ar")?;
        Ok((l.conta.id(), ua))
    }

    fn audio(&self) -> Result<Audios, String> {
        let p = self.guarda.preferencias();
        let mic = som::Microfone::abre(p.microfone.as_deref())
            .map_err(|e| format!("não abri o microfone: {e}"))?;
        let fone = som::AltoFalante::abre(p.alto_falante.as_deref())
            .map_err(|e| format!("não abri o alto-falante: {e}"))?;
        let medidor = mic.medidor();
        Ok((Box::new(mic), Box::new(fone), medidor))
    }

    /// Troca o microfone da ligacao em andamento (o aviso "nao te ouco"
    /// oferece) e guarda a escolha para as proximas.
    pub fn troca_microfone(&self, microfone: Option<String>) -> Result<(), String> {
        let id = trava(&self.chamadas)
            .iter()
            .find(|(_, c)| c.atendida)
            .map(|(id, _)| id.clone())
            .ok_or("nenhuma ligação em andamento")?;
        let mic = som::Microfone::abre(microfone.as_deref())
            .map_err(|e| format!("não abri esse microfone: {e}"))?;
        let medidor = mic.medidor();
        self.ua_da_chamada(&id)?
            .troca_fonte(&id, Box::new(mic))
            .map_err(|e| e.to_string())?;
        *trava(&self.medidor) = Some(medidor);
        let mut p = self.guarda.preferencias();
        p.microfone = microfone;
        self.guarda.grava_preferencias(&p)
    }

    pub async fn liga(&self, destino: &str, conta: Option<&str>) -> Result<String, String> {
        if !trava(&self.chamadas).is_empty() {
            return Err("já tem uma ligação: desligue antes de ligar de novo".into());
        }
        let (conta, ua) = self.ua_para_ligar(conta)?;
        let (fonte, saida, medidor) = self.audio()?;
        let id = ua
            .liga(destino, fonte, saida)
            .await
            .map_err(|e| e.to_string())?;
        *trava(&self.medidor) = Some(medidor);
        trava(&self.chamadas).insert(
            id.clone(),
            Chamada {
                conta,
                entrada: false,
                ramal: destino.to_string(),
                nome: None,
                atendida: false,
                inicio: agora(),
                atendida_em: None,
                mudo: false,
                espera: false,
            },
        );
        Ok(id)
    }

    pub async fn atende(&self, id: &str) -> Result<(), String> {
        let ua = self.ua_da_chamada(id)?;
        let (fonte, saida, medidor) = self.audio()?;
        ua.atende(id, fonte, saida)
            .await
            .map_err(|e| e.to_string())?;
        *trava(&self.medidor) = Some(medidor);
        Ok(())
    }

    pub async fn desliga(&self, id: &str) -> Result<(), String> {
        let ua = self.ua_da_chamada(id)?;
        ua.desliga(id).await.map_err(|e| e.to_string())
    }

    pub fn mudo(&self, id: &str, sim: bool) -> Result<(), String> {
        self.ua_da_chamada(id)?
            .mudo(id, sim)
            .map_err(|e| e.to_string())?;
        if let Some(c) = trava(&self.chamadas).get_mut(id) {
            c.mudo = sim;
        }
        Ok(())
    }

    pub async fn espera(&self, id: &str, sim: bool) -> Result<(), String> {
        let ua = self.ua_da_chamada(id)?;
        ua.espera(id, sim).await.map_err(|e| e.to_string())?;
        if let Some(c) = trava(&self.chamadas).get_mut(id) {
            c.espera = sim;
        }
        Ok(())
    }

    pub async fn mensagem(
        &self,
        destino: &str,
        texto: &str,
        conta: Option<&str>,
    ) -> Result<ItemConversa, String> {
        let (conta, ua) = self.ua_para_ligar(conta)?;
        ua.mensagem(destino, texto)
            .await
            .map_err(|e| e.to_string())?;
        let item = ItemConversa {
            quando: agora(),
            conta,
            ramal: destino.to_string(),
            recebida: false,
            nome: None,
            texto: texto.to_string(),
        };
        if let Err(e) = self.guarda.acrescenta_conversa(&item) {
            warn!("conversa: {e}");
        }
        Ok(item)
    }

    /// Cadastra ou edita: grava o arquivo, a senha, e sobe a conta de novo.
    pub async fn salva_conta(self: &Arc<Self>, f: FormConta) -> Result<(), String> {
        let mut conta = Conta::nova(f.usuario.trim(), f.dominio.trim());
        conta.porta = f.porta;
        conta.nome = f
            .nome
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());
        conta.registra = f.registra;
        conta.valida().map_err(|e| e.to_string())?;
        let id = conta.id();
        let senha = f.senha.filter(|s| !s.is_empty());
        if f.id_antigo.is_none() && senha.is_none() {
            return Err("digite a senha da conta".into());
        }
        let nova = Linha {
            conta: conta.clone(),
            ua: None,
            registro: EstadoRegistro::Iniciando,
            detalhe: None,
        };
        let ua_antigo = {
            let mut linhas = trava(&self.linhas);
            let repetida = linhas
                .iter()
                .any(|l| l.conta.id() == id && f.id_antigo.as_deref() != Some(id.as_str()));
            if repetida {
                return Err(format!("a conta {id} já está cadastrada"));
            }
            // Editando, a conta fica no mesmo lugar da lista.
            let antiga = f
                .id_antigo
                .as_ref()
                .and_then(|a| linhas.iter_mut().find(|l| &l.conta.id() == a));
            let ua_antigo = match antiga {
                Some(l) => std::mem::replace(l, nova).ua,
                None => {
                    linhas.push(nova);
                    None
                }
            };
            let contas: Vec<Conta> = linhas.iter().map(|l| l.conta.clone()).collect();
            self.guarda.grava_contas(&contas)?;
            ua_antigo
        };
        if let Some(ua) = ua_antigo {
            ua.encerra().await;
        }
        if let Some(antigo) = f.id_antigo.as_deref().filter(|a| *a != id) {
            // Trocou o ramal ou o PBX: a senha vai junto para o id novo.
            if let (None, Some(s)) = (&senha, self.guarda.senha(antigo)) {
                self.guarda.grava_senha(&id, &s)?;
            }
            self.guarda.apaga_senha(antigo)?;
        }
        if let Some(s) = senha {
            self.guarda.grava_senha(&id, &s)?;
        }
        self.avisa_contas();
        self.liga_conta(conta).await;
        Ok(())
    }

    pub async fn remove_conta(&self, id: &str) -> Result<(), String> {
        self.desliga_conta(id).await;
        let contas: Vec<Conta> = {
            let mut linhas = trava(&self.linhas);
            linhas.retain(|l| l.conta.id() != id);
            linhas.iter().map(|l| l.conta.clone()).collect()
        };
        self.guarda.grava_contas(&contas)?;
        self.guarda.apaga_senha(id)?;
        self.avisa_contas();
        Ok(())
    }
}
