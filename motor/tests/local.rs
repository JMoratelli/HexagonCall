//! O motor de ponta a ponta sem PBX: dois UAs em 127.0.0.1 falando direto um
//! com o outro, e um servidor falso que pede senha como o chan_sip. E o que o
//! CI roda; o teste contra o PBX de verdade fica na bancada.

// Num teste, unwrap/expect/panico e a forma honesta de dizer "isto tem de
// existir"; o clippy.toml ja libera nas funcoes #[test], aqui vale para os
// auxiliares tambem.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use motor::midia::{Captura, Gravador, Quadro, Silencio, Tom};
use motor::{Erro, Evento, Opcoes, Registro, Ua};
use nucleo::conta::Conta;
use nucleo::ligacao::Fim;
use rsipstack::EndpointBuilder;
use rsipstack::dialog::authenticate::verify_digest;
use rsipstack::sip::{Header, StatusCode};
use rsipstack::transport::TransportLayer;
use rsipstack::transport::udp::UdpConnection;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

const ESPERA: Duration = Duration::from_secs(10);

fn porta_livre() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn ua(
    usuario: &str,
    porta_propria: u16,
    porta_do_outro: u16,
) -> (Arc<Ua>, UnboundedReceiver<Evento>) {
    let mut conta = Conta::nova(usuario, "127.0.0.1");
    conta.porta = porta_do_outro;
    let opcoes = Opcoes {
        porta_sip: porta_propria,
        registrar: false,
    };
    Ua::inicia(conta, "senha".into(), opcoes).await.unwrap()
}

/// Dois UAs, cada um com o outro no lugar do PBX.
async fn par() -> (
    (Arc<Ua>, UnboundedReceiver<Evento>),
    (Arc<Ua>, UnboundedReceiver<Evento>),
) {
    let (pa, pb) = (porta_livre(), porta_livre());
    (ua("a", pa, pb).await, ua("b", pb, pa).await)
}

async fn espera(
    rx: &mut UnboundedReceiver<Evento>,
    mut quer: impl FnMut(&Evento) -> bool,
) -> Evento {
    tokio::time::timeout(ESPERA, async {
        loop {
            let e = rx.recv().await.expect("o motor parou");
            if quer(&e) {
                return e;
            }
        }
    })
    .await
    .expect("tempo esgotado esperando o evento")
}

fn id_recebendo(e: &Evento) -> String {
    match e {
        Evento::Recebendo { id, .. } => id.clone(),
        outro => panic!("esperava Recebendo, veio {outro:?}"),
    }
}

fn fim(e: &Evento) -> Fim {
    match e {
        Evento::Encerrada { fim, .. } => *fim,
        outro => panic!("esperava Encerrada, veio {outro:?}"),
    }
}

/// Frequencia pelos cruzamentos de zero, sem o silencio das pontas.
fn frequencia(amostras: &[i16]) -> f32 {
    let inicio = amostras
        .iter()
        .position(|a| a.unsigned_abs() > 200)
        .unwrap_or(0);
    let fim = amostras
        .iter()
        .rposition(|a| a.unsigned_abs() > 200)
        .unwrap_or(0);
    let trecho = amostras.get(inicio..fim).unwrap_or_default();
    let cruzamentos = trecho
        .windows(2)
        .filter(|w| (w.first() < Some(&0)) != (w.get(1) < Some(&0)))
        .count();
    #[allow(clippy::cast_precision_loss)]
    let (c, n) = (cruzamentos as f32, trecho.len() as f32);
    c / 2.0 / (n / 8000.0)
}

#[tokio::test]
async fn mensagem_de_um_para_o_outro() {
    let ((a, _ra), (_b, mut rb)) = par().await;
    a.mensagem("b", "Olá, ligação às 10h? ção ✓").await.unwrap();
    match espera(&mut rb, |e| matches!(e, Evento::Mensagem { .. })).await {
        Evento::Mensagem { de, texto, .. } => {
            assert_eq!(de, "a");
            assert_eq!(texto, "Olá, ligação às 10h? ção ✓");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn liga_atende_fala_e_desliga() {
    let ((a, mut ra), (b, mut rb)) = par().await;
    let ouvido_a = Captura::default();
    let ouvido_b = Captura::default();

    let id = a
        .liga(
            "b",
            Box::new(Tom::novo(440.0, 0.3)),
            Box::new(ouvido_a.clone()),
        )
        .await
        .unwrap();
    let chegando = id_recebendo(&espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await);
    assert_eq!(chegando, id, "o mesmo Call-ID dos dois lados");
    espera(&mut ra, |e| matches!(e, Evento::Tocando { .. })).await;

    b.atende(
        &chegando,
        Box::new(Tom::novo(1000.0, 0.3)),
        Box::new(ouvido_b.clone()),
    )
    .await
    .unwrap();
    espera(&mut ra, |e| matches!(e, Evento::Atendida { .. })).await;
    espera(&mut rb, |e| matches!(e, Evento::Atendida { .. })).await;

    tokio::time::sleep(Duration::from_millis(1500)).await;
    a.desliga(&id).await.unwrap();

    let fim_a = espera(&mut ra, |e| matches!(e, Evento::Encerrada { .. })).await;
    let fim_b = espera(&mut rb, |e| matches!(e, Evento::Encerrada { .. })).await;
    assert_eq!(fim(&fim_a), Fim::Desligou);
    assert_eq!(fim(&fim_b), Fim::OutroDesligou);
    if let Evento::Encerrada {
        estatistica: Some(e),
        ..
    } = fim_a
    {
        assert!(e.enviados >= 50, "mandou so {} pacotes", e.enviados);
        assert!(
            e.jitter.tocados >= 40,
            "tocou so {} quadros",
            e.jitter.tocados
        );
    } else {
        panic!("a ligacao atendida tem de trazer a estatistica");
    }

    // Cada lado ouviu o tom do outro.
    let fa = frequencia(&ouvido_a.amostras.lock().unwrap());
    let fb = frequencia(&ouvido_b.amostras.lock().unwrap());
    assert!((900.0..1100.0).contains(&fa), "a ouviu {fa} Hz");
    assert!((400.0..480.0).contains(&fb), "b ouviu {fb} Hz");
}

/// O nosso lado, o deles e quantas vezes a gravacao fechou.
type Lados = (Vec<i16>, Vec<i16>, u32);

/// Guarda os dois lados que a sessao entrega e quantas vezes fechou.
#[derive(Clone, Default)]
struct GravaTeste {
    lados: Arc<std::sync::Mutex<Lados>>,
}

impl Gravador for GravaTeste {
    fn quadro(&mut self, nosso: &Quadro, deles: &Quadro) {
        let mut l = self.lados.lock().unwrap();
        l.0.extend_from_slice(nosso);
        l.1.extend_from_slice(deles);
    }

    fn fim(&mut self) {
        self.lados.lock().unwrap().2 += 1;
    }
}

#[tokio::test]
async fn a_gravacao_ouve_os_dois_lados_juntos() {
    let ((a, mut ra), (b, mut rb)) = par().await;
    let id = a
        .liga("b", Box::new(Tom::novo(440.0, 0.3)), Box::new(Silencio))
        .await
        .unwrap();
    // Antes de atender nao ha audio para gravar.
    assert!(matches!(
        a.grava(&id, Some(Box::new(GravaTeste::default()))),
        Err(Erro::EstadoErrado(_))
    ));
    let chegando = id_recebendo(&espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await);
    b.atende(
        &chegando,
        Box::new(Tom::novo(1000.0, 0.3)),
        Box::new(Silencio),
    )
    .await
    .unwrap();
    espera(&mut ra, |e| matches!(e, Evento::Atendida { .. })).await;
    let gravacao = GravaTeste::default();
    a.grava(&id, Some(Box::new(gravacao.clone()))).unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    a.desliga(&id).await.unwrap();
    espera(&mut ra, |e| matches!(e, Evento::Encerrada { .. })).await;

    let (nosso, deles, fechou) = gravacao.lados.lock().unwrap().clone();
    assert_eq!(nosso.len(), deles.len(), "os dois lados andam juntos");
    assert!(nosso.len() >= 8000, "gravou so {} amostras", nosso.len());
    let (fn_, fd) = (frequencia(&nosso), frequencia(&deles));
    assert!((400.0..480.0).contains(&fn_), "o nosso lado tem {fn_} Hz");
    assert!((900.0..1100.0).contains(&fd), "o outro lado tem {fd} Hz");
    assert_eq!(fechou, 1, "a gravacao fecha uma vez, no fim da ligacao");
}

/// O que entrou no ouvido depois de `desde` amostras, pulando 200 ms para o
/// jitter esvaziar o que ja estava guardado.
fn ouvido_desde(ouvido: &Captura, desde: usize) -> Vec<i16> {
    let amostras = ouvido.amostras.lock().unwrap();
    amostras.get(desde + 1600..).unwrap_or_default().to_vec()
}

fn tamanho(ouvido: &Captura) -> usize {
    ouvido.amostras.lock().unwrap().len()
}

fn quieto(amostras: &[i16]) -> bool {
    !amostras.is_empty() && amostras.iter().all(|a| a.unsigned_abs() < 200)
}

#[tokio::test]
async fn espera_e_volta() {
    let ((a, mut ra), (b, mut rb)) = par().await;
    let ouvido_a = Captura::default();
    let ouvido_b = Captura::default();
    let id = a
        .liga(
            "b",
            Box::new(Tom::novo(440.0, 0.3)),
            Box::new(ouvido_a.clone()),
        )
        .await
        .unwrap();
    let chegando = id_recebendo(&espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await);
    b.atende(
        &chegando,
        Box::new(Tom::novo(1000.0, 0.3)),
        Box::new(ouvido_b.clone()),
    )
    .await
    .unwrap();
    espera(&mut ra, |e| matches!(e, Evento::Atendida { .. })).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // A poe em espera: re-INVITE sendonly. B fica sabendo e para de mandar;
    // A manda silencio no lugar do tom.
    a.espera(&id, true).await.unwrap();
    let aviso = espera(&mut rb, |e| matches!(e, Evento::Espera { .. })).await;
    assert!(matches!(aviso, Evento::Espera { sim: true, .. }));
    let (desde_a, desde_b) = (tamanho(&ouvido_a), tamanho(&ouvido_b));
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(
        quieto(&ouvido_desde(&ouvido_a, desde_a)),
        "a ainda ouve b na espera"
    );
    assert!(
        quieto(&ouvido_desde(&ouvido_b, desde_b)),
        "b ainda ouve a na espera"
    );

    // A tira da espera: os dois voltam a se ouvir.
    a.espera(&id, false).await.unwrap();
    let aviso = espera(&mut rb, |e| matches!(e, Evento::Espera { .. })).await;
    assert!(matches!(aviso, Evento::Espera { sim: false, .. }));
    let (desde_a, desde_b) = (tamanho(&ouvido_a), tamanho(&ouvido_b));
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let fa = frequencia(&ouvido_desde(&ouvido_a, desde_a));
    let fb = frequencia(&ouvido_desde(&ouvido_b, desde_b));
    assert!(
        (900.0..1100.0).contains(&fa),
        "a ouviu {fa} Hz depois da espera"
    );
    assert!(
        (400.0..480.0).contains(&fb),
        "b ouviu {fb} Hz depois da espera"
    );

    a.desliga(&id).await.unwrap();
    let fim_b = espera(&mut rb, |e| matches!(e, Evento::Encerrada { .. })).await;
    assert_eq!(fim(&fim_b), Fim::OutroDesligou);
}

#[tokio::test]
async fn tons_do_teclado_vao_como_eventos_rtp() {
    let ((a, mut ra), (b, mut rb)) = par().await;
    let id = a
        .liga("b", Box::new(Silencio), Box::new(Silencio))
        .await
        .unwrap();
    let chegando = id_recebendo(&espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await);
    b.atende(&chegando, Box::new(Silencio), Box::new(Silencio))
        .await
        .unwrap();
    espera(&mut ra, |e| matches!(e, Evento::Atendida { .. })).await;

    // "x" nao e tecla: fica de fora.
    assert_eq!(a.tons(&id, "12x#").unwrap(), 3);
    // 3 teclas x (5 de tom + 3 de fim) + pausas: uns 700 ms.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    a.desliga(&id).await.unwrap();

    let fim_a = espera(&mut ra, |e| matches!(e, Evento::Encerrada { .. })).await;
    let fim_b = espera(&mut rb, |e| matches!(e, Evento::Encerrada { .. })).await;
    let (
        Evento::Encerrada {
            estatistica: Some(ea),
            ..
        },
        Evento::Encerrada {
            estatistica: Some(eb),
            ..
        },
    ) = (fim_a, fim_b)
    else {
        panic!("a ligacao atendida tem de trazer a estatistica");
    };
    assert_eq!(ea.tons, 24, "8 pacotes por tecla");
    assert!(eb.outros >= 24, "b recebeu {} pacotes de tom", eb.outros);
}

#[tokio::test]
async fn transferencia_recusada_mantem_a_ligacao() {
    let ((a, mut ra), (b, mut rb)) = par().await;
    let id = a
        .liga("b", Box::new(Silencio), Box::new(Silencio))
        .await
        .unwrap();
    let chegando = id_recebendo(&espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await);
    b.atende(&chegando, Box::new(Silencio), Box::new(Silencio))
        .await
        .unwrap();
    espera(&mut ra, |e| matches!(e, Evento::Atendida { .. })).await;

    // O UA de teste nao faz transferencia (responde 501): a ligacao segue, e o
    // fim continua sendo de quem desligou.
    match a.transfere(&id, "c").await {
        Err(Erro::Resposta { codigo: 501, .. }) => {}
        outro => panic!("esperava 501, veio {outro:?}"),
    }
    a.desliga(&id).await.unwrap();
    let fim_a = espera(&mut ra, |e| matches!(e, Evento::Encerrada { .. })).await;
    assert_eq!(fim(&fim_a), Fim::Desligou);
}

#[tokio::test]
async fn recusar_da_ocupado_para_quem_liga() {
    let ((a, mut ra), (b, mut rb)) = par().await;
    a.liga("b", Box::new(Silencio), Box::new(Silencio))
        .await
        .unwrap();
    let id = id_recebendo(&espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await);
    b.desliga(&id).await.unwrap();
    assert_eq!(
        fim(&espera(&mut rb, |e| matches!(e, Evento::Encerrada { .. })).await),
        Fim::Recusada
    );
    assert_eq!(
        fim(&espera(&mut ra, |e| matches!(e, Evento::Encerrada { .. })).await),
        Fim::Ocupado
    );
}

#[tokio::test]
async fn desistir_enquanto_toca_vira_perdida_do_outro_lado() {
    let ((a, mut ra), (_b, mut rb)) = par().await;
    let id = a
        .liga("b", Box::new(Silencio), Box::new(Silencio))
        .await
        .unwrap();
    espera(&mut rb, |e| matches!(e, Evento::Recebendo { .. })).await;
    espera(&mut ra, |e| matches!(e, Evento::Tocando { .. })).await;
    a.desliga(&id).await.unwrap();
    assert_eq!(
        fim(&espera(&mut ra, |e| matches!(e, Evento::Encerrada { .. })).await),
        Fim::Cancelada
    );
    assert_eq!(
        fim(&espera(&mut rb, |e| matches!(e, Evento::Encerrada { .. })).await),
        Fim::Perdida
    );
}

/// Um PBX de mentira por UDP cru: o INVITE de um grupo de ramais e depois o
/// CANCEL, com o Reason que o chan_sip poe quando outro ramal atende (ou sem).
async fn grupo_cancela(reason: Option<&str>) -> Fim {
    let pbx = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let do_pbx = pbx.local_addr().unwrap();
    let porta = porta_livre();
    let mut conta = Conta::nova("1001", "127.0.0.1");
    conta.porta = do_pbx.port();
    let opcoes = Opcoes {
        porta_sip: porta,
        registrar: false,
    };
    let (_ua, mut r) = Ua::inicia(conta, "senha".into(), opcoes).await.unwrap();
    let sdp = "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=-\r\nc=IN IP4 127.0.0.1\r\nt=0 0\r\n\
               m=audio 40000 RTP/AVP 0\r\na=rtpmap:0 PCMU/8000\r\n";
    let comum = |metodo: &str| {
        format!(
            "{metodo} sip:1001@127.0.0.1:{porta} SIP/2.0\r\n\
             Via: SIP/2.0/UDP {do_pbx};branch=z9hG4bK-grupo{porta}\r\n\
             Max-Forwards: 70\r\n\
             From: \"Recepcao\" <sip:1002@127.0.0.1>;tag=pbx1\r\n\
             To: <sip:1001@127.0.0.1>\r\n\
             Call-ID: grupo-{porta}@127.0.0.1\r\n\
             CSeq: 102 {metodo}\r\n"
        )
    };
    let invite = format!(
        "{}Contact: <sip:1002@{do_pbx}>\r\nContent-Type: application/sdp\r\n\
         Content-Length: {}\r\n\r\n{sdp}",
        comum("INVITE"),
        sdp.len()
    );
    pbx.send_to(invite.as_bytes(), ("127.0.0.1", porta))
        .await
        .unwrap();
    espera(&mut r, |e| matches!(e, Evento::Recebendo { .. })).await;
    let motivo = reason
        .map(|v| format!("Reason: {v}\r\n"))
        .unwrap_or_default();
    let cancel = format!("{}{motivo}Content-Length: 0\r\n\r\n", comum("CANCEL"));
    pbx.send_to(cancel.as_bytes(), ("127.0.0.1", porta))
        .await
        .unwrap();
    fim(&espera(&mut r, |e| matches!(e, Evento::Encerrada { .. })).await)
}

#[tokio::test]
async fn atendida_em_outro_ramal_nao_e_perdida() {
    assert_eq!(
        grupo_cancela(Some(r#"SIP;cause=200;text="Call completed elsewhere""#)).await,
        Fim::AtendidaEmOutro
    );
    assert_eq!(grupo_cancela(None).await, Fim::Perdida);
}

#[tokio::test]
async fn destino_invalido_nem_sai() {
    let ((a, _ra), _b) = par().await;
    assert!(matches!(
        a.mensagem("64 02", "x").await,
        Err(Erro::Conta(_))
    ));
}

// ---- Autenticacao, contra um servidor que pede senha como o chan_sip ----

/// Responde 401 a quem chega sem Authorization e confere o digest de quem
/// chega com: 200 se a senha bate, 403 se nao (o que o chan_sip faz).
async fn servidor_com_senha(senha: &'static str) -> (SocketAddr, CancellationToken) {
    let cancela = CancellationToken::new();
    let udp = UdpConnection::create_connection(
        "127.0.0.1:0".parse().unwrap(),
        None,
        Some(cancela.child_token()),
    )
    .await
    .unwrap();
    let endereco = udp.get_addr().get_socketaddr().unwrap();
    let transporte = TransportLayer::new(cancela.clone());
    transporte.add_transport(udp.into());
    let endpoint = EndpointBuilder::new()
        .with_cancel_token(cancela.clone())
        .with_transport_layer(transporte)
        .build();
    let mut entrada = endpoint.incoming_transactions().unwrap();
    let interno = endpoint.inner.clone();
    tokio::spawn(async move {
        let _vivo = endpoint;
        let _ = interno.serve().await;
    });
    tokio::spawn(async move {
        while let Some(mut tx) = entrada.recv().await {
            let autorizacao = tx.original.headers.iter().find_map(|h| match h {
                Header::Authorization(a) => Some(a.clone()),
                _ => None,
            });
            let resposta = match autorizacao {
                None => {
                    let desafio = Header::WwwAuthenticate(
                        r#"Digest algorithm=MD5, realm="asterisk", nonce="4f1c2b3a""#.into(),
                    );
                    tx.reply_with(StatusCode::Unauthorized, vec![desafio], None)
                        .await
                }
                Some(a) => {
                    let bate = rsipstack::sip::typed::Authorization::parse(a.value())
                        .is_ok_and(|t| verify_digest(&t, senha, &tx.original.method, a.value()));
                    if bate {
                        tx.reply_with(StatusCode::OK, vec![Header::Expires(600.into())], None)
                            .await
                    } else {
                        tx.reply(StatusCode::Forbidden).await
                    }
                }
            };
            resposta.unwrap();
        }
    });
    (endereco, cancela)
}

async fn ua_no_servidor(
    servidor: SocketAddr,
    senha: &str,
    registrar: bool,
) -> (Arc<Ua>, UnboundedReceiver<Evento>) {
    let mut conta = Conta::nova("1001", "127.0.0.1");
    conta.porta = servidor.port();
    Ua::inicia(
        conta,
        senha.into(),
        Opcoes {
            porta_sip: 0,
            registrar,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn mensagem_autentica_na_segunda_tentativa() {
    let (servidor, _c) = servidor_com_senha("certa").await;
    let (ua, _r) = ua_no_servidor(servidor, "certa", false).await;
    ua.mensagem("1002", "teste").await.unwrap();
}

#[tokio::test]
async fn senha_errada_vira_403() {
    let (servidor, _c) = servidor_com_senha("certa").await;
    let (ua, _r) = ua_no_servidor(servidor, "errada", false).await;
    match ua.mensagem("1002", "teste").await {
        Err(Erro::Resposta { codigo, .. }) => assert_eq!(codigo, 403),
        outro => panic!("esperava 403, veio {outro:?}"),
    }
}

#[tokio::test]
async fn registra_e_desregistra() {
    let (servidor, _c) = servidor_com_senha("certa").await;
    let (ua, mut r) = ua_no_servidor(servidor, "certa", true).await;
    let e = espera(&mut r, |e| matches!(e, Evento::Registro(_))).await;
    assert!(
        matches!(e, Evento::Registro(Registro::Registrado { expira: 600 })),
        "{e:?}"
    );
    ua.encerra().await;
    let e = espera(&mut r, |e| matches!(e, Evento::Registro(_))).await;
    assert!(
        matches!(e, Evento::Registro(Registro::Desregistrado)),
        "{e:?}"
    );
}

#[tokio::test]
async fn abandonar_nao_desregistra() {
    // A rede mudou: a conta sobe de novo e o REGISTER novo substitui o
    // contato. Um desregistro da velha (Expires 0) apagaria, no chan_sip, o
    // registro do ramal inteiro, inclusive o novo.
    let (servidor, _c) = servidor_com_senha("certa").await;
    let (ua, mut r) = ua_no_servidor(servidor, "certa", true).await;
    let e = espera(&mut r, |e| matches!(e, Evento::Registro(_))).await;
    assert!(
        matches!(e, Evento::Registro(Registro::Registrado { .. })),
        "{e:?}"
    );
    ua.abandona().await;
    let depois = tokio::time::timeout(Duration::from_millis(800), async {
        loop {
            match r.recv().await {
                Some(Evento::Registro(Registro::Desregistrado)) => return true,
                Some(_) => {}
                None => return false,
            }
        }
    })
    .await;
    assert!(!matches!(depois, Ok(true)), "abandonar mandou desregistro");
}
