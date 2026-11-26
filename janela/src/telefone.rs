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
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};
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
    /// O toque de quem liga, enquanto toca (largar devolve o volume).
    tocador: Mutex<Option<som::toque::Tocador>>,
}

struct Linha {
    conta: Conta,
    ua: Option<Arc<Ua>>,
    registro: EstadoRegistro,
    detalhe: Option<String>,
    /// A ultima falha foi de rede (sem resposta do PBX), nao uma recusa: so
    /// essa se tenta de novo sozinha. Recusa de senha repetida faz o
    /// fail2ban do PBX bloquear o IP.
    falha_de_rede: bool,
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

/// Um toque de quem liga, para a lista das configuracoes.
#[derive(Debug, Clone, Serialize)]
pub struct ToqueVista {
    pub id: &'static str,
    pub nome: &'static str,
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
    pub toques: Vec<ToqueVista>,
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
            tocador: Mutex::new(None),
        })
    }

    pub fn guarda(&self) -> &Guarda {
        &self.guarda
    }

    /// Le as contas do disco, liga cada uma e fica vigiando a rede.
    pub async fn comeca(self: &Arc<Self>) {
        let tel = Arc::clone(self);
        tauri::async_runtime::spawn(async move { tel.vigia_rede().await });
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
                falha_de_rede: false,
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
            toques: som::toque::toques()
                .iter()
                .map(|t| ToqueVista {
                    id: t.id,
                    nome: t.nome,
                })
                .collect(),
        }
    }

    fn marca_linha(&self, id: &str, registro: EstadoRegistro, detalhe: Option<String>) {
        self.marca_linha_com(id, registro, detalhe, false);
    }

    fn marca_linha_com(
        &self,
        id: &str,
        registro: EstadoRegistro,
        detalhe: Option<String>,
        falha_de_rede: bool,
    ) {
        if let Some(l) = trava(&self.linhas).iter_mut().find(|l| l.conta.id() == id) {
            l.registro = registro;
            l.detalhe = detalhe;
            l.falha_de_rede = falha_de_rede;
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
            // Nem subiu: em geral, sem rede ou sem DNS (app aberto antes da rede).
            Err(e) => self.marca_linha_com(&id, EstadoRegistro::Falhou, Some(e.to_string()), true),
        }
    }

    /// Sobe a conta de novo, sem desregistrar a velha (ver `Ua::abandona`).
    async fn religa_conta(self: &Arc<Self>, id: &str) {
        let (ua, conta) = {
            let mut linhas = trava(&self.linhas);
            let Some(l) = linhas.iter_mut().find(|l| l.conta.id() == id) else {
                return;
            };
            (l.ua.take(), l.conta.clone())
        };
        if let Some(ua) = ua {
            ua.abandona().await;
        }
        self.marca_linha(id, EstadoRegistro::Iniciando, None);
        self.liga_conta(conta).await;
    }

    /// "Reconectar", na tela.
    pub async fn reconecta(self: &Arc<Self>, id: &str) -> Result<(), String> {
        if self.ua_da_conta(id).is_some_and(|ua| ua.em_ligacao()) {
            return Err("a conta está numa ligação: reconecta quando ela acabar".into());
        }
        self.religa_conta(id).await;
        Ok(())
    }

    /// A cada 5 s: o computador voltou da suspensao (o relogio do motor para
    /// enquanto ele dorme, e o registro vence no PBX), a rede mudou (o IP para
    /// chegar ao PBX nao e o anunciado), caiu e voltou, ou a conta nem subiu
    /// (app aberto antes da rede, no boot). Em qualquer caso, a conta sobe de
    /// novo; com ligacao em andamento, depois que ela acabar.
    async fn vigia_rede(self: Arc<Self>) {
        let mut antes = (SystemTime::now(), Instant::now());
        let mut sem_rota: HashSet<String> = HashSet::new();
        let mut pendentes: HashSet<String> = HashSet::new();
        let mut tentou: HashMap<String, Instant> = HashMap::new();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let agora = (SystemTime::now(), Instant::now());
            let acordou = voltou_da_suspensao(antes, agora);
            antes = agora;
            if acordou {
                info!("o computador voltou da suspensao");
            }
            let contas: Vec<(String, Option<Arc<Ua>>, bool)> = trava(&self.linhas)
                .iter()
                .map(|l| {
                    let rede = l.registro == EstadoRegistro::Falhou && l.falha_de_rede;
                    (l.conta.id(), l.ua.clone(), rede)
                })
                .collect();
            for (id, ua, falha_de_rede) in contas {
                // Falha de rede (nem subiu, ou o PBX nao respondeu): de novo a
                // cada 30 s, se nao houver ligacao.
                if falha_de_rede && ua.as_ref().is_none_or(|u| !u.em_ligacao()) {
                    let vez = tentou
                        .get(&id)
                        .is_none_or(|t| t.elapsed() >= Duration::from_secs(30));
                    if vez {
                        tentou.insert(id.clone(), Instant::now());
                        info!(conta = %id, "sem resposta do PBX: tentando de novo");
                        self.religa_conta(&id).await;
                    }
                    continue;
                }
                let Some(ua) = ua else { continue };
                let motivo = match ua.ip_para_o_pbx() {
                    None => {
                        sem_rota.insert(id);
                        continue;
                    }
                    Some(ip) if ip != ua.ip() => Some("a rede mudou"),
                    Some(_) if sem_rota.remove(&id) => Some("a rede voltou"),
                    Some(_) if acordou => Some("voltou da suspensao"),
                    Some(_) if pendentes.contains(&id) => Some("pendente"),
                    Some(_) => None,
                };
                let Some(motivo) = motivo else { continue };
                if ua.em_ligacao() {
                    pendentes.insert(id);
                    continue;
                }
                pendentes.remove(&id);
                info!(conta = %id, "{motivo}: registrando de novo");
                self.religa_conta(&id).await;
            }
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
                    let de_rede = codigo.is_none();
                    self.marca_linha_com(&conta, EstadoRegistro::Falhou, Some(detalhe), de_rede);
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
                    self.para_o_toque();
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
                    if trava(&self.chamadas)
                        .get(&id)
                        .is_some_and(|c| c.entrada && !c.atendida)
                    {
                        self.para_o_toque();
                    }
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
            crate::titulo(preferencias.flavor),
            &id,
            &quem,
            &ramal,
        );
        self.toca_o_toque(&preferencias).await;
        self.avisa(Aviso::Recebendo {
            id,
            conta: conta.to_string(),
            ramal,
            nome,
        });
    }

    /// Toca o toque de quem liga como as preferencias pedem (abrir as saidas e
    /// mexer no volume bloqueia um pouco: fora da tarefa do telefone).
    async fn toca_o_toque(&self, p: &Preferencias) {
        let toque = p.toque.clone();
        let onde = onde_tocar(p);
        let intensidade = intensidade(p.volume_toque);
        let maximo = p.toque_no_maximo;
        let aberto = tauri::async_runtime::spawn_blocking(move || {
            som::toque::Tocador::toca(&toque, &onde, intensidade, maximo)
        })
        .await;
        match aberto {
            Ok(Ok(t)) => *trava(&self.tocador) = Some(t),
            Ok(Err(e)) => warn!("o toque nao tocou: {e}"),
            Err(e) => warn!("o toque nao tocou: {e}"),
        }
    }

    fn para_o_toque(&self) {
        let tocador = trava(&self.tocador).take();
        if let Some(t) = tocador {
            // Devolver o volume roda o pactl: fora da tarefa do telefone.
            std::thread::spawn(move || drop(t));
        }
    }

    /// Toca o toque por 3 s, para escolher nas configuracoes.
    pub async fn ouve_toque(&self, toque: String) -> Result<(), String> {
        if !trava(&self.chamadas).is_empty() {
            return Err("durante a ligação, não".into());
        }
        let p = self.guarda.preferencias();
        let onde = onde_tocar(&p);
        let intensidade = intensidade(p.volume_toque);
        let maximo = p.toque_no_maximo;
        tauri::async_runtime::spawn_blocking(move || {
            let tocador = som::toque::Tocador::toca(&toque, &onde, intensidade, maximo)?;
            std::thread::sleep(std::time::Duration::from_secs(3));
            drop(tocador);
            Ok::<(), som::ErroSom>(())
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
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
        // O toque sai antes de abrir o audio da ligacao (pode ser a mesma saida).
        self.para_o_toque();
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

    pub fn tons(&self, id: &str, teclas: &str) -> Result<(), String> {
        self.ua_da_chamada(id)?
            .tons(id, teclas)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub async fn transfere(&self, id: &str, destino: &str) -> Result<(), String> {
        let ua = self.ua_da_chamada(id)?;
        ua.transfere(id, destino).await.map_err(|e| e.to_string())
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
            falha_de_rede: false,
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

fn onde_tocar(p: &Preferencias) -> som::toque::Onde {
    if p.toque_em_todos {
        som::toque::Onde::Todas
    } else {
        som::toque::Onde::Um(p.alto_falante.clone())
    }
}

/// O volume do toque (0 a 100) na amplitude: em curva, como o ouvido sente.
/// 50 da o volume do toque aprovado em 26/11.
fn intensidade(volume: u8) -> f32 {
    let v = f32::from(volume.min(100)) / 100.0;
    0.25 * v * v
}

/// O relogio da parede andou bem mais que o do motor (que para enquanto o
/// computador dorme): voltou da suspensao. 15 s de folga para acerto de hora.
fn voltou_da_suspensao(antes: (SystemTime, Instant), agora: (SystemTime, Instant)) -> bool {
    let parede = agora.0.duration_since(antes.0).unwrap_or_default();
    let motor = agora.1.saturating_duration_since(antes.1);
    parede.saturating_sub(motor) > Duration::from_secs(15)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn suspensao_e_o_relogio_da_parede_pulando() {
        let (p, m) = (SystemTime::now(), Instant::now());
        let normal = (p + Duration::from_secs(5), m + Duration::from_secs(5));
        assert!(!voltou_da_suspensao((p, m), normal));
        let acerto_de_hora = (p + Duration::from_secs(12), m + Duration::from_secs(5));
        assert!(!voltou_da_suspensao((p, m), acerto_de_hora));
        let dormiu = (p + Duration::from_secs(3600), m + Duration::from_secs(5));
        assert!(voltou_da_suspensao((p, m), dormiu));
        // Hora voltando para tras nao e suspensao.
        let para_tras = (p - Duration::from_secs(600), m + Duration::from_secs(5));
        assert!(!voltou_da_suspensao((p, m), para_tras));
    }
}
