//! O audio de uma ligacao: a cada 20 ms um quadro de 160 amostras sai da
//! `Fonte`, vira G.711 e vai num pacote RTP; o que chega passa pelo jitter e
//! vai para o `Destino`. Microfone e alto-falante (cpal) entram depois como
//! mais uma Fonte e um Destino; por ora ha tom, arquivo WAV e gravacao.

use crate::erro::Erro;
use nucleo::dtmf;
use nucleo::g711;
use nucleo::jitter::{Contagem, Jitter, Variacao};
use nucleo::rtp::{self, Cabecalho};
use nucleo::sdp::Codec;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

pub const TAXA: u32 = 8000;
pub const AMOSTRAS: usize = 160;
pub type Quadro = [i16; AMOSTRAS];
const SILENCIO: Quadro = [0; AMOSTRAS];
/// Folga do jitter no comeco: 3 quadros (60 ms). Dai ele se ajusta a rede.
const ATRASO_JITTER: usize = 3;

pub trait Fonte: Send + 'static {
    /// Preenche o quadro. `false` quando acabou: dai em diante vai silencio.
    fn le(&mut self, quadro: &mut Quadro) -> bool;
}

pub trait Destino: Send + 'static {
    fn escreve(&mut self, quadro: &Quadro);
    /// Chamado uma vez, quando a ligacao acaba (fecha arquivo etc.).
    fn fim(&mut self) {}
}

/// Recebe a ligacao como ela aconteceu, a cada 20 ms: o que foi para o outro
/// lado (ja com o mudo e a espera) e o que tocou aqui. E a gravacao.
pub trait Gravador: Send + 'static {
    fn quadro(&mut self, nosso: &Quadro, deles: &Quadro);
    /// Chamado uma vez, quando a gravacao para ou a ligacao acaba.
    fn fim(&mut self) {}
}

pub struct Silencio;

impl Fonte for Silencio {
    fn le(&mut self, quadro: &mut Quadro) -> bool {
        quadro.fill(0);
        true
    }
}

impl Destino for Silencio {
    fn escreve(&mut self, _quadro: &Quadro) {}
}

/// Um tom puro: para teste de ouvido e para os testes automaticos medirem a
/// frequencia do que chegou do outro lado.
pub struct Tom {
    passo: f32,
    fase: f32,
    amplitude: f32,
}

impl Tom {
    pub fn novo(frequencia: f32, amplitude: f32) -> Self {
        // TAXA cabe exato em f32 (8000).
        #[allow(clippy::cast_precision_loss)]
        let taxa = TAXA as f32;
        Self {
            passo: std::f32::consts::TAU * frequencia / taxa,
            fase: 0.0,
            amplitude: amplitude.clamp(0.0, 1.0) * f32::from(i16::MAX),
        }
    }
}

/// f32 ja limitado a faixa do i16: a conversao so tira a parte fracionaria.
#[allow(clippy::cast_possible_truncation)]
fn amostra_de(valor: f32) -> i16 {
    valor
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

impl Fonte for Tom {
    fn le(&mut self, quadro: &mut Quadro) -> bool {
        for a in quadro.iter_mut() {
            *a = amostra_de(self.fase.sin() * self.amplitude);
            self.fase = (self.fase + self.passo) % std::f32::consts::TAU;
        }
        true
    }
}

/// Um WAV de 8 kHz, mono, 16 bits (o `ffmpeg -ar 8000 -ac 1` resolve os
/// outros). Toca uma vez e depois fica em silencio.
pub struct Wav {
    amostras: Vec<i16>,
    posicao: usize,
}

impl Wav {
    pub fn abre(caminho: &Path) -> Result<Self, Erro> {
        let leitor = hound::WavReader::open(caminho).map_err(|e| Erro::Audio(e.to_string()))?;
        let spec = leitor.spec();
        if spec.sample_rate != TAXA || spec.channels != 1 || spec.bits_per_sample != 16 {
            return Err(Erro::Audio(format!(
                "{} e {} Hz, {} canal(is), {} bits; precisa de 8000 Hz, mono, 16 bits \
                 (ffmpeg -i entrada -ar 8000 -ac 1 -sample_fmt s16 saida.wav)",
                caminho.display(),
                spec.sample_rate,
                spec.channels,
                spec.bits_per_sample
            )));
        }
        let amostras = leitor
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Erro::Audio(e.to_string()))?;
        Ok(Self {
            amostras,
            posicao: 0,
        })
    }
}

impl Fonte for Wav {
    fn le(&mut self, quadro: &mut Quadro) -> bool {
        let resto = self.amostras.get(self.posicao..).unwrap_or_default();
        let n = resto.len().min(AMOSTRAS);
        for (i, a) in quadro.iter_mut().enumerate() {
            *a = if i < n {
                resto.get(i).copied().unwrap_or(0)
            } else {
                0
            };
        }
        self.posicao += n;
        n > 0
    }
}

/// Grava o que chega num WAV de 8 kHz.
pub struct GravaWav {
    escritor: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
}

impl GravaWav {
    pub fn cria(caminho: &Path) -> Result<Self, Erro> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: TAXA,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let escritor =
            hound::WavWriter::create(caminho, spec).map_err(|e| Erro::Audio(e.to_string()))?;
        Ok(Self {
            escritor: Some(escritor),
        })
    }
}

impl Destino for GravaWav {
    fn escreve(&mut self, quadro: &Quadro) {
        if let Some(escritor) = self.escritor.as_mut() {
            for a in quadro {
                if escritor.write_sample(*a).is_err() {
                    warn!("gravacao interrompida");
                    self.escritor = None;
                    return;
                }
            }
        }
    }

    fn fim(&mut self) {
        if let Some(escritor) = self.escritor.take()
            && let Err(e) = escritor.finalize()
        {
            warn!("nao consegui fechar a gravacao: {e}");
        }
    }
}

/// Guarda tudo o que chegou, para os testes conferirem.
#[derive(Clone, Default)]
pub struct Captura {
    pub amostras: Arc<std::sync::Mutex<Vec<i16>>>,
}

impl Destino for Captura {
    fn escreve(&mut self, quadro: &Quadro) {
        if let Ok(mut v) = self.amostras.lock() {
            v.extend_from_slice(quadro);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estatistica {
    pub enviados: u64,
    pub jitter: Contagem,
    /// Variacao entre chegadas (RFC 3550) ao fim da ligacao e a maior, em ms.
    pub variacao_ms: u64,
    pub variacao_pior_ms: u64,
    /// Pacotes de tom do teclado (RFC 4733) mandados.
    pub tons: u64,
    /// Pacotes que nao eram do codec da ligacao (DTMF, ruido de conforto).
    pub outros: u64,
    pub invalidos: u64,
}

/// O que mandar a cada 20 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envio {
    /// O que a fonte da (o microfone).
    Audio,
    /// Silencio: estamos com a ligacao em espera. Os pacotes seguem indo,
    /// para o firewall nao fechar, mas sem a voz de quem esta aqui; e o que
    /// chega nao toca (o Asterisk segue mandando o audio do outro lado).
    Silencio,
    /// Nada: o outro lado nos pos em espera (sendonly, inactive).
    Nada,
}

/// Para onde e como mandar; muda no meio da ligacao (re-INVITE).
#[derive(Debug, Clone, Copy)]
struct Rumo {
    remoto: SocketAddr,
    codec: Codec,
    envio: Envio,
}

/// Uma fonte nova esperando o proximo quadro (trocar o microfone no meio da
/// ligacao).
type FonteNova = Arc<std::sync::Mutex<Option<Box<dyn Fonte>>>>;
/// Os tons do teclado (eventos da RFC 4733) esperando a vez.
type Tons = Arc<std::sync::Mutex<std::collections::VecDeque<u8>>>;
/// A gravacao, que comeca e para no meio da ligacao.
type Gravacao = Arc<std::sync::Mutex<Option<Box<dyn Gravador>>>>;

/// O pico de cada lado desde a ultima leitura (0 a 32767): a tela mostra quem
/// esta falando. Um `fetch_max` por quadro, de 20 em 20 ms.
#[derive(Default)]
struct Niveis {
    nosso: AtomicU32,
    deles: AtomicU32,
}

/// O que a ligacao muda de fora no laco do audio.
#[derive(Clone)]
struct Controles {
    cancela: CancellationToken,
    mudo: Arc<AtomicBool>,
    fonte_nova: FonteNova,
    tons: Tons,
    gravacao: Gravacao,
    niveis: Arc<Niveis>,
}

/// A maior amplitude do quadro.
fn pico(quadro: &Quadro) -> u32 {
    quadro
        .iter()
        .map(|a| u32::from(a.unsigned_abs()))
        .max()
        .unwrap_or(0)
}

pub struct Sessao {
    controles: Controles,
    rumo: watch::Sender<Rumo>,
    /// O tipo do telephone-event que o outro lado combinou no SDP.
    tipo_dtmf: Option<u8>,
    tarefa: JoinHandle<Estatistica>,
}

/// Um tom do teclado saindo: o carimbo e o do comeco, e cada quadro manda um
/// pacote com a duracao maior (os ultimos, com o fim).
struct TomEmCurso {
    evento: u8,
    carimbo: u32,
    quadro: u16,
}

impl Sessao {
    /// Comeca a mandar e receber. `remoto` e o que o SDP disse; se os pacotes
    /// vierem de outro endereco, passamos a responder para la (RTP simetrico).
    /// `tipo_dtmf`: o telephone-event que o outro lado aceita (None: sem tons).
    pub fn inicia(
        socket: UdpSocket,
        remoto: SocketAddr,
        codec: Codec,
        tipo_dtmf: Option<u8>,
        fonte: Box<dyn Fonte>,
        destino: Box<dyn Destino>,
    ) -> Self {
        let (rumo, rumo_rx) = watch::channel(Rumo {
            remoto,
            codec,
            envio: Envio::Audio,
        });
        let controles = Controles {
            cancela: CancellationToken::new(),
            mudo: Arc::new(AtomicBool::new(false)),
            fonte_nova: Arc::default(),
            tons: Arc::default(),
            gravacao: Arc::default(),
            niveis: Arc::default(),
        };
        let tarefa = tokio::spawn(laco(
            socket,
            rumo_rx,
            fonte,
            destino,
            controles.clone(),
            tipo_dtmf,
        ));
        Self {
            controles,
            rumo,
            tipo_dtmf,
            tarefa,
        }
    }

    /// Manda tons do teclado (0-9, *, #, A-D); o resto e ignorado. Devolve
    /// quantos entraram na fila.
    pub fn tons(&self, teclas: &str) -> Result<usize, Erro> {
        if self.tipo_dtmf.is_none() {
            return Err(Erro::SemDtmf);
        }
        let eventos: Vec<u8> = teclas.chars().filter_map(dtmf::evento).collect();
        if let Ok(mut fila) = self.controles.tons.lock() {
            fila.extend(&eventos);
        }
        Ok(eventos.len())
    }

    /// Troca o que vai para o outro lado (outro microfone) sem parar a ligacao.
    pub fn troca_fonte(&self, fonte: Box<dyn Fonte>) {
        if let Ok(mut f) = self.controles.fonte_nova.lock() {
            *f = Some(fonte);
        }
    }

    /// Comeca a gravar (`Some`) ou para (`None`); a gravacao de antes, se
    /// havia, e fechada.
    pub fn grava(&self, gravador: Option<Box<dyn Gravador>>) {
        let antiga = match self.controles.gravacao.lock() {
            Ok(mut g) => std::mem::replace(&mut *g, gravador),
            Err(_) => return,
        };
        if let Some(mut g) = antiga {
            g.fim();
        }
    }

    pub fn mudo(&self, sim: bool) {
        self.controles.mudo.store(sim, Ordering::Relaxed);
    }

    /// O pico do que saiu (ja com o mudo) e do que tocou aqui desde a ultima
    /// leitura, de 0 a 32767; ler zera.
    pub fn niveis(&self) -> (u16, u16) {
        let le = |n: &AtomicU32| u16::try_from(n.swap(0, Ordering::Relaxed)).unwrap_or(u16::MAX);
        (
            le(&self.controles.niveis.nosso),
            le(&self.controles.niveis.deles),
        )
    }

    /// Depois de uma renegociacao: o endereco novo do outro lado (None mantem
    /// o de agora), o codec e o que mandar.
    pub fn ajusta(&self, remoto: Option<SocketAddr>, codec: Option<Codec>, envio: Envio) {
        self.rumo.send_modify(|r| {
            if let Some(remoto) = remoto {
                r.remoto = remoto;
            }
            if let Some(codec) = codec {
                r.codec = codec;
            }
            r.envio = envio;
        });
    }

    pub async fn encerra(self) -> Estatistica {
        self.controles.cancela.cancel();
        self.tarefa.await.unwrap_or_default()
    }
}

fn codifica(codec: Codec, quadro: &Quadro, saida: &mut Vec<u8>) {
    saida.clear();
    match codec {
        Codec::Pcma => saida.extend(quadro.iter().map(|a| g711::alaw_de_linear(*a))),
        Codec::Pcmu => saida.extend(quadro.iter().map(|a| g711::ulaw_de_linear(*a))),
    }
}

fn decodifica(codec: Codec, carga: &[u8]) -> Quadro {
    let mut quadro = SILENCIO;
    for (a, c) in quadro.iter_mut().zip(carga) {
        *a = match codec {
            Codec::Pcma => g711::linear_de_alaw(*c),
            Codec::Pcmu => g711::linear_de_ulaw(*c),
        };
    }
    quadro
}

async fn laco(
    socket: UdpSocket,
    mut rumo: watch::Receiver<Rumo>,
    mut fonte: Box<dyn Fonte>,
    mut destino: Box<dyn Destino>,
    controles: Controles,
    tipo_dtmf: Option<u8>,
) -> Estatistica {
    let Controles {
        cancela,
        mudo,
        fonte_nova,
        tons,
        gravacao,
        niveis,
    } = controles;
    let mut tom: Option<TomEmCurso> = None;
    let mut pausa: u16 = 0;
    let Rumo {
        mut remoto,
        mut codec,
        mut envio,
    } = *rumo.borrow_and_update();
    // O que o SDP disse; `remoto` pode virar o endereco de onde os pacotes vem.
    let mut remoto_sdp = remoto;
    let mut estatistica = Estatistica::default();
    let mut jitter: Jitter<Quadro> = Jitter::novo(ATRASO_JITTER);
    let mut variacao = Variacao::default();
    let inicio = Instant::now();
    let mut cabecalho = Cabecalho {
        tipo: codec.tipo(),
        marcador: true,
        sequencia: rand::random(),
        carimbo: rand::random(),
        ssrc: rand::random(),
    };
    let mut quadro = SILENCIO;
    let mut carga = Vec::with_capacity(AMOSTRAS);
    let mut pacote = Vec::with_capacity(rtp::TAMANHO_CABECALHO + AMOSTRAS);
    let mut entrada = [0u8; 2048];
    let mut fonte_acabou = false;
    let mut latched = false;
    let mut relogio = tokio::time::interval(Duration::from_millis(20));

    loop {
        tokio::select! {
            () = cancela.cancelled() => break,
            Ok(()) = rumo.changed() => {
                let novo = *rumo.borrow_and_update();
                if novo.remoto != remoto_sdp {
                    debug!("rtp: o outro lado agora recebe em {}", novo.remoto);
                    remoto_sdp = novo.remoto;
                    remoto = novo.remoto;
                    latched = false;
                }
                if novo.codec != codec {
                    codec = novo.codec;
                    cabecalho.tipo = codec.tipo();
                }
                if novo.envio != envio && envio == Envio::Nada {
                    // Volta da espera: o marcador avisa o salto no carimbo.
                    cabecalho.marcador = true;
                }
                envio = novo.envio;
            }
            _ = relogio.tick() => {
                if let Ok(mut f) = fonte_nova.try_lock()
                    && let Some(nova) = f.take()
                {
                    fonte = nova;
                    fonte_acabou = false;
                }
                if fonte_acabou || !fonte.le(&mut quadro) {
                    fonte_acabou = true;
                    quadro = SILENCIO;
                }
                if envio != Envio::Audio || mudo.load(Ordering::Relaxed) {
                    quadro = SILENCIO;
                }
                // Um tom do teclado na vez deste quadro vai no lugar do audio.
                if tom.is_none() && envio != Envio::Nada {
                    if pausa > 0 {
                        pausa -= 1;
                    } else if tipo_dtmf.is_some()
                        && let Some(evento) = tons.try_lock().ok().and_then(|mut f| f.pop_front())
                    {
                        tom = Some(TomEmCurso { evento, carimbo: cabecalho.carimbo, quadro: 0 });
                    }
                }
                let mut mandou_tom = false;
                if let (Some(t), Some(tipo)) = (tom.as_mut(), tipo_dtmf) {
                    let fim = t.quadro >= dtmf::QUADROS_DO_TOM;
                    let duracao = 160 * (t.quadro.min(dtmf::QUADROS_DO_TOM - 1) + 1);
                    let evento = Cabecalho {
                        tipo,
                        marcador: t.quadro == 0,
                        sequencia: cabecalho.sequencia,
                        carimbo: t.carimbo,
                        ssrc: cabecalho.ssrc,
                    };
                    evento.escreve(&dtmf::carga(t.evento, fim, duracao), &mut pacote);
                    match socket.send_to(&pacote, remoto).await {
                        Ok(_) => estatistica.tons += 1,
                        Err(e) => debug!("rtp: tom nao saiu: {e}"),
                    }
                    cabecalho.sequencia = cabecalho.sequencia.wrapping_add(1);
                    t.quadro += 1;
                    if t.quadro >= dtmf::QUADROS_DO_TOM + dtmf::FINS {
                        tom = None;
                        pausa = dtmf::PAUSA;
                    }
                    mandou_tom = true;
                }
                if envio != Envio::Nada && !mandou_tom {
                    codifica(codec, &quadro, &mut carga);
                    cabecalho.escreve(&carga, &mut pacote);
                    match socket.send_to(&pacote, remoto).await {
                        Ok(_) => estatistica.enviados += 1,
                        Err(e) => debug!("rtp: envio falhou: {e}"),
                    }
                    cabecalho.marcador = false;
                    cabecalho.sequencia = cabecalho.sequencia.wrapping_add(1);
                }
                cabecalho.carimbo = cabecalho.carimbo.wrapping_add(160);
                let tocado = jitter.retira().unwrap_or(SILENCIO);
                // Quem pos em espera nao ouve o outro lado.
                let ouvido = if envio == Envio::Silencio { &SILENCIO } else { &tocado };
                destino.escreve(ouvido);
                niveis.nosso.fetch_max(pico(&quadro), Ordering::Relaxed);
                niveis.deles.fetch_max(pico(ouvido), Ordering::Relaxed);
                if let Ok(mut g) = gravacao.lock()
                    && let Some(g) = g.as_mut()
                {
                    g.quadro(&quadro, ouvido);
                }
            }
            recebido = socket.recv_from(&mut entrada) => {
                let Ok((n, de)) = recebido else { continue };
                let Some(bytes) = entrada.get(..n) else { continue };
                match rtp::le(bytes) {
                    Ok((cab, carga_recebida)) if cab.tipo == codec.tipo() => {
                        if !latched && de != remoto {
                            debug!("rtp: o outro lado manda de {de}, nao de {remoto}; respondo para la");
                            remoto = de;
                        }
                        latched = true;
                        // 125 us por amostra a 8 kHz.
                        let chegada = u32::try_from(inicio.elapsed().as_micros() / 125).unwrap_or(u32::MAX);
                        variacao.chegou(chegada, cab.carimbo);
                        jitter.insere(cab.sequencia, decodifica(codec, carga_recebida));
                    }
                    Ok(_) => estatistica.outros += 1,
                    Err(_) => estatistica.invalidos += 1,
                }
            }
        }
    }
    destino.fim();
    if let Some(mut g) = gravacao.lock().ok().and_then(|mut g| g.take()) {
        g.fim();
    }
    estatistica.jitter = jitter.contagem();
    (estatistica.variacao_ms, estatistica.variacao_pior_ms) = variacao.ms(TAXA);
    estatistica
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tom_e_periodico_e_cabe_no_i16() {
        let mut tom = Tom::novo(1000.0, 1.0);
        let mut q = SILENCIO;
        assert!(tom.le(&mut q));
        // 1 kHz a 8 kHz: periodo de 8 amostras.
        assert_eq!(q[0], 0);
        assert!((q[2] - i16::MAX).abs() <= 1);
        assert!((i32::from(q[8]) - i32::from(q[0])).abs() <= 2);
    }

    #[test]
    fn ida_e_volta_pelo_g711() {
        let mut tom = Tom::novo(440.0, 0.5);
        let mut q = SILENCIO;
        tom.le(&mut q);
        for codec in [Codec::Pcma, Codec::Pcmu] {
            let mut bytes = Vec::new();
            codifica(codec, &q, &mut bytes);
            assert_eq!(bytes.len(), AMOSTRAS);
            let volta = decodifica(codec, &bytes);
            for (a, b) in q.iter().zip(volta.iter()) {
                assert!((i32::from(*a) - i32::from(*b)).abs() < 600);
            }
        }
    }

    #[test]
    fn wav_curto_completa_com_silencio_e_avisa_o_fim() {
        let mut wav = Wav {
            amostras: vec![5; 200],
            posicao: 0,
        };
        let mut q = SILENCIO;
        assert!(wav.le(&mut q));
        assert!(wav.le(&mut q));
        assert_eq!(q[39], 5);
        assert_eq!(q[40], 0);
        assert!(!wav.le(&mut q));
    }
}
