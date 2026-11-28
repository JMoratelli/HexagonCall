//! Uma conta SIP em funcionamento: registro, mensagens, presenca e ligacoes.
//!
//! O rsipstack cuida das transacoes, da autenticacao e dos dialogos de
//! INVITE. O que o PBX de hoje pede e a biblioteca nao traz pronto fica aqui:
//! MESSAGE e SUBSCRIBE fora de dialogo (com a segunda tentativa autenticada),
//! o NOTIFY da presenca e o CRLF de sinal de vida, sem o qual o firewall da
//! maquina barra os avisos do PBX depois de 2 minutos (bancada de 26/11/2026).

use crate::erro::Erro;
use crate::midia::{Destino, Envio, Estatistica, Fonte, Gravador, Sessao};
use crate::rede;
use nucleo::conta::Conta;
use nucleo::ligacao::{Fim, atendida_em_outro_lugar, codigo_do_sipfrag, fim_por_status};
use nucleo::presenca::{self, Presenca};
use nucleo::sdp::{self, Codec, Direcao, Local, NOSSOS_CODECS, Remota, TIPO_DTMF};
use rsipstack::EndpointBuilder;
use rsipstack::dialog::authenticate::{Credential, handle_client_authenticate};
use rsipstack::dialog::dialog::{
    DialogState, DialogStateReceiver, DialogStateSender, TerminatedReason, TransactionHandle,
};
use rsipstack::dialog::dialog_layer::DialogLayer;
use rsipstack::dialog::invitation::{InviteAsyncResult, InviteOption};
use rsipstack::dialog::invite_dialog::InviteDialog;
use rsipstack::dialog::registration::{Registration, backoff_interval, refresh_interval};
use rsipstack::sip::prelude::*;
use rsipstack::sip::{
    self as sip, Header, Method, Request, Response, SipMessage, StatusCode, StatusCodeKind, Uri,
};
use rsipstack::transaction::endpoint::{EndpointInnerRef, EndpointOption, MessageInspector};
use rsipstack::transaction::key::{TransactionKey, TransactionRole};
use rsipstack::transaction::{Endpoint, Transaction, TransactionReceiver, make_tag};
use rsipstack::transport::udp::UdpConnection;
use rsipstack::transport::{SipAddr, TransportLayer};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const AGENTE: &str = concat!("Hexagon Call/", env!("CARGO_PKG_VERSION"));
const EXPIRA_PRESENCA: u32 = 600;
/// O menor intervalo entre dois registros pedidos pelo offline do PBX: o PBX que
/// insiste no offline (a sonda dele nao chega aqui) nao vira enxurrada de REGISTER.
const ENTRE_REGISTROS_FORCADOS: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Registro {
    Registrado { expira: u32 },
    Falhou { codigo: Option<u16>, motivo: String },
    Desregistrado,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sentido {
    Saida,
    Entrada,
}

#[derive(Debug, Clone)]
pub enum Evento {
    Registro(Registro),
    Mensagem {
        de: String,
        nome: Option<String>,
        texto: String,
    },
    Presenca {
        ramal: String,
        presenca: Presenca,
    },
    /// O PBX encerrou uma assinatura de presenca (ou ela venceu).
    PresencaEncerrada {
        ramal: String,
    },
    /// Alguem esta ligando para esta conta.
    Recebendo {
        id: String,
        de: String,
        nome: Option<String>,
    },
    /// A ligacao que fizemos esta tocando do outro lado (180/183).
    Tocando {
        id: String,
        /// A resposta que disse que tocava: 180 (toca de verdade) ou 183 (o
        /// PBX ja manda audio, que pode ser so um recado).
        codigo: u16,
    },
    Atendida {
        id: String,
        codec: Codec,
    },
    /// O outro lado pos a ligacao em espera (`sim`) ou tirou dela.
    Espera {
        id: String,
        sim: bool,
    },
    Encerrada {
        id: String,
        fim: Fim,
        estatistica: Option<Estatistica>,
    },
    /// A sonda do PBX (OPTIONS): respondeu, em quanto tempo, ou nao respondeu
    /// (`None`). Vem a cada `Sonda::intervalo`, e mais cedo depois de uma falha.
    Pbx {
        resposta: Option<Duration>,
    },
    /// O PBX aceitou a transferencia, mas ela nao se completou (o NOTIFY
    /// trouxe uma falha final): a ligacao segue com quem estava.
    TransferenciaFalhou {
        id: String,
        codigo: u16,
    },
}

/// A sonda do PBX: um OPTIONS de tempos em tempos, como o qualify do Asterisk.
/// O sinal de vida (CRLF) so mantem o caminho aberto e nao tem resposta; a
/// sonda e o que diz se o PBX esta la.
#[derive(Debug, Clone, Copy)]
pub struct Sonda {
    pub intervalo: Duration,
    /// Quanto esperar a resposta.
    pub espera: Duration,
    /// Depois de uma sem resposta, a proxima vem mais cedo, para confirmar.
    pub reconfere: Duration,
}

impl Default for Sonda {
    fn default() -> Self {
        Self {
            intervalo: Duration::from_secs(30),
            espera: Duration::from_secs(5),
            reconfere: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Opcoes {
    /// Porta SIP local; 0 deixa o sistema escolher (cada conta precisa da sua).
    pub porta_sip: u16,
    /// Registrar a conta (para receber ligacoes e mensagens). A bancada desliga
    /// para testar sem tirar o ramal de outro aparelho: o chan_sip guarda um
    /// contato so por ramal.
    pub registrar: bool,
    /// None: sem sonda.
    pub sonda: Option<Sonda>,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            porta_sip: 0,
            registrar: true,
            sonda: Some(Sonda::default()),
        }
    }
}

struct Ligacao {
    sentido: Sentido,
    dialogo: InviteDialog,
    /// Para uma ligacao de saida: o socket reservado, o SDP oferecido e o
    /// audio, que so comecam quando o outro lado atende.
    rtp: Option<tokio::net::UdpSocket>,
    audio: Option<(Box<dyn Fonte>, Box<dyn Destino>)>,
    /// Para uma de entrada: a oferta que chegou no INVITE.
    oferta: Option<Remota>,
    sessao: Option<Sessao>,
    /// O nosso SDP; existe desde a oferta (saida) ou o atendimento (entrada).
    midia: Option<Midia>,
    atendida: bool,
    tocando: bool,
    /// Fomos nos que desligamos, cancelamos ou recusamos.
    por_nos: bool,
    encerrada: bool,
    /// O PBX aceitou a transferencia: o BYE que vier e o fim dela.
    transferida: bool,
}

/// O que o nosso SDP disse nesta ligacao. Uma renegociacao responde com o
/// mesmo `o=`, e a versao so sobe quando algo muda (RFC 3264, secao 8).
struct Midia {
    sessao_sdp: u64,
    versao: u64,
    local: Local,
    /// O codec combinado; numa ligacao de saida, so depois do atendimento.
    codec: Option<Codec>,
    /// Nos pusemos a ligacao em espera.
    em_espera: bool,
    /// O outro lado nos pos em espera.
    deles_em_espera: bool,
}

impl Midia {
    fn nova(local: Local, codec: Option<Codec>) -> (Self, String) {
        let sessao_sdp = rand::random::<u32>().into();
        let corpo = sdp::monta(sessao_sdp, 1, &local);
        let midia = Self {
            sessao_sdp,
            versao: 1,
            local,
            codec,
            em_espera: false,
            deles_em_espera: false,
        };
        (midia, corpo)
    }

    fn sdp(&self) -> String {
        sdp::monta(self.sessao_sdp, self.versao, &self.local)
    }

    /// O que mandar, pela direcao combinada e pela nossa espera.
    fn envio(&self, podemos_mandar: bool) -> Envio {
        if !podemos_mandar {
            Envio::Nada
        } else if self.em_espera {
            Envio::Silencio
        } else {
            Envio::Audio
        }
    }
}

/// Para onde mandar, pelo SDP do outro lado. `c=0.0.0.0` (a espera antiga) e
/// porta zero nao sao endereco: fica o de antes.
fn endereco(remota: &Remota) -> Option<SocketAddr> {
    (!remota.ip.is_unspecified() && remota.porta != 0)
        .then(|| SocketAddr::new(remota.ip, remota.porta))
}

/// O codec da renegociacao: o de agora, se ainda vier na oferta.
fn codec_da_renegociacao(remota: &Remota, atual: Option<Codec>) -> Result<Codec, Erro> {
    let atual: Vec<Codec> = atual.into_iter().collect();
    Ok(sdp::escolhe(remota, &atual).or_else(|_| sdp::escolhe(remota, &NOSSOS_CODECS))?)
}

struct Assinatura {
    ramal: String,
    uri: Uri,
    call_id: String,
    de_tag: String,
    para_tag: Option<String>,
    /// A do proprio ramal, que o motor faz sozinho (`assina_propria`): nao vai
    /// para a tela.
    propria: bool,
}

pub struct Ua {
    conta: Conta,
    credencial: Credential,
    _endpoint: Endpoint,
    endpoint: EndpointInnerRef,
    dialogos: Arc<DialogLayer>,
    estados: DialogStateSender,
    eventos: mpsc::UnboundedSender<Evento>,
    udp: UdpConnection,
    servidor: SipAddr,
    /// O PBX resolvido: para ver se o caminho ate ele mudou (outra rede).
    pbx: SocketAddr,
    ip: IpAddr,
    aor: Uri,
    contato: Uri,
    cancela: CancellationToken,
    cancela_registro: CancellationToken,
    tarefa_registro: Mutex<Option<JoinHandle<()>>>,
    ligacoes: Mutex<HashMap<String, Ligacao>>,
    assinaturas: Mutex<HashMap<String, Assinatura>>,
    cseq: AtomicU32,
    registrado: AtomicBool,
    /// O PBX deu o proprio ramal como offline: registrar de novo ja.
    registra_agora: tokio::sync::Notify,
    /// Call-IDs das ligacoes chegando que outro aparelho atendeu (Inspetor).
    em_outro: Arc<Mutex<HashSet<String>>>,
}

/// Olha o que chega antes das transacoes. O rsipstack responde ao CANCEL de
/// uma ligacao chegando sozinho, e o dialogo so diz que cancelaram: o Reason
/// que conta que outro aparelho atendeu (grupo de ramais, fila) e lido aqui e
/// fica guardado pelo Call-ID ate a ligacao acabar.
struct Inspetor {
    em_outro: Arc<Mutex<HashSet<String>>>,
}

impl MessageInspector for Inspetor {
    fn before_send(&self, msg: SipMessage, _: Option<&SipAddr>) -> SipMessage {
        msg
    }

    fn after_received(&self, msg: SipMessage, _: Option<&SipAddr>) -> SipMessage {
        if let SipMessage::Request(req) = &msg
            && req.method == Method::Cancel
            && valores_de(&req.headers, "Reason").any(|v| atendida_em_outro_lugar(&v))
            && let Ok(call_id) = req.call_id_header()
        {
            let mut em_outro = trava(&self.em_outro);
            // CANCEL de ligacao que nunca chegou aqui nao sai mais: nao cresce.
            if em_outro.len() >= 64 {
                em_outro.clear();
            }
            em_outro.insert(call_id.value().to_string());
        }
        msg
    }
}

/// Um Mutex envenenado (panico com ele travado) segue utilizavel: o estado
/// aqui e de uma ligacao, e perder a ligacao e melhor que derrubar o motor.
fn trava<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O valor de um cabecalho pelo nome, sem depender de a biblioteca ter tipo
/// para ele (Subscription-State, Expires).
fn valor_de(cabecalhos: &sip::Headers, nome: &str) -> Option<String> {
    valores_de(cabecalhos, nome).next()
}

/// Todos os valores de um cabecalho que pode vir repetido.
fn valores_de<'a>(
    cabecalhos: &'a sip::Headers,
    nome: &'a str,
) -> impl Iterator<Item = String> + 'a {
    cabecalhos.iter().filter_map(move |h| {
        let texto = h.to_string();
        let (n, v) = texto.split_once(':')?;
        n.trim()
            .eq_ignore_ascii_case(nome)
            .then(|| v.trim().to_string())
    })
}

/// Quem mandou: o ramal (parte do usuario) e o nome, se veio.
fn quem(req: &Request) -> (String, Option<String>) {
    match req.from_header().and_then(|f| f.typed()) {
        Ok(de) => {
            let ramal = de
                .uri
                .auth
                .as_ref()
                .map_or_else(|| de.uri.to_string(), |a| a.user.clone());
            let nome = de
                .display_name
                .map(|n| n.trim_matches('"').to_string())
                .filter(|n| !n.is_empty());
            (ramal, nome)
        }
        Err(_) => (String::from("?"), None),
    }
}

fn codigo(resp: &Response) -> u16 {
    resp.status_code.code()
}

fn ok_ou_erro(resp: &Response) -> Result<(), Erro> {
    if resp.status_code.kind() == StatusCodeKind::Successful {
        Ok(())
    } else {
        Err(Erro::Resposta {
            codigo: codigo(resp),
            motivo: resp.status_code.to_string(),
        })
    }
}

struct Pedido {
    metodo: Method,
    uri: Uri,
    cabecalhos: Vec<Header>,
    corpo: Vec<u8>,
    call_id: Option<String>,
    de_tag: Option<String>,
    para_tag: Option<String>,
}

impl Pedido {
    fn novo(metodo: Method, uri: Uri) -> Self {
        Self {
            metodo,
            uri,
            cabecalhos: Vec::new(),
            corpo: Vec::new(),
            call_id: None,
            de_tag: None,
            para_tag: None,
        }
    }
}

impl Ua {
    /// Liga a conta: abre a porta SIP, sobe o endpoint e as tarefas de fundo.
    pub async fn inicia(
        conta: Conta,
        senha: String,
        opcoes: Opcoes,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Evento>), Erro> {
        conta.valida()?;
        let destino = rede::resolve(&conta.dominio, conta.porta).await?;
        let ip = rede::ip_local_para(destino)?;
        let cancela = CancellationToken::new();

        let udp = UdpConnection::create_connection(
            SocketAddr::new(ip, opcoes.porta_sip),
            None,
            Some(cancela.child_token()),
        )
        .await?;
        let local = udp.get_addr().get_socketaddr()?;
        let transporte = TransportLayer::new(cancela.clone());
        transporte.add_transport(udp.clone().into());
        // O Call-ID leva o nosso IP (o padrao da biblioteca e o dominio dela).
        let opcao = EndpointOption {
            callid_suffix: Some(ip.to_string()),
            ..Default::default()
        };
        let em_outro = Arc::new(Mutex::new(HashSet::new()));
        let inspetor = Inspetor {
            em_outro: Arc::clone(&em_outro),
        };
        let endpoint = EndpointBuilder::new()
            .with_option(opcao)
            .with_cancel_token(cancela.clone())
            .with_transport_layer(transporte)
            .with_user_agent(AGENTE)
            .with_inspector(Box::new(inspetor))
            .build();
        let entrada = endpoint.incoming_transactions()?;
        let dialogos = Arc::new(DialogLayer::new(endpoint.inner.clone()));
        let (estados, estados_rx) = dialogos.new_dialog_state_channel();
        let (eventos, eventos_rx) = mpsc::unbounded_channel();

        let aor = Uri::try_from(conta.aor().as_str())?;
        let contato = Uri {
            scheme: Some(sip::Scheme::Sip),
            auth: Some(sip::Auth {
                user: conta.usuario.clone(),
                password: None,
            }),
            host_with_port: local.into(),
            params: vec![],
            headers: vec![],
        };
        let credencial = Credential {
            username: conta.usuario.clone(),
            password: senha,
            realm: None,
            auth_username: conta.usuario_auth.clone(),
        };
        info!(conta = %conta.id(), %local, pbx = %destino, "conta ligada");

        let ua = Arc::new(Self {
            credencial,
            endpoint: endpoint.inner.clone(),
            _endpoint: endpoint,
            dialogos,
            estados,
            eventos,
            udp,
            servidor: SipAddr::from(destino),
            pbx: destino,
            ip,
            aor,
            contato,
            cancela_registro: cancela.child_token(),
            cancela,
            tarefa_registro: Mutex::new(None),
            ligacoes: Mutex::new(HashMap::new()),
            assinaturas: Mutex::new(HashMap::new()),
            cseq: AtomicU32::new(rand::random::<u32>() % 10_000 + 1),
            registrado: AtomicBool::new(false),
            registra_agora: tokio::sync::Notify::new(),
            em_outro,
            conta,
        });

        let servidor = Arc::clone(&ua.endpoint);
        tokio::spawn(async move {
            if let Err(e) = servidor.serve().await {
                warn!("endpoint parou: {e}");
            }
        });
        tokio::spawn(Arc::clone(&ua).laco_entrada(entrada));
        tokio::spawn(Arc::clone(&ua).laco_estados(estados_rx));
        tokio::spawn(Arc::clone(&ua).laco_sinal_de_vida());
        if let Some(sonda) = opcoes.sonda {
            tokio::spawn(Arc::clone(&ua).laco_sonda(sonda));
        }
        if opcoes.registrar && ua.conta.registra {
            let tarefa = tokio::spawn(Arc::clone(&ua).laco_registro());
            *trava(&ua.tarefa_registro) = Some(tarefa);
        }
        Ok((ua, eventos_rx))
    }

    pub fn conta(&self) -> &Conta {
        &self.conta
    }

    /// O endereco local (IP e porta SIP) desta conta.
    pub fn endereco(&self) -> String {
        self.contato.host_with_port.to_string()
    }

    /// O IP local com que esta conta se anunciou ao PBX (o do Contact).
    pub fn ip(&self) -> IpAddr {
        self.ip
    }

    /// O IP que o sistema usaria agora para chegar ao PBX (None: sem rota,
    /// rede fora). Diferente de `ip()`, a rede mudou e a conta tem de subir de
    /// novo. Nao manda pacote nenhum.
    pub fn ip_para_o_pbx(&self) -> Option<IpAddr> {
        rede::ip_local_para(self.pbx).ok()
    }

    /// Ha ligacao em andamento ou chegando.
    pub fn em_ligacao(&self) -> bool {
        trava(&self.ligacoes).values().any(|l| !l.encerrada)
    }

    fn avisa(&self, evento: Evento) {
        // Ninguem escutando (a janela fechou) nao e erro do motor.
        let _ = self.eventos.send(evento);
    }

    fn proximo_cseq(&self) -> u32 {
        self.cseq.fetch_add(1, Ordering::Relaxed)
    }

    fn uri(&self, destino: &str) -> Result<Uri, Erro> {
        Ok(Uri::try_from(self.conta.uri_de(destino)?.as_str())?)
    }

    /// Um pedido fora de dialogo (ou num dialogo nosso, como a renovacao da
    /// presenca), com uma segunda tentativa se o PBX pedir autenticacao.
    async fn envia(&self, p: Pedido) -> Result<Response, Erro> {
        let via = self.endpoint.get_via(None, None)?;
        let de_tag = p.de_tag.map_or_else(make_tag, sip::param::Tag::new);
        let de = sip::typed::From {
            display_name: self.conta.nome.clone(),
            uri: self.aor.clone(),
            params: vec![],
        }
        .with_tag(de_tag);
        let mut para = sip::typed::To {
            display_name: None,
            uri: p.uri.clone(),
            params: vec![],
        };
        if let Some(tag) = p.para_tag {
            para = para.with_tag(sip::param::Tag::new(tag));
        }
        let mut pedido = self.endpoint.make_request(
            p.metodo,
            p.uri,
            via,
            de,
            para,
            self.proximo_cseq(),
            p.call_id.map(sip::headers::CallId::new),
        );
        for h in p.cabecalhos {
            pedido.headers.unique_push(h);
        }
        let tamanho = u32::try_from(p.corpo.len()).unwrap_or(u32::MAX);
        pedido
            .headers
            .unique_push(Header::ContentLength(tamanho.into()));
        pedido.body = p.corpo;

        let chave = TransactionKey::from_request(&pedido, TransactionRole::Client)?;
        let mut tx = Transaction::new_client(chave, pedido, self.endpoint.clone(), None);
        tx.destination = Some(self.servidor.clone());
        tx.send().await?;
        let mut autenticou = false;
        while let Some(msg) = tx.receive().await {
            let SipMessage::Response(resp) = msg else {
                continue;
            };
            if resp.status_code.kind() == StatusCodeKind::Provisional {
                continue;
            }
            let desafio = matches!(
                resp.status_code,
                StatusCode::Unauthorized | StatusCode::ProxyAuthenticationRequired
            );
            if desafio && !autenticou {
                autenticou = true;
                tx = handle_client_authenticate(self.proximo_cseq(), &tx, resp, &self.credencial)
                    .await?;
                tx.destination = Some(self.servidor.clone());
                tx.send().await?;
                continue;
            }
            return Ok(resp);
        }
        Err(Erro::SemResposta)
    }

    // ---- O PBX esta la? ----

    /// Pergunta ao PBX se ele esta la (OPTIONS) e devolve quanto tempo levou.
    /// Qualquer resposta vale, ate um 404: o caminho ate ele funciona.
    pub async fn sonda(&self, espera: Duration) -> Result<Duration, Erro> {
        let uri = Uri::try_from(self.conta.servidor().as_str())?;
        let inicio = Instant::now();
        match tokio::time::timeout(espera, self.envia(Pedido::novo(Method::Options, uri))).await {
            Ok(Ok(_)) => Ok(inicio.elapsed()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Erro::SemResposta),
        }
    }

    async fn laco_sonda(self: Arc<Self>, sonda: Sonda) {
        let mut espera = Duration::ZERO;
        loop {
            let resposta = tokio::select! {
                () = self.cancela.cancelled() => return,
                r = async {
                    tokio::time::sleep(espera).await;
                    self.sonda(sonda.espera).await
                } => r,
            };
            if let Err(e) = &resposta {
                debug!("o PBX nao respondeu a sonda: {e}");
            }
            let resposta = resposta.ok();
            espera = if resposta.is_some() {
                sonda.intervalo
            } else {
                sonda.reconfere
            };
            self.avisa(Evento::Pbx { resposta });
        }
    }

    // ---- Mensagens ----

    /// Manda uma mensagem (SIP MESSAGE). O Ok so quer dizer que o PBX aceitou
    /// (202): o SIP nao confirma entrega nem leitura.
    pub async fn mensagem(&self, destino: &str, texto: &str) -> Result<(), Erro> {
        let mut p = Pedido::novo(Method::Message, self.uri(destino)?);
        p.cabecalhos
            .push(Header::ContentType("text/plain;charset=UTF-8".into()));
        p.corpo = texto.as_bytes().to_vec();
        let resp = self.envia(p).await?;
        ok_ou_erro(&resp)
    }

    async fn recebe_mensagem(&self, mut tx: Transaction) -> Result<(), Erro> {
        let (de, nome) = quem(&tx.original);
        let texto = String::from_utf8_lossy(&tx.original.body).into_owned();
        tx.reply(StatusCode::OK).await?;
        self.avisa(Evento::Mensagem { de, nome, texto });
        Ok(())
    }

    // ---- Presenca ----

    /// Assina a presenca de um ramal. O estado chega como Evento::Presenca, a
    /// primeira vez logo depois do 200 e de novo a cada mudanca.
    pub async fn assina_presenca(self: &Arc<Self>, ramal: &str) -> Result<(), Erro> {
        let (call_id, expira) = self.nova_assinatura(ramal, false).await?;
        tokio::spawn(Arc::clone(self).renova_presenca(ramal.to_string(), call_id, expira, false));
        Ok(())
    }

    /// A presenca do proprio ramal, assinada depois do primeiro registro. O
    /// chan_sip guarda um contato por ramal: outro aparelho do mesmo ramal que
    /// sai (Expires 0) apaga o registro deste, e o PBX passa a dar o ramal como
    /// offline (480 para quem liga) enquanto a conta se acha registrada. Visto
    /// num PC com Windows em 27/11/2026: fechar e abrir o app resolveu. Com o
    /// offline daqui, a conta registra de novo na hora, sem esperar a renovacao
    /// (75% do prazo: 7,5 min com os 600 s de sempre). PBX sem hint para o ramal
    /// recusa o SUBSCRIBE, e fica so a renovacao.
    async fn assina_propria(self: Arc<Self>) {
        let ramal = self.conta.usuario.clone();
        match self.nova_assinatura(&ramal, true).await {
            Ok((call_id, expira)) => self.renova_presenca(ramal, call_id, expira, true).await,
            Err(e) => debug!("sem a presenca do proprio ramal: {e}"),
        }
    }

    /// O SUBSCRIBE inicial. Devolve o Call-ID da assinatura e o prazo dado.
    async fn nova_assinatura(&self, ramal: &str, propria: bool) -> Result<(String, u32), Erro> {
        let uri = self.uri(ramal)?;
        let call_id = format!("{:016x}@{}", rand::random::<u64>(), self.ip);
        let de_tag = make_tag().value().to_string();
        // Antes de mandar: o NOTIFY pode chegar antes de o 200 ser tratado.
        trava(&self.assinaturas).insert(
            call_id.clone(),
            Assinatura {
                ramal: ramal.to_string(),
                uri: uri.clone(),
                call_id: call_id.clone(),
                de_tag: de_tag.clone(),
                para_tag: None,
                propria,
            },
        );
        let resp = match self
            .subscribe(uri, &call_id, &de_tag, None, EXPIRA_PRESENCA)
            .await
        {
            Ok(r) if r.status_code.kind() == StatusCodeKind::Successful => r,
            outro => {
                trava(&self.assinaturas).remove(&call_id);
                return Err(match outro {
                    Ok(r) => ok_ou_erro(&r).err().unwrap_or(Erro::SemResposta),
                    Err(e) => e,
                });
            }
        };
        let para_tag = resp
            .to_header()
            .ok()
            .and_then(|t| t.tag().ok().flatten())
            .map(|t| t.value().to_string());
        let expira = valor_de(&resp.headers, "Expires")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(EXPIRA_PRESENCA);
        if let Some(a) = trava(&self.assinaturas).get_mut(&call_id) {
            a.para_tag = para_tag;
        }
        Ok((call_id, expira))
    }

    async fn subscribe(
        &self,
        uri: Uri,
        call_id: &str,
        de_tag: &str,
        para_tag: Option<String>,
        expira: u32,
    ) -> Result<Response, Erro> {
        let mut p = Pedido::novo(Method::Subscribe, uri);
        p.call_id = Some(call_id.to_string());
        p.de_tag = Some(de_tag.to_string());
        p.para_tag = para_tag;
        p.cabecalhos = vec![
            Header::Event("presence".into()),
            Header::Accept("application/pidf+xml".into()),
            Header::Expires(expira.into()),
            Header::Contact(format!("<{}>", self.contato).into()),
        ];
        self.envia(p).await
    }

    /// Renova a assinatura a 75% do prazo. Se o PBX esqueceu dela (481, como
    /// depois de um NOTIFY sem resposta), assina de novo do zero.
    async fn renova_presenca(
        self: Arc<Self>,
        ramal: String,
        mut call_id: String,
        expira: u32,
        propria: bool,
    ) {
        let tres_quartos = |s: u32| Duration::from_secs(u64::from(s) * 3 / 4);
        let mut espera = tres_quartos(expira);
        loop {
            tokio::select! {
                () = self.cancela.cancelled() => return,
                () = tokio::time::sleep(espera) => {}
            }
            let dados = trava(&self.assinaturas)
                .get(&call_id)
                .map(|a| (a.uri.clone(), a.de_tag.clone(), a.para_tag.clone()));
            // Sumiu do mapa: cancelada ou encerrada pelo PBX.
            let Some((uri, de_tag, para_tag)) = dados else {
                return;
            };
            match self
                .subscribe(uri, &call_id, &de_tag, para_tag, EXPIRA_PRESENCA)
                .await
            {
                Ok(r) if r.status_code.kind() == StatusCodeKind::Successful => {
                    espera = tres_quartos(EXPIRA_PRESENCA);
                }
                resultado => {
                    debug!(%ramal, ?resultado, "renovacao da presenca falhou; assinando de novo");
                    trava(&self.assinaturas).remove(&call_id);
                    match self.nova_assinatura(&ramal, propria).await {
                        Ok((novo, prazo)) => {
                            call_id = novo;
                            espera = tres_quartos(prazo);
                        }
                        Err(e) => {
                            warn!(%ramal, "nao consegui assinar a presenca de novo: {e}");
                            if !propria {
                                self.avisa(Evento::PresencaEncerrada { ramal });
                            }
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Cancela a assinatura que a tela pediu para o ramal (a do proprio ramal,
    /// do motor, segue).
    pub async fn cancela_presenca(&self, ramal: &str) -> Result<(), Erro> {
        let assinatura = {
            let mut lista = trava(&self.assinaturas);
            let chave = lista
                .iter()
                .find(|(_, a)| a.ramal == ramal && !a.propria)
                .map(|(k, _)| k.clone());
            chave.and_then(|k| lista.remove(&k))
        };
        match assinatura {
            Some(a) => self.encerra_assinatura(a).await,
            None => Ok(()),
        }
    }

    /// O SUBSCRIBE com Expires 0 de uma assinatura ja tirada do mapa.
    async fn encerra_assinatura(&self, a: Assinatura) -> Result<(), Erro> {
        let resp = self
            .subscribe(a.uri, &a.call_id, &a.de_tag, a.para_tag, 0)
            .await?;
        ok_ou_erro(&resp)
    }

    async fn recebe_notify(&self, mut tx: Transaction) -> Result<(), Erro> {
        let call_id = tx.original.call_id_header()?.value().to_string();
        let achada = trava(&self.assinaturas)
            .get(&call_id)
            .map(|a| (a.ramal.clone(), a.propria));
        let Some((ramal, propria)) = achada else {
            tx.reply(StatusCode::CallTransactionDoesNotExist).await?;
            return Ok(());
        };
        tx.reply(StatusCode::OK).await?;
        if !tx.original.body.is_empty() {
            let presenca = presenca::de_pidf(&String::from_utf8_lossy(&tx.original.body));
            if !propria {
                self.avisa(Evento::Presenca {
                    ramal: ramal.clone(),
                    presenca,
                });
            } else if presenca == Presenca::Offline && self.registrado.load(Ordering::Relaxed) {
                info!(%ramal, "o PBX deu o ramal como offline: registrando de novo");
                self.registra_agora.notify_one();
            }
        }
        let estado = valor_de(&tx.original.headers, "Subscription-State").unwrap_or_default();
        if estado.to_ascii_lowercase().starts_with("terminated") {
            trava(&self.assinaturas).remove(&call_id);
            if !propria {
                self.avisa(Evento::PresencaEncerrada { ramal });
            }
        }
        Ok(())
    }

    // ---- Ligacoes ----

    /// Liga para um ramal ou numero. Devolve o id da ligacao; o resto chega
    /// como eventos (Tocando, Atendida, Encerrada).
    pub async fn liga(
        self: &Arc<Self>,
        destino: &str,
        fonte: Box<dyn Fonte>,
        saida: Box<dyn Destino>,
    ) -> Result<String, Erro> {
        let uri = self.uri(destino)?;
        let socket = rede::socket_rtp(self.ip).await?;
        let local = Local {
            ip: self.ip,
            porta: socket.local_addr()?.port(),
            codecs: NOSSOS_CODECS.to_vec(),
            dtmf: Some(TIPO_DTMF),
            direcao: Direcao::EnviaERecebe,
        };
        let (midia, oferta) = Midia::nova(local, None);
        let opcao = InviteOption {
            caller: self.aor.clone(),
            caller_display_name: self.conta.nome.clone(),
            callee: uri,
            destination: Some(self.servidor.clone()),
            contact: self.contato.clone(),
            credential: Some(self.credencial.clone()),
            content_type: Some("application/sdp".into()),
            offer: Some(oferta.into_bytes()),
            ..Default::default()
        };
        let (dialogo, espera) = self.dialogos.do_invite_async(opcao, self.estados.clone())?;
        let id = dialogo.id().call_id.clone();
        trava(&self.ligacoes).insert(
            id.clone(),
            Ligacao {
                sentido: Sentido::Saida,
                dialogo,
                rtp: Some(socket),
                audio: Some((fonte, saida)),
                oferta: None,
                sessao: None,
                midia: Some(midia),
                atendida: false,
                tocando: false,
                por_nos: false,
                encerrada: false,
                transferida: false,
            },
        );
        let ua = Arc::clone(self);
        let id_tarefa = id.clone();
        tokio::spawn(async move {
            let resultado = espera.await;
            ua.fim_do_convite(&id_tarefa, resultado).await;
        });
        info!(%id, destino, "ligando");
        Ok(id)
    }

    async fn fim_do_convite(
        &self,
        id: &str,
        resultado: Result<InviteAsyncResult, tokio::task::JoinError>,
    ) {
        let por_nos = trava(&self.ligacoes).get(id).is_some_and(|l| l.por_nos);
        match resultado {
            Ok(Ok((_, Some(resp)))) if resp.status_code.kind() == StatusCodeKind::Successful => {
                if let Err(e) = self.comeca_audio_de_saida(id, &resp).await {
                    warn!(%id, "atenderam, mas o audio nao comecou: {e}");
                    let _ = self.desliga(id).await;
                }
            }
            Ok(Ok((_, Some(resp)))) => {
                let fim = if por_nos {
                    Fim::Cancelada
                } else {
                    fim_por_status(codigo(&resp))
                };
                self.encerra_ligacao(id, fim).await;
            }
            Ok(Ok((_, None))) => self.encerra_ligacao(id, Fim::SemResposta).await,
            Ok(Err(e)) => {
                warn!(%id, "o INVITE falhou: {e}");
                let fim = if por_nos {
                    Fim::Cancelada
                } else {
                    Fim::Falha(0)
                };
                self.encerra_ligacao(id, fim).await;
            }
            Err(e) => {
                warn!(%id, "a tarefa do INVITE caiu: {e}");
                self.encerra_ligacao(id, Fim::Falha(0)).await;
            }
        }
    }

    async fn comeca_audio_de_saida(&self, id: &str, resp: &Response) -> Result<(), Erro> {
        let remota = sdp::le(&String::from_utf8_lossy(&resp.body))?;
        let codec = sdp::escolhe(&remota, &NOSSOS_CODECS)?;
        let mut ligacoes = trava(&self.ligacoes);
        let ligacao = ligacoes
            .get_mut(id)
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
        let (Some(socket), Some((fonte, saida))) = (ligacao.rtp.take(), ligacao.audio.take())
        else {
            return Ok(());
        };
        ligacao.sessao = Some(Sessao::inicia(
            socket,
            SocketAddr::new(remota.ip, remota.porta),
            codec,
            remota.dtmf,
            fonte,
            saida,
        ));
        ligacao.atendida = true;
        if let Some(m) = ligacao.midia.as_mut() {
            m.codec = Some(codec);
        }
        drop(ligacoes);
        info!(%id, codec = codec.nome(), "atendida");
        self.avisa(Evento::Atendida {
            id: id.to_string(),
            codec,
        });
        Ok(())
    }

    async fn recebe_ligacao(self: &Arc<Self>, mut tx: Transaction) -> Result<(), Erro> {
        let oferta = match sdp::le(&String::from_utf8_lossy(&tx.original.body)) {
            Ok(o) if sdp::escolhe(&o, &NOSSOS_CODECS).is_ok() => o,
            resultado => {
                debug!(?resultado, "INVITE sem audio que a gente fale");
                tx.reply(StatusCode::NotAcceptableHere).await?;
                return Ok(());
            }
        };
        let (de, nome) = quem(&tx.original);
        let mut dialogo = self.dialogos.get_or_create_server_invite(
            &tx,
            self.estados.clone(),
            None,
            Some(self.contato.clone()),
        )?;
        let id = dialogo.id().call_id.clone();
        trava(&self.ligacoes).insert(
            id.clone(),
            Ligacao {
                sentido: Sentido::Entrada,
                dialogo: dialogo.clone(),
                rtp: None,
                audio: None,
                oferta: Some(oferta),
                sessao: None,
                midia: None,
                atendida: false,
                tocando: false,
                por_nos: false,
                encerrada: false,
                transferida: false,
            },
        );
        info!(%id, %de, "recebendo ligacao");
        self.avisa(Evento::Recebendo { id, de, nome });
        // Conduz a transacao do INVITE ate o ACK (ou o CANCEL de quem ligou).
        dialogo.handle(&mut tx).await?;
        Ok(())
    }

    /// Atende uma ligacao que esta chegando.
    pub async fn atende(
        &self,
        id: &str,
        fonte: Box<dyn Fonte>,
        saida: Box<dyn Destino>,
    ) -> Result<(), Erro> {
        let (dialogo, oferta) = {
            let ligacoes = trava(&self.ligacoes);
            let l = ligacoes
                .get(id)
                .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
            match (&l.sentido, l.atendida, &l.oferta) {
                (Sentido::Entrada, false, Some(o)) => (l.dialogo.clone(), o.clone()),
                _ => return Err(Erro::EstadoErrado(id.to_string())),
            }
        };
        let codec = sdp::escolhe(&oferta, &NOSSOS_CODECS)?;
        let socket = rede::socket_rtp(self.ip).await?;
        let local = Local {
            ip: self.ip,
            porta: socket.local_addr()?.port(),
            codecs: vec![codec],
            dtmf: oferta.dtmf,
            direcao: sdp::direcao_da_resposta(oferta.direcao),
        };
        let (midia, corpo) = Midia::nova(local, Some(codec));
        dialogo.accept(
            Some(vec![Header::ContentType("application/sdp".into())]),
            Some(corpo.into_bytes()),
        )?;
        let sessao = Sessao::inicia(
            socket,
            SocketAddr::new(oferta.ip, oferta.porta),
            codec,
            oferta.dtmf,
            fonte,
            saida,
        );
        if let Some(l) = trava(&self.ligacoes).get_mut(id) {
            l.sessao = Some(sessao);
            l.midia = Some(midia);
            l.atendida = true;
        }
        info!(%id, codec = codec.nome(), "atendemos");
        self.avisa(Evento::Atendida {
            id: id.to_string(),
            codec,
        });
        Ok(())
    }

    /// O pico da voz de cada lado desde a ultima leitura (nosso, deles), de 0
    /// a 32767: a tela mostra quem esta falando.
    pub fn niveis(&self, id: &str) -> Result<(u16, u16), Erro> {
        let ligacoes = trava(&self.ligacoes);
        let sessao = ligacoes
            .get(id)
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?
            .sessao
            .as_ref()
            .ok_or_else(|| Erro::EstadoErrado(id.to_string()))?;
        Ok(sessao.niveis())
    }

    /// Grava a ligacao atendida (`Some`) ou para de gravar (`None`).
    pub fn grava(&self, id: &str, gravador: Option<Box<dyn Gravador>>) -> Result<(), Erro> {
        let ligacoes = trava(&self.ligacoes);
        let sessao = ligacoes
            .get(id)
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?
            .sessao
            .as_ref()
            .ok_or_else(|| Erro::EstadoErrado(id.to_string()))?;
        sessao.grava(gravador);
        Ok(())
    }

    /// Desliga: BYE se atendida, CANCEL se ainda tocando do outro lado,
    /// recusa (486) se for uma ligacao chegando.
    pub async fn desliga(&self, id: &str) -> Result<(), Erro> {
        let (dialogo, sentido, atendida, transferida) = {
            let mut ligacoes = trava(&self.ligacoes);
            let l = ligacoes
                .get_mut(id)
                .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
            if l.encerrada {
                return Ok(());
            }
            l.por_nos = true;
            (l.dialogo.clone(), l.sentido, l.atendida, l.transferida)
        };
        match (sentido, atendida) {
            (_, true) => {
                if let Err(e) = dialogo.bye().await {
                    warn!(%id, "o BYE falhou: {e}");
                }
                // O BYE que encerra uma transferencia completa (NOTIFY 200).
                let fim = if transferida {
                    Fim::Transferida
                } else {
                    Fim::Desligou
                };
                self.encerra_ligacao(id, fim).await;
            }
            (Sentido::Saida, false) => {
                // O fim chega pelo 487 do INVITE (fim_do_convite).
                dialogo.cancel().await?;
            }
            (Sentido::Entrada, false) => {
                dialogo.reject(Some(StatusCode::BusyHere), None)?;
                self.encerra_ligacao(id, Fim::Recusada).await;
            }
        }
        Ok(())
    }

    /// Corta o microfone de uma ligacao (manda silencio).
    pub fn mudo(&self, id: &str, sim: bool) -> Result<(), Erro> {
        let ligacoes = trava(&self.ligacoes);
        let sessao = ligacoes
            .get(id)
            .and_then(|l| l.sessao.as_ref())
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
        sessao.mudo(sim);
        Ok(())
    }

    /// Transfere a ligacao, as cegas: REFER para o destino. O PBX liga para
    /// ele e derruba a nossa perna (BYE), que entra como "transferida".
    pub async fn transfere(&self, id: &str, destino: &str) -> Result<(), Erro> {
        let uri = self.uri(destino)?;
        let dialogo = {
            let ligacoes = trava(&self.ligacoes);
            match ligacoes.get(id) {
                Some(l) if l.atendida && !l.encerrada => l.dialogo.clone(),
                Some(_) => return Err(Erro::EstadoErrado(id.to_string())),
                None => return Err(Erro::SemLigacao(id.to_string())),
            }
        };
        // Marca ANTES de mandar: o chan_sip manda o NOTIFY com o andamento 1 ms
        // depois do 202, e ele pode ser tratado antes da resposta do REFER.
        self.marca_transferida(id, true);
        let resposta = dialogo.refer(format!("<{uri}>"), None, None).await;
        match resposta {
            Ok(Some(r)) if r.status_code.kind() == StatusCodeKind::Successful => {
                info!(%id, destino, "transferencia aceita pelo PBX");
                Ok(())
            }
            Ok(Some(r)) => {
                self.marca_transferida(id, false);
                Err(Erro::Resposta {
                    codigo: r.status_code.code(),
                    motivo: r.status_code.text().to_string(),
                })
            }
            Ok(None) => {
                self.marca_transferida(id, false);
                Err(Erro::EstadoErrado(id.to_string()))
            }
            Err(e) => {
                self.marca_transferida(id, false);
                Err(e.into())
            }
        }
    }

    fn marca_transferida(&self, id: &str, sim: bool) {
        if let Some(l) = trava(&self.ligacoes).get_mut(id)
            && !l.encerrada
        {
            l.transferida = sim;
        }
    }

    /// O NOTIFY do REFER (RFC 3515) traz o andamento da transferencia num
    /// sipfrag ("SIP/2.0 200 OK"). O chan_sip nao derruba a nossa perna quando
    /// a transferencia da certo: espera o BYE de quem transferiu, como a RFC
    /// 5589 manda, e a ligacao ficava presa na tela ate alguem desligar (visto
    /// no PBX em 26/11/2026). Com o 2xx final, desligamos nos (o fim entra como
    /// "transferida"); com uma falha final, a ligacao segue e a tela fica
    /// sabendo. O BYE do PBX, quando vem antes, continua valendo.
    async fn andamento_da_transferencia(&self, call_id: &str, req: &Request) {
        let evento = valor_de(&req.headers, "Event").unwrap_or_default();
        if !evento.trim().to_ascii_lowercase().starts_with("refer") {
            debug!(%evento, "NOTIFY no dialogo");
            return;
        }
        let Some(codigo) = codigo_do_sipfrag(&String::from_utf8_lossy(&req.body)) else {
            return;
        };
        let transferindo = trava(&self.ligacoes)
            .get(call_id)
            .is_some_and(|l| l.transferida && !l.encerrada);
        if !transferindo {
            return;
        }
        if (200..300).contains(&codigo) {
            info!(id = %call_id, codigo, "transferencia completa: desligando a nossa perna");
            if let Err(e) = self.desliga(call_id).await {
                warn!(id = %call_id, "desligar depois da transferencia: {e}");
            }
        } else if codigo >= 300 {
            if let Some(l) = trava(&self.ligacoes).get_mut(call_id) {
                l.transferida = false;
            }
            warn!(id = %call_id, codigo, "a transferencia nao se completou: a ligacao segue");
            self.avisa(Evento::TransferenciaFalhou {
                id: call_id.to_string(),
                codigo,
            });
        }
    }

    /// Manda tons do teclado (DTMF, RFC 4733) numa ligacao atendida.
    pub fn tons(&self, id: &str, teclas: &str) -> Result<usize, Erro> {
        let ligacoes = trava(&self.ligacoes);
        let sessao = ligacoes
            .get(id)
            .and_then(|l| l.sessao.as_ref())
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
        sessao.tons(teclas)
    }

    /// Troca o microfone (a fonte) de uma ligacao em andamento.
    pub fn troca_fonte(&self, id: &str, fonte: Box<dyn Fonte>) -> Result<(), Erro> {
        let ligacoes = trava(&self.ligacoes);
        let sessao = ligacoes
            .get(id)
            .and_then(|l| l.sessao.as_ref())
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
        sessao.troca_fonte(fonte);
        Ok(())
    }

    /// Poe a ligacao em espera (`sim`) ou tira dela: re-INVITE com sendonly
    /// ou sendrecv. Em espera vai silencio, nao o microfone, ja a partir daqui.
    pub async fn espera(&self, id: &str, sim: bool) -> Result<(), Erro> {
        let (dialogo, corpo) = {
            let mut ligacoes = trava(&self.ligacoes);
            let l = ligacoes
                .get_mut(id)
                .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
            let Ligacao {
                dialogo,
                sessao,
                midia,
                atendida: true,
                encerrada: false,
                ..
            } = l
            else {
                return Err(Erro::EstadoErrado(id.to_string()));
            };
            let m = midia
                .as_mut()
                .ok_or_else(|| Erro::EstadoErrado(id.to_string()))?;
            if m.em_espera == sim {
                return Ok(());
            }
            m.em_espera = sim;
            m.local.direcao = if sim {
                Direcao::SoEnvia
            } else {
                Direcao::EnviaERecebe
            };
            m.versao += 1;
            if sim && let Some(s) = sessao {
                s.ajusta(None, None, Envio::Silencio);
            }
            (dialogo.clone(), m.sdp())
        };
        let resposta = dialogo
            .reinvite(
                Some(vec![Header::ContentType("application/sdp".into())]),
                Some(corpo.into_bytes()),
            )
            .await;
        let mut ligacoes = trava(&self.ligacoes);
        let l = ligacoes
            .get_mut(id)
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
        let Ligacao {
            sessao,
            midia: Some(m),
            ..
        } = l
        else {
            return Err(Erro::EstadoErrado(id.to_string()));
        };
        let aceita = match resposta {
            Ok(Some(r)) if r.status_code.kind() == StatusCodeKind::Successful => Ok(r),
            Ok(Some(r)) => Err(Erro::Resposta {
                codigo: r.status_code.code(),
                motivo: r.status_code.text().to_string(),
            }),
            Ok(None) => Err(Erro::EstadoErrado(id.to_string())),
            Err(e) => Err(e.into()),
        };
        let r = match aceita {
            Ok(r) => r,
            Err(e) => {
                // Recusado: a ligacao segue como estava (RFC 3261, 14.1).
                m.em_espera = !sim;
                m.local.direcao = Direcao::de(true, !m.em_espera);
                if let Some(s) = sessao {
                    s.ajusta(None, None, m.envio(true));
                }
                return Err(e);
            }
        };
        let remota = sdp::le(&String::from_utf8_lossy(&r.body))?;
        let codec = codec_da_renegociacao(&remota, m.codec)?;
        m.codec = Some(codec);
        let podemos_mandar = m.local.direcao.envia() && remota.direcao.recebe();
        if let Some(s) = sessao {
            s.ajusta(endereco(&remota), Some(codec), m.envio(podemos_mandar));
        }
        info!(%id, sim, "espera");
        Ok(())
    }

    async fn encerra_ligacao(&self, id: &str, fim: Fim) {
        let sessao = {
            let mut ligacoes = trava(&self.ligacoes);
            let Some(l) = ligacoes.get_mut(id) else {
                return;
            };
            if l.encerrada {
                return;
            }
            l.encerrada = true;
            let sessao = l.sessao.take();
            ligacoes.remove(id);
            sessao
        };
        let estatistica = match sessao {
            Some(s) => Some(s.encerra().await),
            None => None,
        };
        info!(%id, fim = %fim.texto(), ?estatistica, "ligacao encerrada");
        self.avisa(Evento::Encerrada {
            id: id.to_string(),
            fim,
            estatistica,
        });
    }

    /// `em_outro`: o CANCEL disse que outro aparelho atendeu (Inspetor).
    fn fim_pelo_dialogo(l: &Ligacao, motivo: &TerminatedReason, em_outro: bool) -> Option<Fim> {
        if l.transferida {
            return Some(Fim::Transferida);
        }
        if l.por_nos {
            return Some(if l.atendida {
                Fim::Desligou
            } else if l.sentido == Sentido::Saida {
                Fim::Cancelada
            } else {
                Fim::Recusada
            });
        }
        if l.atendida {
            return Some(Fim::OutroDesligou);
        }
        if l.sentido == Sentido::Saida {
            // Quem decide e a resposta final do INVITE (fim_do_convite).
            return None;
        }
        Some(match motivo {
            TerminatedReason::UacCancel if em_outro => Fim::AtendidaEmOutro,
            TerminatedReason::UacCancel => Fim::Perdida,
            TerminatedReason::Timeout => Fim::Perdida,
            TerminatedReason::UacOther(c) | TerminatedReason::UasOther(c) => {
                fim_por_status(c.code())
            }
            _ => Fim::Perdida,
        })
    }

    async fn trata_estado(&self, estado: DialogState) {
        match estado {
            DialogState::Calling(id) => {
                // Chegando: avisa quem ligou que esta tocando aqui (180).
                let dialogo = trava(&self.ligacoes)
                    .get(&id.call_id)
                    .filter(|l| l.sentido == Sentido::Entrada && !l.tocando)
                    .map(|l| l.dialogo.clone());
                if let Some(d) = dialogo {
                    if let Err(e) = d.ringing(None, None) {
                        debug!("180 nao saiu: {e}");
                    }
                    if let Some(l) = trava(&self.ligacoes).get_mut(&id.call_id) {
                        l.tocando = true;
                    }
                }
            }
            DialogState::Early(id, resposta) => {
                let avisar = trava(&self.ligacoes)
                    .get_mut(&id.call_id)
                    .filter(|l| l.sentido == Sentido::Saida && !l.tocando)
                    .map(|l| {
                        l.tocando = true;
                    })
                    .is_some();
                if avisar {
                    self.avisa(Evento::Tocando {
                        id: id.call_id,
                        codigo: resposta.status_code.code(),
                    });
                }
            }
            DialogState::Terminated(id, motivo) => {
                let em_outro = trava(&self.em_outro).remove(&id.call_id);
                let fim = trava(&self.ligacoes)
                    .get(&id.call_id)
                    .and_then(|l| Self::fim_pelo_dialogo(l, &motivo, em_outro));
                self.dialogos.remove_dialog(&id);
                if let Some(fim) = fim {
                    self.encerra_ligacao(&id.call_id, fim).await;
                }
            }
            DialogState::Updated(id, req, handle) => {
                self.renegocia(&id.call_id, &req, handle).await;
            }
            DialogState::Notify(id, req, handle) => {
                let _ = handle.reply(StatusCode::OK).await;
                self.andamento_da_transferencia(&id.call_id, &req).await;
            }
            DialogState::Info(_, req, handle)
            | DialogState::Options(_, req, handle)
            | DialogState::Message(_, req, handle) => {
                debug!(metodo = %req.method, "pedido no dialogo");
                let _ = handle.reply(StatusCode::OK).await;
            }
            DialogState::Refer(_, _, handle) | DialogState::Publish(_, _, handle) => {
                let _ = handle.reply(StatusCode::NotImplemented).await;
            }
            _ => {}
        }
    }

    /// re-INVITE ou UPDATE do outro lado (espera, troca de codec). Por ora
    /// recusa com 488: a ligacao segue como estava.
    /// re-INVITE ou UPDATE no meio da ligacao. O Asterisk manda um re-INVITE
    /// logo depois do ACK de toda ligacao que chega, com o mesmo SDP e a versao
    /// do o= somada; responder 488 fazia ele desligar (causa 58). Aceitamos:
    /// mesmo codec se ainda vier, o endereco novo e a direcao (espera).
    async fn renegocia(&self, id: &str, req: &Request, handle: TransactionHandle) {
        if req.body.is_empty() && req.method == Method::Update {
            let _ = handle.reply(StatusCode::OK).await;
            return;
        }
        match self.resposta_da_renegociacao(id, &req.body) {
            Ok((corpo, aviso)) => {
                let cabecalhos = vec![Header::ContentType("application/sdp".into())];
                if let Err(e) = handle
                    .respond(StatusCode::OK, Some(cabecalhos), Some(corpo.into_bytes()))
                    .await
                {
                    debug!(metodo = %req.method, "resposta da renegociacao nao saiu: {e}");
                }
                if let Some(sim) = aviso {
                    info!(%id, sim, "o outro lado mudou a espera");
                    self.avisa(Evento::Espera {
                        id: id.to_string(),
                        sim,
                    });
                }
            }
            Err(e) => {
                // Sem acordo: a ligacao segue como estava (RFC 3261, 14.2).
                debug!(metodo = %req.method, "renegociacao recusada: {e}");
                let _ = handle.reply(StatusCode::NotAcceptableHere).await;
            }
        }
    }

    /// O SDP da resposta e, se a espera do outro lado mudou, o aviso.
    fn resposta_da_renegociacao(
        &self,
        id: &str,
        corpo: &[u8],
    ) -> Result<(String, Option<bool>), Erro> {
        let mut ligacoes = trava(&self.ligacoes);
        let l = ligacoes
            .get_mut(id)
            .ok_or_else(|| Erro::SemLigacao(id.to_string()))?;
        let Ligacao {
            sessao,
            midia: Some(m),
            ..
        } = l
        else {
            return Err(Erro::EstadoErrado(id.to_string()));
        };
        if corpo.is_empty() {
            // re-INVITE sem oferta: vai a nossa de novo. A resposta viria no
            // ACK; por ora fica o que ja estava combinado.
            return Ok((m.sdp(), None));
        }
        let oferta = sdp::le(&String::from_utf8_lossy(corpo))?;
        let codec = codec_da_renegociacao(&oferta, m.codec)?;
        let quero = Direcao::de(true, !m.em_espera);
        let direcao = quero.junta(sdp::direcao_da_resposta(oferta.direcao));
        let novo = Local {
            codecs: vec![codec],
            dtmf: oferta.dtmf,
            direcao,
            ..m.local.clone()
        };
        if novo != m.local {
            m.versao += 1;
            m.local = novo;
        }
        m.codec = Some(codec);
        let deles = !oferta.direcao.recebe();
        let aviso = (deles != m.deles_em_espera).then_some(deles);
        m.deles_em_espera = deles;
        if let Some(s) = sessao {
            s.ajusta(endereco(&oferta), Some(codec), m.envio(direcao.envia()));
        }
        Ok((m.sdp(), aviso))
    }

    // ---- Tarefas de fundo ----

    async fn laco_entrada(self: Arc<Self>, mut entrada: TransactionReceiver) {
        while let Some(tx) = entrada.recv().await {
            let ua = Arc::clone(&self);
            tokio::spawn(async move {
                let metodo = tx.original.method.clone();
                if let Err(e) = ua.trata_pedido(tx).await {
                    debug!(%metodo, "pedido de fora: {e}");
                }
            });
        }
    }

    async fn trata_pedido(self: &Arc<Self>, mut tx: Transaction) -> Result<(), Erro> {
        let no_dialogo = tx
            .original
            .to_header()
            .ok()
            .and_then(|t| t.tag().ok().flatten())
            .is_some();
        match tx.original.method {
            Method::Invite if !no_dialogo => self.recebe_ligacao(tx).await,
            Method::Message if !no_dialogo => self.recebe_mensagem(tx).await,
            // O NOTIFY de uma transferencia (REFER) e do dialogo da ligacao; o
            // resto e presenca. Mandar todos para a presenca respondia 481 ao
            // NOTIFY da transferencia, e o chan_sip derrubava a nossa perna
            // sem BYE: a ligacao ficava presa na tela (visto em 26/11/2026).
            Method::Notify if !self.e_de_ligacao(&tx) => self.recebe_notify(tx).await,
            Method::Options if !no_dialogo => Ok(tx.reply(StatusCode::OK).await?),
            _ => match self.dialogos.match_dialog(&tx) {
                Some(mut dialogo) => Ok(dialogo.handle(&mut tx).await?),
                None if tx.original.method == Method::Ack => Ok(()),
                None => Ok(tx.reply(StatusCode::CallTransactionDoesNotExist).await?),
            },
        }
    }

    /// O pedido e de uma ligacao nossa (pelo Call-ID)?
    fn e_de_ligacao(&self, tx: &Transaction) -> bool {
        tx.original
            .call_id_header()
            .ok()
            .is_some_and(|c| trava(&self.ligacoes).contains_key(c.value()))
    }

    async fn laco_estados(self: Arc<Self>, mut estados: DialogStateReceiver) {
        while let Some(estado) = estados.recv().await {
            self.trata_estado(estado).await;
        }
    }

    /// CRLF para o PBX a cada `sinal_de_vida_s`: mantem aberto, no firewall
    /// desta maquina e nos do caminho, a volta dos avisos do PBX.
    async fn laco_sinal_de_vida(self: Arc<Self>) {
        let periodo = Duration::from_secs(u64::from(self.conta.sinal_de_vida_s));
        let mut relogio = tokio::time::interval(periodo);
        loop {
            tokio::select! {
                () = self.cancela.cancelled() => return,
                _ = relogio.tick() => {
                    if let Err(e) = self.udp.send_raw(b"\r\n\r\n", &self.servidor).await {
                        debug!("sinal de vida nao saiu: {e}");
                    }
                }
            }
        }
    }

    async fn laco_registro(self: Arc<Self>) {
        let servidor = match Uri::try_from(self.conta.servidor().as_str()) {
            Ok(u) => u,
            Err(e) => {
                self.avisa(Evento::Registro(Registro::Falhou {
                    codigo: None,
                    motivo: e.to_string(),
                }));
                return;
            }
        };
        // O laco do `Registration::keep_alive` do rsipstack (renova a 75% do
        // prazo; depois de falha, de novo em 1, 2, 4... ate 60 s), com uma porta
        // a mais: o offline do proprio ramal (`assina_propria`) adianta a vez.
        let mut registro = Registration::new(self.endpoint.clone(), Some(self.credencial.clone()));
        let mut falhas: u32 = 0;
        let mut forcado: Option<Instant> = None;
        let mut assinou = false;
        loop {
            let evento = match registro
                .register(servidor.clone(), Some(self.conta.expira_s))
                .await
            {
                Ok(resp) if resp.status_code.kind() == StatusCodeKind::Successful => {
                    falhas = 0;
                    self.registrado.store(true, Ordering::Relaxed);
                    let expira = valor_de(&resp.headers, "Expires")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(self.conta.expira_s);
                    Registro::Registrado { expira }
                }
                Ok(resp) => {
                    falhas = falhas.saturating_add(1);
                    Registro::Falhou {
                        codigo: Some(codigo(&resp)),
                        motivo: resp.status_code.text().to_string(),
                    }
                }
                Err(e) => {
                    falhas = falhas.saturating_add(1);
                    Registro::Falhou {
                        codigo: None,
                        motivo: e.to_string(),
                    }
                }
            };
            self.avisa(Evento::Registro(evento));
            if falhas == 0 && !assinou {
                assinou = true;
                tokio::spawn(Arc::clone(&self).assina_propria());
            }
            let espera = if falhas == 0 {
                let prazo = registro.granted_expires.filter(|g| *g > 0);
                refresh_interval(prazo.unwrap_or(self.conta.expira_s))
            } else {
                backoff_interval(falhas.saturating_sub(1))
            };
            tokio::select! {
                () = self.cancela_registro.cancelled() => break,
                () = tokio::time::sleep(espera) => {}
                () = self.registra_agora.notified() => {
                    if let Some(antes) = forcado {
                        let falta = ENTRE_REGISTROS_FORCADOS.saturating_sub(antes.elapsed());
                        tokio::select! {
                            () = self.cancela_registro.cancelled() => break,
                            () = tokio::time::sleep(falta) => {}
                        }
                    }
                    forcado = Some(Instant::now());
                }
            }
        }
        if self.registrado.swap(false, Ordering::Relaxed) {
            match registro.register(servidor, Some(0)).await {
                Ok(_) => self.avisa(Evento::Registro(Registro::Desregistrado)),
                Err(e) => warn!("o desregistro falhou: {e}"),
            }
        }
    }

    /// Desliga a conta com educacao: encerra as ligacoes, cancela as
    /// assinaturas, tira o registro e para as tarefas.
    pub async fn encerra(&self) {
        let ids: Vec<String> = trava(&self.ligacoes).keys().cloned().collect();
        for id in ids {
            if let Err(e) = self.desliga(&id).await {
                debug!(%id, "ao encerrar: {e}");
            }
        }
        // Todas, a do proprio ramal tambem (antes do desregistro: o offline que
        // ele causa nao chega a pedir registro de novo).
        let assinaturas: Vec<Assinatura> =
            trava(&self.assinaturas).drain().map(|(_, a)| a).collect();
        for a in assinaturas {
            let ramal = a.ramal.clone();
            if let Err(e) = self.encerra_assinatura(a).await {
                debug!(%ramal, "ao cancelar a presenca: {e}");
            }
        }
        self.cancela_registro.cancel();
        let tarefa = trava(&self.tarefa_registro).take();
        if let Some(t) = tarefa {
            let _ = tokio::time::timeout(Duration::from_secs(5), t).await;
        }
        self.cancela.cancel();
    }

    /// Sai sem desregistrar: a rede mudou (ou o computador voltou da
    /// suspensao) e a conta vai subir de novo. O REGISTER da nova substitui o
    /// contato; um desregistro daqui (Expires 0) apagaria, no chan_sip, o
    /// registro do ramal inteiro, inclusive o novo.
    pub async fn abandona(&self) {
        self.registrado.store(false, Ordering::Relaxed);
        self.cancela_registro.cancel();
        let tarefa = trava(&self.tarefa_registro).take();
        if let Some(t) = tarefa {
            let _ = tokio::time::timeout(Duration::from_secs(2), t).await;
        }
        self.cancela.cancel();
    }
}
