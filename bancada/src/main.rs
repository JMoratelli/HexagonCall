//! Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
//! Licenca GNU GPL 3 ou posterior (LICENSE na raiz do repositorio).
//!
//! A bancada: o motor pela linha de comando, para testar contra um PBX de
//! verdade sem a janela. O PBX (servidor, ramal, senha e o ramal do outro
//! lado) fica no `.env` da raiz, fora do git: so na maquina de quem testa
//! (`.env.exemplo` mostra as chaves). O que vier na linha de comando ou no
//! ambiente vale mais que o `.env`; a senha nunca vai como argumento
//! (apareceria no `ps`).
//!
//!   bancada mensagem "oi"              (para o SIP_DESTINO, ou --para 1002)
//!   bancada presenca --segundos 120
//!   bancada liga --toca fala.wav --grava volta.wav --segundos 20
//!   bancada liga --microfone --alto-falante --segundos 120
//!   bancada escuta --atende --tom 440
//!   bancada dispositivos               (microfones e alto-falantes, com o id)

use clap::{Parser, Subcommand};
use motor::midia::{Destino, Fonte, GravaWav, Quadro, Silencio, Tom, Wav};
use motor::{Evento, Opcoes, Registro, Ua};
use nucleo::conta::{Conta, le_contas};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep_until};

#[derive(Parser)]
#[command(version, about = "Testa o motor do Hexagon Call contra um PBX")]
struct Args {
    /// Arquivo de contas (TOML); usa a primeira, ou a de --usuario.
    #[arg(long)]
    contas: Option<PathBuf>,
    #[arg(long)]
    usuario: Option<String>,
    #[arg(long)]
    dominio: Option<String>,
    #[arg(long)]
    nome: Option<String>,
    /// Porta SIP local (0 = qualquer).
    #[arg(long, default_value_t = 0)]
    porta_sip: u16,
    /// Registra a conta. Tira o ramal de outro aparelho que esteja nele: o
    /// chan_sip guarda um contato so por ramal.
    #[arg(long)]
    registra: bool,
    #[command(subcommand)]
    comando: Comando,
}

#[derive(clap::Args, Clone)]
struct Audio {
    /// WAV de 8 kHz, mono, 16 bits para mandar.
    #[arg(long, conflicts_with = "tom")]
    toca: Option<PathBuf>,
    /// Um tom nesta frequencia (Hz) para mandar.
    #[arg(long)]
    tom: Option<f32>,
    /// Fala pelo microfone: o padrao do sistema, ou o id de `dispositivos`.
    #[arg(long, num_args = 0..=1, default_missing_value = "", conflicts_with_all = ["toca", "tom"])]
    microfone: Option<String>,
    /// Grava o que chega neste WAV.
    #[arg(long)]
    grava: Option<PathBuf>,
    /// Ouve pelo alto-falante: o padrao do sistema, ou o id de `dispositivos`.
    /// Junto com --grava, ouve e grava.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    alto_falante: Option<String>,
}

#[derive(Subcommand)]
enum Comando {
    /// Manda uma mensagem (SIP MESSAGE).
    Mensagem {
        texto: String,
        /// O ramal; sem ele, o SIP_DESTINO do .env.
        #[arg(long)]
        para: Option<String>,
    },
    /// Assina a presenca de um ou mais ramais (sem nenhum, o SIP_DESTINO) e
    /// mostra cada mudanca.
    Presenca {
        ramais: Vec<String>,
        #[arg(long, default_value_t = 120)]
        segundos: u64,
    },
    /// Liga, fala por um tempo e desliga.
    Liga {
        /// O ramal ou numero; sem ele, o SIP_DESTINO do .env.
        destino: Option<String>,
        #[command(flatten)]
        audio: Audio,
        /// Quanto tempo fica na ligacao depois de atendida.
        #[arg(long, default_value_t = 20)]
        segundos: u64,
        /// Quanto tempo deixa tocar antes de desistir.
        #[arg(long, default_value_t = 45)]
        toque: u64,
    },
    /// Registra e mostra o que chegar (mensagens, ligacoes).
    Escuta {
        #[arg(long, default_value_t = 300)]
        segundos: u64,
        /// Atende as ligacoes que chegarem.
        #[arg(long)]
        atende: bool,
        #[command(flatten)]
        audio: Audio,
    },
    /// Lista os microfones e alto-falantes (o * e o padrao do sistema).
    Dispositivos,
    /// Mostra o volume do microfone a cada meio segundo, sem ligar.
    Nivel {
        /// O id de `dispositivos`; sem ele, o padrao do sistema.
        #[arg(long)]
        microfone: Option<String>,
        #[arg(long, default_value_t = 5)]
        segundos: u64,
    },
}

/// O `.env` da raiz (so nesta maquina) e o ambiente, que vale mais.
struct Ambiente(HashMap<String, String>);

impl Ambiente {
    fn carrega() -> Self {
        let caminhos = [
            PathBuf::from(".env"),
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env")),
        ];
        let texto = caminhos
            .iter()
            .find_map(|c| std::fs::read_to_string(c).ok())
            .unwrap_or_default();
        Self(le_env(&texto))
    }

    fn le(&self, chave: &str) -> Option<String> {
        std::env::var(chave)
            .ok()
            .or_else(|| self.0.get(chave).cloned())
            .filter(|v| !v.is_empty())
    }

    fn destino(&self, dito: Option<String>) -> Result<String, String> {
        dito.or_else(|| self.le("SIP_DESTINO"))
            .ok_or_else(|| "diga o ramal ou defina SIP_DESTINO no .env".into())
    }
}

/// `CHAVE=valor` por linha; `#` comenta; aspas em volta do valor saem.
fn le_env(texto: &str) -> HashMap<String, String> {
    texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (chave, valor) = l.split_once('=')?;
            let chave = chave.trim().trim_start_matches("export ").trim();
            let valor = valor.trim();
            let valor = valor
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(valor);
            Some((chave.to_string(), valor.to_string()))
        })
        .collect()
}

fn conta(args: &Args, amb: &Ambiente) -> Result<Conta, String> {
    if let Some(arquivo) = &args.contas {
        let texto =
            std::fs::read_to_string(arquivo).map_err(|e| format!("{}: {e}", arquivo.display()))?;
        let contas = le_contas(&texto).map_err(|e| e.to_string())?;
        return match &args.usuario {
            Some(u) => contas.into_iter().find(|c| &c.usuario == u),
            None => contas.into_iter().next(),
        }
        .ok_or_else(|| "a conta pedida nao esta no arquivo".to_string());
    }
    let usuario = args.usuario.clone().or_else(|| amb.le("SIP_USUARIO"));
    let dominio = args.dominio.clone().or_else(|| amb.le("SIP_DOMINIO"));
    let (Some(usuario), Some(dominio)) = (usuario, dominio) else {
        return Err(
            "diga a conta: SIP_USUARIO e SIP_DOMINIO no .env, --usuario e --dominio, ou --contas"
                .into(),
        );
    };
    let mut conta = Conta::nova(&usuario, &dominio);
    conta.nome = args.nome.clone().or_else(|| amb.le("SIP_NOME"));
    Ok(conta)
}

/// O que vai e o que volta numa ligacao.
type Audios = (Box<dyn Fonte>, Box<dyn Destino>);

/// Vazio e o padrao do sistema.
fn id_do(dispositivo: &str) -> Option<&str> {
    Some(dispositivo).filter(|d| !d.is_empty())
}

fn audio(a: &Audio) -> Result<Audios, String> {
    let fonte: Box<dyn Fonte> = match (&a.toca, a.tom, &a.microfone) {
        (Some(arquivo), _, _) => Box::new(Wav::abre(arquivo).map_err(|e| e.to_string())?),
        (None, Some(hz), _) => Box::new(Tom::novo(hz, 0.3)),
        (None, None, Some(d)) => {
            Box::new(som::Microfone::abre(id_do(d)).map_err(|e| e.to_string())?)
        }
        (None, None, None) => Box::new(Silencio),
    };
    let grava: Option<Box<dyn Destino>> = match &a.grava {
        Some(arquivo) => Some(Box::new(
            GravaWav::cria(arquivo).map_err(|e| e.to_string())?,
        )),
        None => None,
    };
    let ouve: Option<Box<dyn Destino>> = match &a.alto_falante {
        Some(d) => Some(Box::new(
            som::AltoFalante::abre(id_do(d)).map_err(|e| e.to_string())?,
        )),
        None => None,
    };
    let destino: Box<dyn Destino> = match (grava, ouve) {
        (Some(g), Some(o)) => Box::new(Dois(g, o)),
        (Some(um), None) | (None, Some(um)) => um,
        (None, None) => Box::new(Silencio),
    };
    Ok((fonte, destino))
}

/// Ouve e grava ao mesmo tempo.
struct Dois(Box<dyn Destino>, Box<dyn Destino>);

impl Destino for Dois {
    fn escreve(&mut self, quadro: &Quadro) {
        self.0.escreve(quadro);
        self.1.escreve(quadro);
    }

    fn fim(&mut self) {
        self.0.fim();
        self.1.fim();
    }
}

fn dispositivos() -> Result<(), String> {
    for (titulo, lista) in [
        ("microfones", som::microfones()),
        ("alto-falantes", som::alto_falantes()),
    ] {
        println!("{titulo}:");
        for d in lista.map_err(|e| e.to_string())? {
            let marca = if d.padrao { '*' } else { ' ' };
            println!("  {marca} {}  [{}]", d.nome, d.id);
        }
    }
    Ok(())
}

fn hora() -> String {
    // Sem crate de data: segundos desde a meia-noite UTC bastam para a ordem.
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{:02}:{:02}:{:02}Z", s / 3600 % 24, s / 60 % 60, s % 60)
}

fn mostra(evento: &Evento) {
    let texto = match evento {
        Evento::Registro(Registro::Registrado { expira }) => format!("registrado ({expira} s)"),
        Evento::Registro(Registro::Falhou { codigo, motivo }) => {
            format!("registro falhou: {motivo} {codigo:?}")
        }
        Evento::Registro(Registro::Desregistrado) => "desregistrado".into(),
        Evento::Mensagem { de, nome, texto } => {
            format!(
                "mensagem de {de}{}: {texto}",
                nome.as_ref().map_or(String::new(), |n| format!(" ({n})"))
            )
        }
        Evento::Presenca { ramal, presenca } => format!("{ramal}: {}", presenca.texto()),
        Evento::PresencaEncerrada { ramal } => format!("{ramal}: assinatura encerrada"),
        Evento::Recebendo { id, de, nome } => format!(
            "ligacao de {de}{} [{id}]",
            nome.as_ref().map_or(String::new(), |n| format!(" ({n})"))
        ),
        Evento::Tocando { .. } => "tocando do outro lado".into(),
        Evento::Atendida { codec, .. } => format!("atendida ({})", codec.nome()),
        Evento::Espera { sim: true, .. } => "o outro lado pos em espera".into(),
        Evento::Espera { sim: false, .. } => "o outro lado tirou da espera".into(),
        Evento::Encerrada {
            fim, estatistica, ..
        } => match estatistica {
            Some(e) => format!(
                "encerrada: {} | enviados {} recebidos {} tocados {} perdidos {} atrasados {} \
                 esperas {} descartados {} | variacao {} ms (pior {} ms)",
                fim.texto(),
                e.enviados,
                e.jitter.recebidos,
                e.jitter.tocados,
                e.jitter.perdidos,
                e.jitter.atrasados,
                e.jitter.esperas,
                e.jitter.descartados,
                e.variacao_ms,
                e.variacao_pior_ms
            ),
            None => format!("encerrada: {}", fim.texto()),
        },
    };
    println!("{} {texto}", hora());
}

/// Espera um evento ate `limite`; None se o tempo acabou ou o motor parou.
async fn proximo(eventos: &mut UnboundedReceiver<Evento>, limite: Instant) -> Option<Evento> {
    tokio::select! {
        e = eventos.recv() => e,
        () = sleep_until(limite) => None,
        _ = tokio::signal::ctrl_c() => None,
    }
}

/// O volume do microfone em dBFS (0 e o maximo), de meio em meio segundo.
async fn nivel(microfone: Option<&str>, segundos: u64) -> Result<(), String> {
    let mut mic = som::Microfone::abre(microfone).map_err(|e| e.to_string())?;
    let mut relogio = tokio::time::interval(Duration::from_millis(20));
    let mut quadro = [0i16; motor::midia::AMOSTRAS];
    let (mut soma, mut n) = (0.0f64, 0u32);
    for _ in 0..segundos * 50 {
        relogio.tick().await;
        mic.le(&mut quadro);
        soma += quadro.iter().map(|a| f64::from(*a).powi(2)).sum::<f64>();
        n += 1;
        if n == 25 {
            let rms = (soma / (25.0 * 160.0)).sqrt() / 32768.0;
            let db = 20.0 * rms.max(1e-6).log10();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let barra = "#".repeat(((db + 60.0).max(0.0) / 2.0) as usize);
            println!("{} {db:6.1} dBFS {barra}", hora());
            (soma, n) = (0.0, 0);
        }
    }
    Ok(())
}

async fn executa(args: Args) -> Result<(), String> {
    match &args.comando {
        Comando::Dispositivos => return dispositivos(),
        Comando::Nivel {
            microfone,
            segundos,
        } => return nivel(microfone.as_deref(), *segundos).await,
        _ => {}
    }
    let amb = Ambiente::carrega();
    let conta = conta(&args, &amb)?;
    let senha = amb
        .le("SIP_SENHA")
        .ok_or("defina SIP_SENHA no .env da raiz ou no ambiente (nunca como argumento)")?;
    let registrar = args.registra || matches!(args.comando, Comando::Escuta { .. });
    let opcoes = Opcoes {
        porta_sip: args.porta_sip,
        registrar,
    };
    let (ua, mut eventos) = Ua::inicia(conta, senha, opcoes)
        .await
        .map_err(|e| e.to_string())?;
    println!("{} conta {} em {}", hora(), ua.conta().id(), ua.endereco());

    let resultado = match args.comando {
        Comando::Mensagem { texto, para } => match ua.mensagem(&amb.destino(para)?, &texto).await {
            Ok(()) => {
                println!("{} aceita pelo PBX (o SIP nao confirma a entrega)", hora());
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        },
        Comando::Presenca { ramais, segundos } => {
            let ramais = if ramais.is_empty() {
                vec![amb.destino(None)?]
            } else {
                ramais
            };
            for ramal in &ramais {
                ua.assina_presenca(ramal).await.map_err(|e| e.to_string())?;
                println!("{} assinando a presenca de {ramal}", hora());
            }
            let limite = Instant::now() + Duration::from_secs(segundos);
            while let Some(e) = proximo(&mut eventos, limite).await {
                mostra(&e);
            }
            Ok(())
        }
        Comando::Liga {
            destino,
            audio: a,
            segundos,
            toque,
        } => {
            let destino = amb.destino(destino)?;
            liga(&ua, &mut eventos, &destino, &a, segundos, toque).await
        }
        Comando::Escuta {
            segundos,
            atende,
            audio: a,
        } => {
            let limite = Instant::now() + Duration::from_secs(segundos);
            while let Some(e) = proximo(&mut eventos, limite).await {
                mostra(&e);
                if let (true, Evento::Recebendo { id, .. }) = (atende, &e) {
                    let (fonte, destino) = audio(&a)?;
                    if let Err(erro) = ua.atende(id, fonte, destino).await {
                        println!("{} nao consegui atender: {erro}", hora());
                    }
                }
            }
            Ok(())
        }
        Comando::Dispositivos | Comando::Nivel { .. } => Ok(()),
    };
    ua.encerra().await;
    // O que chegou durante o encerramento (desregistro, fim da ligacao).
    let limite = Instant::now() + Duration::from_millis(500);
    while let Some(e) = proximo(&mut eventos, limite).await {
        mostra(&e);
    }
    resultado
}

async fn liga(
    ua: &Arc<Ua>,
    eventos: &mut UnboundedReceiver<Evento>,
    destino: &str,
    a: &Audio,
    segundos: u64,
    toque: u64,
) -> Result<(), String> {
    let (fonte, saida) = audio(a)?;
    let id = ua
        .liga(destino, fonte, saida)
        .await
        .map_err(|e| e.to_string())?;
    println!("{} ligando para {destino} [{id}]", hora());
    let mut limite = Instant::now() + Duration::from_secs(toque);
    let mut atendida = false;
    loop {
        match proximo(eventos, limite).await {
            Some(e) => {
                mostra(&e);
                match e {
                    Evento::Atendida { .. } => {
                        atendida = true;
                        limite = Instant::now() + Duration::from_secs(segundos);
                    }
                    Evento::Encerrada { .. } => return Ok(()),
                    _ => {}
                }
            }
            None => {
                println!(
                    "{} {}",
                    hora(),
                    if atendida {
                        "tempo da ligacao acabou: desligando"
                    } else {
                        "ninguem atendeu: desistindo"
                    }
                );
                ua.desliga(&id).await.map_err(|e| e.to_string())?;
                let fim = Instant::now() + Duration::from_secs(5);
                while let Some(e) = proximo(eventos, fim).await {
                    mostra(&e);
                    if matches!(e, Evento::Encerrada { .. }) {
                        return Ok(());
                    }
                }
                return Ok(());
            }
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,motor=info,pulseaudio=off".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    match executa(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_env_com_comentario_aspas_e_export() {
        let env = le_env(
            "# comentario\nSIP_USUARIO=1001\n export SIP_DOMINIO = pbx.exemplo.com.br \nSIP_SENHA=\"a=b c\"\n\nlixo\n",
        );
        assert_eq!(env.get("SIP_USUARIO").map(String::as_str), Some("1001"));
        assert_eq!(
            env.get("SIP_DOMINIO").map(String::as_str),
            Some("pbx.exemplo.com.br")
        );
        assert_eq!(env.get("SIP_SENHA").map(String::as_str), Some("a=b c"));
        assert_eq!(env.len(), 3);
    }
}
