//! O telefone por tras da janela: um `Ua` do motor por conta, as ligacoes em
//! andamento e a ponte com a tela. Cada evento do motor vira um `Aviso` para a
//! tela (pelo evento "telefone" do Tauri); o que acaba vai para o historico em
//! texto (`guarda`).
//!
//! Uma ligacao por vez, como no prototipo: chegando outra durante uma
//! ligacao, ela e recusada (486) e entra no historico como perdida.

use crate::gravacao::Gravacoes;
use crate::guarda::{
    ChaveHistorico, Guarda, ItemConversa, ItemHistorico, OndeASenha, Preferencias, agora,
};
use crate::musica::Musica;
use crate::notificacao::Notificacoes;
use motor::midia::{Destino, Fonte, Silencio};
use motor::{Evento, Opcoes, Registro, Ua};
use nucleo::conta::Conta;
use nucleo::presenca::Presenca;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{debug, info, warn};

/// O que vai (microfone) e o que volta (alto-falante) numa ligacao, o
/// medidor de silencio do microfone e o que nao abriu. Sem microfone ou sem
/// alto-falante a ligacao segue em silencio daquele lado, com o aviso na
/// tela: recusar o atender deixaria quem liga tocando sem resposta.
struct Audios {
    fonte: Box<dyn Fonte>,
    destino: Box<dyn Destino>,
    medidor: Option<som::Medidor>,
    faltando: Faltando,
}

/// O dispositivo que nao abriu, com o motivo.
#[derive(Debug, Clone, Default)]
struct Faltando {
    microfone: Option<String>,
    alto_falante: Option<String>,
}

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
    /// O teste do microfone (Ajustes) esta rodando.
    testando: std::sync::atomic::AtomicBool,
    /// O que a ligacao pausou (musica, video), para retomar ao desligar.
    musica: Musica,
    /// A pasta das gravacoes; None num sistema sem pasta de dados.
    gravacoes: Option<Gravacoes>,
    /// O ultimo estado de cada ramal assinado (Ramais), para a tela reaberta.
    presencas: Mutex<HashMap<String, Presenca>>,
}

/// Quantos ramais a conta principal assina, no maximo.
const RAMAIS_ASSINADOS: usize = 50;

/// O jeito curto da presenca, para a tela.
fn presenca_da_tela(p: Presenca) -> &'static str {
    match p {
        Presenca::Livre => "livre",
        Presenca::EmLigacao => "ocupado",
        Presenca::Chamando => "tocando",
        Presenca::Offline => "offline",
        Presenca::Desconhecida => "desconhecida",
    }
}

/// So digitos, ate 6: um ramal. Numero de fora nao tem presenca no PBX.
fn e_ramal(r: &str) -> bool {
    !r.is_empty() && r.len() <= 6 && r.chars().all(|c| c.is_ascii_digit())
}

/// O que a exclusao do historico tirou.
#[derive(Debug, Clone, Serialize)]
pub struct Exclusao {
    pub ligacoes: usize,
    pub gravacoes: usize,
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
    /// A ultima resposta do PBX a sonda: em quantos ms e quando.
    pbx: Option<(u32, u64)>,
    /// Sondas seguidas sem resposta.
    sem_resposta: u32,
    /// Desde quando o PBX nao responde (sonda ou registro sem resposta); some
    /// quando a conta registra de novo.
    fora_desde: Option<u64>,
    /// Ramais com presenca assinada pelo `Ua` de agora.
    assinadas: HashSet<String>,
}

/// Sondas seguidas sem resposta (uns 10 s, com a reconferencia de 5 s) para a
/// conta aparecer "sem resposta do PBX" e subir de novo.
const SEM_RESPOSTA: u32 = 2;

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
    /// Microfone ou alto-falante que nao abriu nesta ligacao.
    faltando: Faltando,
    /// O arquivo da gravacao, se esta gravando.
    gravacao: Option<String>,
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
    pub principal: bool,
    pub registro: EstadoRegistro,
    pub detalhe: Option<String>,
    pub senha: Option<OndeASenha>,
    /// O PBX respondeu a ultima sonda em tantos ms (None: ainda nao respondeu).
    pub pbx_ms: Option<u32>,
    /// Quando respondeu pela ultima vez (segundos desde 1970).
    pub pbx_em: Option<u64>,
    /// O PBX parou de responder (duas sondas seguidas).
    pub sem_resposta: bool,
    /// A ultima falha do registro foi de rede (sem resposta), nao recusa.
    pub falha_de_rede: bool,
    /// Desde quando o PBX nao responde (segundos desde 1970).
    pub fora_desde: Option<u64>,
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
    pub gravando: bool,
}

/// Outra entrada que esta captando som, para a tela sugerir.
#[derive(Debug, Clone, Serialize)]
pub struct Alternativa {
    pub id: String,
    pub nome: String,
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
    /// Os nomes dados aos ramais aqui (Ramais, o lapis).
    pub apelidos: std::collections::BTreeMap<String, String>,
    /// Ramal -> livre, ocupado, tocando, offline, desconhecida.
    pub presencas: HashMap<String, &'static str>,
}

/// O que chega na tela.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "kebab-case")]
pub enum Aviso {
    Contas {
        contas: Vec<ContaVista>,
    },
    /// A sonda do PBX de uma conta, a cada resposta (ou falta dela).
    Pbx {
        conta: String,
        #[serde(rename = "pbxMs")]
        pbx_ms: Option<u32>,
        #[serde(rename = "pbxEm")]
        pbx_em: Option<u64>,
        #[serde(rename = "semResposta")]
        sem_resposta: bool,
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
        gravando: bool,
    },
    Espera {
        id: String,
        sim: bool,
    },
    /// O microfone so manda silencio (`silencio`) ou voltou a mandar som. Com
    /// silencio, o diagnostico: mudo no sistema, ou outras entradas captando.
    Microfone {
        id: String,
        silencio: bool,
        mudo: bool,
        alternativas: Vec<Alternativa>,
    },
    /// O teste do microfone (Ajustes): o nivel a cada 100 ms, e o fim.
    Nivel {
        db: f32,
        mudo: bool,
        fim: bool,
        erro: Option<String>,
    },
    Encerrada {
        id: String,
        item: ItemHistorico,
    },
    Mensagem {
        item: ItemConversa,
    },
    Presenca {
        ramal: String,
        estado: &'static str,
    },
    /// "Responder" no aviso da mensagem: a tela abre a conversa.
    AbreConversa {
        ramal: String,
    },
    /// O microfone ou o alto-falante nao abriu: a ligacao segue em silencio
    /// daquele lado. `None` nos dois e "voltou" (trocou o microfone).
    SemAudio {
        id: String,
        microfone: Option<String>,
        #[serde(rename = "altoFalante")]
        alto_falante: Option<String>,
    },
    /// Um erro sem tela de origem (o atender pela notificacao).
    Erro {
        texto: String,
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
    /// As ligacoes saem por ela (tira a marca das outras).
    #[serde(default)]
    pub principal: bool,
    /// Vazio mantem a senha que ja estava.
    pub senha: Option<String>,
}

fn trava<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Telefone {
    pub fn novo(app: AppHandle, guarda: Guarda) -> Arc<Self> {
        let flavor = guarda.preferencias().flavor;
        Arc::new(Self {
            app,
            guarda,
            linhas: Mutex::new(Vec::new()),
            chamadas: Mutex::new(HashMap::new()),
            notificacoes: Notificacoes::nova(flavor),
            medidor: Mutex::new(None),
            tocador: Mutex::new(None),
            testando: std::sync::atomic::AtomicBool::new(false),
            musica: Musica::default(),
            gravacoes: Gravacoes::abre().map_err(|e| warn!("gravacoes: {e}")).ok(),
            presencas: Mutex::new(HashMap::new()),
        })
    }

    pub fn guarda(&self) -> &Guarda {
        &self.guarda
    }

    /// Exclui ligacoes do historico (a tela confirmou) e, se pedido, as
    /// gravacoes delas.
    pub fn exclui_historico(
        &self,
        chaves: &[ChaveHistorico],
        com_gravacoes: bool,
    ) -> Result<Exclusao, String> {
        let sairam = self.guarda.exclui_historico(chaves)?;
        let mut gravacoes = 0;
        if com_gravacoes && let Some(g) = self.gravacoes.as_ref() {
            let soltas = sairam
                .itens
                .iter()
                .filter_map(|i| i.gravacao.as_deref())
                .filter(|a| !sairam.gravacoes_em_uso.contains(*a));
            for a in soltas {
                match g.apaga(a) {
                    Ok(()) => gravacoes += 1,
                    Err(e) => warn!("excluir a gravacao: {e}"),
                }
            }
        }
        info!(
            ligacoes = sairam.itens.len(),
            gravacoes, "historico excluido"
        );
        Ok(Exclusao {
            ligacoes: sairam.itens.len(),
            gravacoes,
        })
    }

    /// Da (ou tira) o nome de um ramal; devolve todos.
    pub fn grava_nome(
        &self,
        ramal: &str,
        nome: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.guarda.grava_nome(ramal, nome)
    }

    /// Assina a presenca dos ramais conhecidos (historico e conversas, os mais
    /// recentes primeiro) pela conta principal, cada um uma vez por `Ua`.
    async fn assina_presencas(self: Arc<Self>, conta: String, novos: Option<Vec<String>>) {
        let (ua, meus, ja) = {
            let linhas = trava(&self.linhas);
            let principal = linhas
                .iter()
                .find(|l| l.conta.principal && l.registro == EstadoRegistro::Registrado)
                .or_else(|| {
                    linhas
                        .iter()
                        .find(|l| l.registro == EstadoRegistro::Registrado)
                });
            let Some(p) = principal.filter(|p| p.conta.id() == conta) else {
                return;
            };
            let meus: HashSet<String> = linhas.iter().map(|l| l.conta.usuario.clone()).collect();
            (p.ua.clone(), meus, p.assinadas.clone())
        };
        let Some(ua) = ua else { return };
        let candidatos = novos.unwrap_or_else(|| {
            let mut quem: Vec<(u64, String)> = self
                .guarda
                .historico()
                .into_iter()
                .map(|h| (h.quando, h.ramal))
                .chain(
                    self.guarda
                        .conversas()
                        .into_iter()
                        .map(|c| (c.quando, c.ramal)),
                )
                .collect();
            quem.sort_by_key(|q| std::cmp::Reverse(q.0));
            quem.into_iter().map(|(_, r)| r).collect()
        });
        let mut vistos = HashSet::new();
        let ramais: Vec<String> = candidatos
            .into_iter()
            .filter(|r| e_ramal(r) && !meus.contains(r) && vistos.insert(r.clone()))
            .take(RAMAIS_ASSINADOS)
            .filter(|r| !ja.contains(r))
            .collect();
        for ramal in ramais {
            if let Some(l) = trava(&self.linhas)
                .iter_mut()
                .find(|l| l.conta.id() == conta)
            {
                l.assinadas.insert(ramal.clone());
            }
            if let Err(e) = ua.assina_presenca(&ramal).await {
                debug!(%ramal, "presenca: {e}");
            }
        }
    }

    /// Um ramal novo (ligou ou mandou mensagem): assina a presenca dele.
    fn assina_novo(self: &Arc<Self>, conta: &str, ramal: &str) {
        let tel = Arc::clone(self);
        let (conta, ramal) = (conta.to_string(), ramal.to_string());
        tauri::async_runtime::spawn(tel.assina_presencas(conta, Some(vec![ramal])));
    }

    /// "Responder" no aviso da mensagem: a tela abre a conversa.
    pub fn abre_conversa(&self, ramal: &str) {
        self.avisa(Aviso::AbreConversa {
            ramal: ramal.to_string(),
        });
    }

    /// A janela esta na frente de quem usa: a mensagem nova nao precisa do
    /// aviso do sistema.
    fn janela_na_frente(&self) -> bool {
        self.app
            .get_webview_window("principal")
            .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
    }

    pub fn gravacoes(&self) -> Result<&Gravacoes, String> {
        self.gravacoes
            .as_ref()
            .ok_or_else(|| "sem pasta para as gravações neste sistema".into())
    }

    /// Apaga as gravacoes mais velhas que a retencao escolhida (ao abrir e a
    /// cada 6 horas). Diminuir a retencao na tela pede confirmacao antes.
    async fn vigia_gravacoes(self: Arc<Self>) {
        loop {
            match (self.gravacoes.as_ref(), self.guarda.retencao_salva()) {
                (Some(g), Some(dias)) => {
                    g.aplica_retencao(dias);
                }
                (Some(_), None) => {
                    warn!("retencao das gravacoes parada: preferencias.toml nao abre ou nao vale");
                }
                (None, _) => {}
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    }

    /// Comeca a gravar a ligacao que acabou de ser atendida.
    fn comeca_gravacao(&self, id: &str) -> bool {
        let Some((inicio, entrada, ramal, nome)) = trava(&self.chamadas)
            .get(id)
            .map(|c| (c.inicio, c.entrada, c.ramal.clone(), c.nome.clone()))
        else {
            return false;
        };
        let resultado = self.gravacoes().and_then(|g| {
            let (arquivo, gravacao) = g.nova(inicio, entrada, &ramal, nome.as_deref())?;
            self.ua_da_chamada(id)?
                .grava(id, Some(Box::new(gravacao)))
                .map_err(|e| e.to_string())?;
            Ok(arquivo)
        });
        match resultado {
            Ok(arquivo) => {
                info!(%id, %arquivo, "gravando");
                if let Some(c) = trava(&self.chamadas).get_mut(id) {
                    c.gravacao = Some(arquivo);
                }
                true
            }
            Err(e) => {
                warn!(%id, "a gravacao nao comecou: {e}");
                self.avisa_erro(format!("A gravação não começou: {e}"));
                false
            }
        }
    }

    /// Le as contas do disco, liga cada uma e fica vigiando a rede.
    pub async fn comeca(self: &Arc<Self>) {
        let tel = Arc::clone(self);
        tauri::async_runtime::spawn(async move { tel.vigia_rede().await });
        let tel = Arc::clone(self);
        tauri::async_runtime::spawn(async move { tel.vigia_gravacoes().await });
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
                pbx: None,
                sem_resposta: 0,
                fora_desde: None,
                assinadas: HashSet::new(),
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

    /// Ha ligacao chegando, tocando ou em andamento.
    pub fn tem_chamada(&self) -> bool {
        !trava(&self.chamadas).is_empty()
    }

    /// Um erro para a tela mostrar, quando nao ha tela esperando a resposta.
    pub fn avisa_erro(&self, texto: String) {
        self.avisa(Aviso::Erro { texto });
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
                principal: l.conta.principal,
                registro: l.registro,
                detalhe: l.detalhe.clone(),
                senha: self.guarda.onde_a_senha(&l.conta.id()),
                pbx_ms: l.pbx.map(|(ms, _)| ms),
                pbx_em: l.pbx.map(|(_, em)| em),
                sem_resposta: l.sem_resposta >= SEM_RESPOSTA,
                falha_de_rede: l.falha_de_rede,
                fora_desde: l.fora_desde,
            })
            .collect()
    }

    /// O historico, sem a gravacao que a retencao ja apagou.
    fn historico(&self) -> Vec<ItemHistorico> {
        let mut h = self.guarda.historico();
        for i in &mut h {
            if i.gravacao
                .as_deref()
                .is_some_and(|a| !self.gravacoes.as_ref().is_some_and(|g| g.existe(a)))
            {
                i.gravacao = None;
            }
        }
        h
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
                gravando: c.gravacao.is_some(),
            });
        Estado {
            contas: self.contas_vistas(),
            chamada,
            historico: self.historico(),
            conversas: self.guarda.conversas(),
            preferencias: self.guarda.preferencias(),
            apelidos: self.guarda.nomes(),
            presencas: trava(&self.presencas)
                .iter()
                .map(|(r, p)| (r.clone(), presenca_da_tela(*p)))
                .collect(),
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
            match registro {
                EstadoRegistro::Registrado => l.fora_desde = None,
                EstadoRegistro::Falhou if falha_de_rede => {
                    l.fora_desde.get_or_insert_with(agora);
                }
                _ => {}
            }
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
            ..Opcoes::default()
        };
        let registra = conta.registra;
        match Ua::inicia(conta, senha, opcoes).await {
            Ok((ua, eventos)) => {
                if let Some(l) = trava(&self.linhas).iter_mut().find(|l| l.conta.id() == id) {
                    l.ua = Some(Arc::clone(&ua));
                    l.assinadas.clear();
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
        if let Some(l) = trava(&self.linhas).iter_mut().find(|l| l.conta.id() == id) {
            l.sem_resposta = 0;
        }
        self.marca_linha(id, EstadoRegistro::Iniciando, None);
        self.liga_conta(conta).await;
    }

    /// A sonda do PBX respondeu (ou nao). Quem sobe a conta de novo quando o
    /// PBX some e o vigia da rede.
    fn resposta_do_pbx(&self, conta: &str, resposta: Option<Duration>) {
        let aviso = {
            let mut linhas = trava(&self.linhas);
            let Some(l) = linhas.iter_mut().find(|l| l.conta.id() == conta) else {
                return;
            };
            match resposta {
                Some(t) => {
                    l.sem_resposta = 0;
                    l.pbx = Some((u32::try_from(t.as_millis()).unwrap_or(u32::MAX), agora()));
                }
                None => l.sem_resposta = l.sem_resposta.saturating_add(1),
            }
            if l.sem_resposta >= SEM_RESPOSTA {
                l.fora_desde.get_or_insert_with(agora);
            }
            Aviso::Pbx {
                conta: conta.to_string(),
                pbx_ms: l.pbx.map(|(ms, _)| ms),
                pbx_em: l.pbx.map(|(_, em)| em),
                sem_resposta: l.sem_resposta >= SEM_RESPOSTA,
            }
        };
        self.avisa(aviso);
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
            let contas: Vec<(String, Option<Arc<Ua>>, bool, bool)> = trava(&self.linhas)
                .iter()
                .map(|l| {
                    let rede = l.registro == EstadoRegistro::Falhou && l.falha_de_rede;
                    // Registrada, mas o PBX nao responde a sonda: o registro
                    // que ele guarda pode nem valer mais.
                    let sumiu =
                        l.registro == EstadoRegistro::Registrado && l.sem_resposta >= SEM_RESPOSTA;
                    (l.conta.id(), l.ua.clone(), rede, sumiu)
                })
                .collect();
            for (id, ua, falha_de_rede, pbx_sumiu) in contas {
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
                    Some(_) if pbx_sumiu => Some("o PBX parou de responder"),
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
        self.notificacoes.fecha_todas();
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
                    // A cada registro (e renovacao): so assina quem falta.
                    let tel = Arc::clone(&self);
                    tauri::async_runtime::spawn(tel.assina_presencas(conta.clone(), None));
                }
                Evento::Registro(Registro::Falhou { codigo, motivo }) => {
                    // 408: o REGISTER ficou sem resposta (o rsipstack responde
                    // isso sozinho quando o PBX some). E falta de rede, nao
                    // recusa: tenta de novo a cada 30 s, como sem codigo.
                    let de_rede = matches!(codigo, None | Some(408));
                    let detalhe = match codigo {
                        Some(408) => "o PBX não respondeu".to_string(),
                        Some(c) => format!("{c} {motivo}"),
                        None => motivo,
                    };
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
                    if !self.janela_na_frente() {
                        let quem = self
                            .guarda
                            .nomes()
                            .remove(&item.ramal)
                            .or_else(|| item.nome.clone());
                        self.notificacoes.mensagem(
                            &self.app,
                            &item.ramal,
                            quem.as_deref(),
                            &item.texto,
                        );
                    }
                    self.assina_novo(&conta, &item.ramal);
                    self.avisa(Aviso::Mensagem { item });
                }
                Evento::Recebendo { id, de, nome } => self.chegando(&conta, id, de, nome).await,
                Evento::Tocando { id } => self.avisa(Aviso::Tocando { id }),
                Evento::Atendida { id, codec } => {
                    self.notificacoes.fecha(&id);
                    self.para_o_toque();
                    if self.guarda.preferencias().pausar_musica {
                        self.musica.pausa();
                    }
                    if let Some(c) = trava(&self.chamadas).get_mut(&id) {
                        c.atendida = true;
                        c.atendida_em = Some(agora());
                    }
                    let gravando = self.guarda.preferencias().gravar && self.comeca_gravacao(&id);
                    let tel = Arc::clone(&self);
                    let vigiada = id.clone();
                    tauri::async_runtime::spawn(async move { tel.vigia_microfone(vigiada).await });
                    let faltando = trava(&self.chamadas)
                        .get(&id)
                        .map(|c| c.faltando.clone())
                        .unwrap_or_default();
                    self.avisa(Aviso::Atendida {
                        id: id.clone(),
                        codec: codec.nome().to_string(),
                        gravando,
                    });
                    if faltando.microfone.is_some() || faltando.alto_falante.is_some() {
                        self.avisa(Aviso::SemAudio {
                            id,
                            microfone: faltando.microfone,
                            alto_falante: faltando.alto_falante,
                        });
                    }
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
                    if c.atendida {
                        self.musica.retoma();
                    }
                    let item = ItemHistorico {
                        quando: c.inicio,
                        conta: c.conta,
                        entrada: c.entrada,
                        ramal: c.ramal,
                        nome: c.nome,
                        atendida: c.atendida,
                        duracao: c.atendida_em.map_or(0, |t| agora().saturating_sub(t)),
                        fim: fim.texto().to_string(),
                        gravacao: c.gravacao,
                    };
                    if let Err(e) = self.guarda.acrescenta_historico(&item) {
                        warn!("historico: {e}");
                    }
                    self.assina_novo(&conta, &item.ramal);
                    self.avisa(Aviso::Encerrada { id, item });
                }
                Evento::TransferenciaFalhou { codigo, .. } => {
                    self.avisa_erro(format!(
                        "A transferência não se completou ({codigo}). A ligação continua com você."
                    ));
                }
                Evento::Pbx { resposta } => self.resposta_do_pbx(&conta, resposta),
                Evento::Presenca { ramal, presenca } => {
                    trava(&self.presencas).insert(ramal.clone(), presenca);
                    self.avisa(Aviso::Presenca {
                        ramal,
                        estado: presenca_da_tela(presenca),
                    });
                }
                Evento::PresencaEncerrada { ramal } => {
                    // O PBX encerrou: na proxima renovacao do registro, assina de novo.
                    if let Some(l) = trava(&self.linhas)
                        .iter_mut()
                        .find(|l| l.conta.id() == conta)
                    {
                        l.assinadas.remove(&ramal);
                    }
                    trava(&self.presencas).insert(ramal.clone(), Presenca::Desconhecida);
                    self.avisa(Aviso::Presenca {
                        ramal,
                        estado: presenca_da_tela(Presenca::Desconhecida),
                    });
                }
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
                let (mudo, alternativas) = if silencio {
                    self.diagnostica_microfone().await
                } else {
                    (false, Vec::new())
                };
                self.avisa(Aviso::Microfone {
                    id: id.clone(),
                    silencio,
                    mudo,
                    alternativas,
                });
            }
        }
    }

    /// Por que nao ha som: primeiro, a entrada esta muda no sistema? Se nao,
    /// outra entrada esta captando? (so quando a principal nao capta.)
    async fn diagnostica_microfone(&self) -> (bool, Vec<Alternativa>) {
        let microfone = self.guarda.preferencias().microfone;
        let feito = tauri::async_runtime::spawn_blocking(move || {
            if som::diagnostico::muda_no_sistema(microfone.as_deref()) == Some(true) {
                return (true, Vec::new());
            }
            let outras = som::diagnostico::outras_captando(
                microfone.as_deref(),
                Duration::from_millis(1500),
            );
            let alternativas = outras
                .into_iter()
                .map(|c| Alternativa {
                    id: c.id,
                    nome: c.nome,
                })
                .collect();
            (false, alternativas)
        })
        .await;
        feito.unwrap_or_default()
    }

    /// "Tirar do mudo", no aviso ou no teste do microfone.
    pub fn tira_do_mudo(&self) -> Result<(), String> {
        let microfone = self.guarda.preferencias().microfone;
        som::diagnostico::tira_do_mudo(microfone.as_deref()).map_err(|e| e.to_string())
    }

    /// O teste do microfone das configuracoes: 10 s mandando o nivel para a
    /// tela (a cada 100 ms), com o aviso de mudo no sistema.
    pub fn testa_microfone(self: &Arc<Self>, microfone: Option<String>) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        if !trava(&self.chamadas).is_empty() {
            return Err("durante a ligação, não".into());
        }
        if self.testando.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let tel = Arc::clone(self);
        std::thread::spawn(move || {
            let m = microfone.as_deref();
            let mudo = som::diagnostico::muda_no_sistema(m) == Some(true);
            let mut erro = None;
            match som::Microfone::abre(m) {
                Ok(mut mic) => {
                    let fim = Instant::now() + Duration::from_secs(10);
                    let mut quadro = [0i16; motor::midia::AMOSTRAS];
                    let (mut maior, mut n) = (-120.0f32, 0u32);
                    while Instant::now() < fim && tel.testando.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(20));
                        mic.le(&mut quadro);
                        maior = maior.max(som::diagnostico::nivel_db(&quadro));
                        n += 1;
                        if n % 5 == 0 {
                            tel.avisa(Aviso::Nivel {
                                db: maior,
                                mudo,
                                fim: false,
                                erro: None,
                            });
                            maior = -120.0;
                        }
                    }
                }
                Err(e) => erro = Some(e.to_string()),
            }
            tel.testando.store(false, Ordering::SeqCst);
            tel.avisa(Aviso::Nivel {
                db: -120.0,
                mudo,
                fim: true,
                erro,
            });
        });
        Ok(())
    }

    pub fn para_teste_microfone(&self) {
        self.testando
            .store(false, std::sync::atomic::Ordering::SeqCst);
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
                faltando: Faltando::default(),
                gravacao: None,
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
        self.notificacoes.chegando(&self.app, &id, &quem, &ramal);
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
        // A pedida; senao a principal; senao a primeira registrada; senao a
        // primeira no ar.
        let escolhida = match conta {
            Some(id) => linhas.iter().find(|l| l.conta.id() == id),
            None => linhas
                .iter()
                .find(|l| l.conta.principal && l.ua.is_some())
                .or_else(|| {
                    linhas
                        .iter()
                        .find(|l| l.registro == EstadoRegistro::Registrado && l.ua.is_some())
                })
                .or_else(|| linhas.iter().find(|l| l.ua.is_some())),
        };
        let l = escolhida.ok_or("nenhuma conta no ar: cadastre uma nas configurações")?;
        let ua = l.ua.clone().ok_or("essa conta não está no ar")?;
        Ok((l.conta.id(), ua))
    }

    fn audio(&self) -> Audios {
        let p = self.guarda.preferencias();
        let mut faltando = Faltando::default();
        let (fonte, medidor): (Box<dyn Fonte>, _) =
            match som::Microfone::abre(p.microfone.as_deref()) {
                Ok(mic) => {
                    let medidor = mic.medidor();
                    (Box::new(mic), Some(medidor))
                }
                Err(e) => {
                    warn!("a ligacao segue sem microfone: {e}");
                    faltando.microfone = Some(e.to_string());
                    (Box::new(Silencio), None)
                }
            };
        let destino: Box<dyn Destino> = match som::AltoFalante::abre(p.alto_falante.as_deref()) {
            Ok(fone) => Box::new(fone),
            Err(e) => {
                warn!("a ligacao segue sem alto-falante: {e}");
                faltando.alto_falante = Some(e.to_string());
                Box::new(Silencio)
            }
        };
        Audios {
            fonte,
            destino,
            medidor,
            faltando,
        }
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
        let voltou = trava(&self.chamadas).get_mut(&id).and_then(|c| {
            c.faltando
                .microfone
                .take()
                .map(|_| c.faltando.alto_falante.clone())
        });
        if let Some(alto_falante) = voltou {
            self.avisa(Aviso::SemAudio {
                id: id.clone(),
                microfone: None,
                alto_falante,
            });
        }
        let mut p = self.guarda.preferencias();
        p.microfone = microfone;
        self.guarda.grava_preferencias(&p)
    }

    pub async fn liga(&self, destino: &str, conta: Option<&str>) -> Result<String, String> {
        if !trava(&self.chamadas).is_empty() {
            return Err("já tem uma ligação: desligue antes de ligar de novo".into());
        }
        let (conta, ua) = self.ua_para_ligar(conta)?;
        let a = self.audio();
        let id = ua
            .liga(destino, a.fonte, a.destino)
            .await
            .map_err(|e| e.to_string())?;
        *trava(&self.medidor) = a.medidor;
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
                faltando: a.faltando,
                gravacao: None,
            },
        );
        Ok(id)
    }

    pub async fn atende(&self, id: &str) -> Result<(), String> {
        let ua = self.ua_da_chamada(id)?;
        // O toque sai antes de abrir o audio da ligacao (pode ser a mesma saida).
        self.para_o_toque();
        let a = self.audio();
        // Antes de atender: o motor avisa "atendida" logo, e o aviso do que
        // faltou sai junto com ele.
        if let Some(c) = trava(&self.chamadas).get_mut(id) {
            c.faltando = a.faltando;
        }
        ua.atende(id, a.fonte, a.destino)
            .await
            .map_err(|e| e.to_string())?;
        *trava(&self.medidor) = a.medidor;
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
        conta.principal = f.principal;
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
            pbx: None,
            sem_resposta: 0,
            fora_desde: None,
            assinadas: HashSet::new(),
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
            if conta.principal {
                for l in linhas.iter_mut().filter(|l| l.conta.id() != id) {
                    l.conta.principal = false;
                }
            }
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
