//! Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
//! Licenca GNU GPL 3 ou posterior.
//!
//! O som do computador para o telefone: o microfone vira uma `Fonte` do motor
//! e o alto-falante um `Destino`. Por baixo, o cpal: ALSA no Linux (e o
//! PipeWire ou o PulseAudio por ele) e WASAPI no Windows.
//!
//! A placa trabalha no ritmo dela, numa thread de audio que nao pode esperar
//! por nada; o motor trabalha de 20 em 20 ms. Entre os dois, uma fila sem
//! trava (rtrb) e a `conversao` de taxa. Os dois relogios nunca andam iguais:
//! cada lado segura uma folga pequena e joga fora o que passa dela, para o
//! atraso nao crescer ao longo da ligacao.

pub mod conversao;

use conversao::{DoTelefone, ParaTelefone};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
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

/// O dispositivo pelo id da lista, ou o padrao do sistema.
fn escolhe(id: Option<&str>, sentido: Sentido) -> Result<cpal::Device, ErroSom> {
    let host = cpal::default_host();
    match id {
        Some(id) => {
            let chave = id.parse().map_err(|_| ErroSom::NaoAchei(id.to_string()))?;
            host.device_by_id(&chave)
                .ok_or_else(|| ErroSom::NaoAchei(id.to_string()))
        }
        None => match sentido {
            Sentido::Entrada => host
                .default_input_device()
                .ok_or(ErroSom::SemDispositivo("microfone")),
            Sentido::Saida => host
                .default_output_device()
                .ok_or(ErroSom::SemDispositivo("alto-falante")),
        },
    }
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

    /// `id` da lista de `microfones()`; `None` e o padrao do sistema.
    pub fn abre(id: Option<&str>) -> Result<Self, ErroSom> {
        let dispositivo = escolhe(id, Sentido::Entrada)?;
        let suportado = dispositivo.default_input_config()?;
        let taxa = suportado.sample_rate();
        let canais = usize::from(suportado.channels().max(1));
        let formato = suportado.sample_format();
        let config: StreamConfig = suportado.into();
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
            if let Ok(velho) = self.fila.read_chunk(tem - precisa - self.folga) {
                velho.commit_all();
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
}

impl AltoFalante {
    /// `id` da lista de `alto_falantes()`; `None` e o padrao do sistema.
    pub fn abre(id: Option<&str>) -> Result<Self, ErroSom> {
        let dispositivo = escolhe(id, Sentido::Saida)?;
        let suportado = dispositivo.default_output_config()?;
        let taxa = suportado.sample_rate();
        let canais = usize::from(suportado.channels().max(1));
        let formato = suportado.sample_format();
        let config: StreamConfig = suportado.into();
        let capacidade = amostras_em(taxa, 500);
        let (mut fila, consumidor) = RingBuffer::new(capacidade);
        // 40 ms de silencio na frente: o motor entrega de 20 em 20 ms e a
        // placa pede no ritmo dela; sem isso, falta no primeiro pedido.
        for _ in 0..amostras_em(taxa, 40) {
            let _ = fila.push(0.0);
        }
        let fluxo = match formato {
            SampleFormat::F32 => toca::<f32>(&dispositivo, config, canais, consumidor)?,
            SampleFormat::I16 => toca::<i16>(&dispositivo, config, canais, consumidor)?,
            SampleFormat::I32 => toca::<i32>(&dispositivo, config, canais, consumidor)?,
            SampleFormat::U16 => toca::<u16>(&dispositivo, config, canais, consumidor)?,
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
        })
    }
}

fn toca<T>(
    dispositivo: &cpal::Device,
    config: StreamConfig,
    canais: usize,
    mut fila: Consumer<f32>,
) -> Result<cpal::Stream, ErroSom>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let fluxo = dispositivo.build_output_stream(
        config,
        move |dados: &mut [T], _: &cpal::OutputCallbackInfo| {
            for quadro in dados.chunks_exact_mut(canais) {
                let amostra = T::from_sample(fila.pop().unwrap_or(0.0));
                quadro.fill(amostra);
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
                let _ = self.fila.push_partial_slice(amostras);
            }
            Err(e) => debug!("alto-falante: {e}"),
        }
    }
}
