//! Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
//! Licenca GNU GPL 3 ou posterior.
//!
//! O som do computador para o telefone: o microfone vira uma `Fonte` do motor
//! e o alto-falante um `Destino`. Por baixo, o cpal: no Linux, o PipeWire (ou
//! o PulseAudio) pelo protocolo do PulseAudio, com o ALSA de reserva quando
//! nao ha servidor; WASAPI no Windows. Pelo ALSA direto, uma placa que o
//! PipeWire esta usando responde "ocupada", e os nomes sao os dos plugins; pelo
//! PulseAudio, cada saida e entrada aparece com o nome que o sistema mostra.
//!
//! A placa trabalha no ritmo dela, numa thread de audio que nao pode esperar
//! por nada; o motor trabalha de 20 em 20 ms. Entre os dois, uma fila sem
//! trava (rtrb) e a `conversao` de taxa. Os dois relogios nunca andam iguais:
//! cada lado segura uma folga pequena e joga fora o que passa dela, para o
//! atraso nao crescer ao longo da ligacao.

pub mod conversao;
pub mod diagnostico;
pub mod eco;
pub mod gravacao;
mod pactl;
pub mod toque;

use conversao::{DoTelefone, ParaTelefone};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use eco::{Clareza, Limpador, Limpeza, Referencia};
use motor::midia::{Destino, Fonte, Quadro};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Debug, Error)]
pub enum ErroSom {
    #[error("nenhum {0} no sistema")]
    SemDispositivo(&'static str),
    #[error("dispositivo de som nao encontrado: {0}")]
    NaoAchei(String),
    #[error("formato de som nao suportado: {0}")]
    Formato(String),
    #[error("som: {0}")]
    Cpal(#[from] cpal::Error),
    #[error("conversao de taxa: {0}")]
    Conversao(String),
}

/// Um microfone ou alto-falante, para a lista das configuracoes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispositivo {
    /// O que se passa para abrir (`alsa:...`, `wasapi:...`).
    pub id: String,
    pub nome: String,
    pub padrao: bool,
}

pub fn microfones() -> Result<Vec<Dispositivo>, ErroSom> {
    lista(Sentido::Entrada)
}

pub fn alto_falantes() -> Result<Vec<Dispositivo>, ErroSom> {
    lista(Sentido::Saida)
}

#[derive(Clone, Copy)]
enum Sentido {
    Entrada,
    Saida,
}

fn lista(sentido: Sentido) -> Result<Vec<Dispositivo>, ErroSom> {
    let host = cpal::default_host();
    let (padrao, dispositivos) = match sentido {
        Sentido::Entrada => (host.default_input_device(), host.input_devices()?),
        Sentido::Saida => (host.default_output_device(), host.output_devices()?),
    };
    let padrao = padrao.and_then(|d| d.id().ok());
    Ok(dispositivos
        .filter_map(|d| {
            let id = d.id().ok()?;
            // O "monitor" de uma saida (o que ela toca) nao e microfone.
            if id.to_string().ends_with(".monitor") {
                return None;
            }
            let nome = d
                .description()
                .map_or_else(|_| id.to_string(), |x| x.name().to_string());
            Some(Dispositivo {
                padrao: padrao.as_ref() == Some(&id),
                id: id.to_string(),
                nome,
            })
        })
        .collect())
}

/// O dispositivo pelo id da lista, ou o padrao do sistema. Um id que sumiu
/// (headset desligado, outro host) cai no padrao: ligacao com som vale mais
/// que ligacao que nao sai.
fn escolhe(id: Option<&str>, sentido: Sentido) -> Result<cpal::Device, ErroSom> {
    let host = cpal::default_host();
    if let Some(id) = id {
        let achado = id.parse().ok().and_then(|chave| host.device_by_id(&chave));
        match achado {
            Some(d) => return Ok(d),
            None => warn!(id, "dispositivo nao encontrado: vai o padrao do sistema"),
        }
    }
    match sentido {
        Sentido::Entrada => host
            .default_input_device()
            .ok_or(ErroSom::SemDispositivo("microfone")),
        Sentido::Saida => host
            .default_output_device()
            .ok_or(ErroSom::SemDispositivo("alto-falante")),
    }
}

/// A configuracao padrao da placa, com buffer de 20 ms no PulseAudio: o
/// padrao do servidor para tocar chega a 2 s, o que numa ligacao e eco de si
/// mesmo. No ALSA e no WASAPI fica o que o dispositivo escolhe.
fn configuracao(
    dispositivo: &cpal::Device,
    suportado: cpal::SupportedStreamConfig,
) -> StreamConfig {
    let taxa = suportado.sample_rate();
    let mut config: StreamConfig = suportado.into();
    let pulse = dispositivo
        .id()
        .is_ok_and(|id| id.to_string().starts_with("pulseaudio:"));
    if pulse {
        config.buffer_size = BufferSize::Fixed(taxa / 50);
    }
    config
}

/// Os dispositivos de saida, para tocar em todos.
pub(crate) fn todas_as_saidas() -> Vec<cpal::Device> {
    cpal::default_host()
        .output_devices()
        .map(Iterator::collect)
        .unwrap_or_default()
}

pub(crate) fn saida(id: Option<&str>) -> Result<cpal::Device, ErroSom> {
    escolhe(id, Sentido::Saida)
}

pub(crate) fn configuracao_de_saida(d: &cpal::Device) -> Result<StreamConfig, ErroSom> {
    Ok(configuracao(d, d.default_output_config()?))
}

fn amostras_em(taxa: u32, ms: u32) -> usize {
    usize::try_from(u64::from(taxa) * u64::from(ms) / 1000).unwrap_or(4800)
}

fn nome_de(d: &cpal::Device) -> String {
    d.description()
        .map_or_else(|_| "?".into(), |x| x.name().to_string())
}

/// O microfone: a thread de audio poe as amostras (ja em mono) na fila, e o
/// motor tira um quadro de 20 ms por vez.
pub struct Microfone {
    _fluxo: cpal::Stream,
    fila: Consumer<f32>,
    conversor: ParaTelefone,
    entrada: Vec<f32>,
    /// Sem folga (comeco, ou a placa atrasou): manda silencio ate juntar.
    enchendo: bool,
    folga: usize,
    folga_maxima: usize,
    silencio: Arc<AtomicU32>,
    /// O eco e o ruido (eco.rs); None sem nada a limpar.
    limpador: Option<Limpador>,
}

/// Ha quanto tempo o microfone so manda silencio absoluto (zeros): o que um
/// headset faz com a haste levantada ou mudo no proprio fio, e o que a placa
/// entrega quando nao chega nada. A janela avisa "nao te ouco".
#[derive(Clone, Default)]
pub struct Medidor(Arc<AtomicU32>);

impl Medidor {
    pub fn segundos_em_silencio(&self) -> u32 {
        self.0.load(Ordering::Relaxed) / 50
    }
}

impl Microfone {
    pub fn medidor(&self) -> Medidor {
        Medidor(Arc::clone(&self.silencio))
    }

    /// `id` da lista de `microfones()`; `None` e o padrao do sistema. Sem
    /// limpeza (o teste do microfone mede o que ele capta de verdade).
    pub fn abre(id: Option<&str>) -> Result<Self, ErroSom> {
        let nada = Limpeza {
            eco: false,
            ruido: false,
            nitida: false,
        };
        Self::abre_limpo(id, nada, None)
    }

    /// O microfone da ligacao: tira o eco com a `referencia` do alto-falante
    /// da mesma ligacao e o ruido, como a `limpeza` mandar.
    pub fn abre_limpo(
        id: Option<&str>,
        limpeza: Limpeza,
        referencia: Option<&Referencia>,
    ) -> Result<Self, ErroSom> {
        let dispositivo = escolhe(id, Sentido::Entrada)?;
        let suportado = dispositivo.default_input_config()?;
        let taxa = suportado.sample_rate();
        let canais = usize::from(suportado.channels().max(1));
        let formato = suportado.sample_format();
        let config = configuracao(&dispositivo, suportado);
        // Meio segundo de fila: so enche se o motor parar de ler.
        let (produtor, fila) = RingBuffer::new(amostras_em(taxa, 500));
        let fluxo = match formato {
            SampleFormat::F32 => captura::<f32>(&dispositivo, config, canais, produtor)?,
            SampleFormat::I16 => captura::<i16>(&dispositivo, config, canais, produtor)?,
            SampleFormat::I32 => captura::<i32>(&dispositivo, config, canais, produtor)?,
            SampleFormat::U16 => captura::<u16>(&dispositivo, config, canais, produtor)?,
            outro => return Err(ErroSom::Formato(outro.to_string())),
        };
        fluxo.play()?;
        info!(nome = nome_de(&dispositivo), taxa, canais, %formato, "microfone aberto");
        Ok(Self {
            _fluxo: fluxo,
            fila,
            conversor: ParaTelefone::novo(taxa)?,
            entrada: Vec::new(),
            enchendo: true,
            folga: amostras_em(taxa, 40),
            folga_maxima: amostras_em(taxa, 120),
            silencio: Arc::default(),
            limpador: Limpador::novo(taxa, limpeza, referencia.cloned()),
        })
    }
}

fn captura<T>(
    dispositivo: &cpal::Device,
    config: StreamConfig,
    canais: usize,
    mut fila: Producer<f32>,
) -> Result<cpal::Stream, ErroSom>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let peso = 1.0 / f32::from(u16::try_from(canais).unwrap_or(1));
    let fluxo = dispositivo.build_input_stream(
        config,
        move |dados: &[T], _: &cpal::InputCallbackInfo| {
            for quadro in dados.chunks_exact(canais) {
                let soma: f32 = quadro.iter().map(|s| f32::from_sample(*s)).sum();
                // Fila cheia: o motor parou de ler (a ligacao acabou); perde.
                let _ = fila.push(soma * peso);
            }
        },
        |e| warn!("microfone: {e}"),
        None,
    )?;
    Ok(fluxo)
}

impl Fonte for Microfone {
    fn le(&mut self, quadro: &mut Quadro) -> bool {
        self.le_da_placa(quadro);
        // Um bit de diferenca e o arredondamento da conversao, nao som.
        if quadro.iter().all(|a| a.unsigned_abs() <= 1) {
            self.silencio.fetch_add(1, Ordering::Relaxed);
        } else {
            self.silencio.store(0, Ordering::Relaxed);
        }
        true
    }
}

impl Microfone {
    fn le_da_placa(&mut self, quadro: &mut Quadro) {
        let precisa = self.conversor.precisa();
        let tem = self.fila.slots();
        if tem > precisa + self.folga_maxima {
            // A placa anda mais rapido que o relogio do motor: joga fora o
            // mais velho em vez de deixar o atraso crescer.
            let fora = tem - precisa - self.folga;
            if let Ok(velho) = self.fila.read_chunk(fora) {
                velho.commit_all();
                // A referencia do eco perde o mesmo tempo, no mesmo ponto.
                if let Some(l) = self.limpador.as_mut() {
                    l.descarta(fora);
                }
            }
        }
        let tem = self.fila.slots();
        if (self.enchendo && tem < precisa + self.folga) || tem < precisa {
            self.enchendo = true;
            quadro.fill(0);
            return;
        }
        self.enchendo = false;
        self.entrada.resize(precisa, 0.0);
        let lido = self.fila.pop_entire_slice(&mut self.entrada);
        if lido.is_ok()
            && let Some(l) = self.limpador.as_mut()
        {
            l.processa(&mut self.entrada);
        }
        if lido.is_err() || self.conversor.converte(&self.entrada, quadro).is_err() {
            debug!("microfone: quadro perdido na conversao");
            quadro.fill(0);
        }
    }
}

/// O alto-falante: o motor entrega um quadro de 20 ms por vez, e a thread de
/// audio tira da fila no ritmo da placa (silencio se faltar).
pub struct AltoFalante {
    _fluxo: cpal::Stream,
    fila: Producer<f32>,
    conversor: DoTelefone,
    capacidade: usize,
    folga_maxima: usize,
    /// A voz mais nitida (eco.rs); None desligada.
    clareza: Option<Clareza>,
}

impl AltoFalante {
    /// `id` da lista de `alto_falantes()`; `None` e o padrao do sistema.
    pub fn abre(id: Option<&str>) -> Result<Self, ErroSom> {
        Self::abre_limpo(id, Limpeza::default(), None)
    }

    /// O alto-falante da ligacao: poe na `referencia` o que tocou (para o
    /// microfone tirar o eco) e, com `limpeza.nitida`, limpa o que chega.
    pub fn abre_limpo(
        id: Option<&str>,
        limpeza: Limpeza,
        referencia: Option<&Referencia>,
    ) -> Result<Self, ErroSom> {
        let dispositivo = escolhe(id, Sentido::Saida)?;
        let suportado = dispositivo.default_output_config()?;
        let taxa = suportado.sample_rate();
        let canais = usize::from(suportado.channels().max(1));
        let formato = suportado.sample_format();
        let config = configuracao(&dispositivo, suportado);
        let capacidade = amostras_em(taxa, 500);
        let (mut fila, consumidor) = RingBuffer::new(capacidade);
        // 40 ms de silencio na frente: o motor entrega de 20 em 20 ms e a
        // placa pede no ritmo dela; sem isso, falta no primeiro pedido.
        for _ in 0..amostras_em(taxa, 40) {
            let _ = fila.push(0.0);
        }
        let tocou = referencia.map(|r| r.para_o_alto_falante(taxa));
        let fluxo = match formato {
            SampleFormat::F32 => toca::<f32>(&dispositivo, config, canais, consumidor, tocou)?,
            SampleFormat::I16 => toca::<i16>(&dispositivo, config, canais, consumidor, tocou)?,
            SampleFormat::I32 => toca::<i32>(&dispositivo, config, canais, consumidor, tocou)?,
            SampleFormat::U16 => toca::<u16>(&dispositivo, config, canais, consumidor, tocou)?,
            outro => return Err(ErroSom::Formato(outro.to_string())),
        };
        fluxo.play()?;
        info!(nome = nome_de(&dispositivo), taxa, canais, %formato, "alto-falante aberto");
        Ok(Self {
            _fluxo: fluxo,
            fila,
            conversor: DoTelefone::novo(taxa)?,
            capacidade,
            folga_maxima: amostras_em(taxa, 120),
            clareza: if limpeza.nitida {
                Clareza::nova(taxa)
            } else {
                None
            },
        })
    }
}

fn toca<T>(
    dispositivo: &cpal::Device,
    config: StreamConfig,
    canais: usize,
    mut fila: Consumer<f32>,
    mut tocou: Option<Producer<f32>>,
) -> Result<cpal::Stream, ErroSom>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let fluxo = dispositivo.build_output_stream(
        config,
        move |dados: &mut [T], _: &cpal::OutputCallbackInfo| {
            for quadro in dados.chunks_exact_mut(canais) {
                let s = fila.pop().unwrap_or(0.0);
                // A referencia do eco: exatamente o que tocou, ate o silencio.
                if let Some(r) = tocou.as_mut() {
                    let _ = r.push(s);
                }
                quadro.fill(T::from_sample(s));
            }
        },
        |e| warn!("alto-falante: {e}"),
        None,
    )?;
    Ok(fluxo)
}

impl Destino for AltoFalante {
    fn escreve(&mut self, quadro: &Quadro) {
        let na_fila = self.capacidade - self.fila.slots();
        if na_fila > self.folga_maxima {
            // O relogio do motor anda mais rapido que a placa: pula este
            // quadro em vez de deixar o atraso crescer.
            return;
        }
        match self.conversor.converte(quadro) {
            Ok(amostras) => {
                let amostras = match self.clareza.as_mut() {
                    Some(c) => c.processa(amostras),
                    None => amostras,
                };
                let _ = self.fila.push_partial_slice(amostras);
            }
            Err(e) => debug!("alto-falante: {e}"),
        }
    }
}
