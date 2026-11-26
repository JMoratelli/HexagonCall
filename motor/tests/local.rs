//! O motor de ponta a ponta sem PBX: dois UAs em 127.0.0.1 falando direto um
//! com o outro, e um servidor falso que pede senha como o chan_sip. E o que o
//! CI roda; o teste contra o PBX de verdade fica na bancada.

// Num teste, unwrap/expect/panico e a forma honesta de dizer "isto tem de
// existir"; o clippy.toml ja libera nas funcoes #[test], aqui vale para os
// auxiliares tambem.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use motor::midia::{Captura, Silencio, Tom};
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
