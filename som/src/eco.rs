//! A limpeza da voz numa ligacao (pesquisa de 27/11, docs/estado.md):
//! - o eco: o AEC3 do WebRTC portado para Rust (sonora), no microfone, com a
//!   referencia do que o alto-falante tocou de verdade. So quem toca tem a
//!   referencia: tirar o eco e servico do envio, e quem ganha e o outro lado;
//! - o ruido do microfone: a supressao do WebRTC (moderada) e o filtro de
//!   graves, no mesmo processador;
//! - a voz mais nitida (opcional): o RNNoise (nnnoiseless) no que chega do
//!   outro lado, a 48 kHz.
//!
//! A referencia e o microfone perdem amostras juntos: um corte so de um lado
//! (a folga do microfone jogando fora atraso) faz o eco voltar por segundos.

use rtrb::{Consumer, Producer, RingBuffer};
use sonora::config::{EchoCanceller, HighPassFilter, NoiseSuppression, NoiseSuppressionLevel};
use sonora::{AudioProcessing, Config, StreamConfig};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tracing::{info, warn};

/// O que limpar na voz de uma ligacao (Ajustes > Audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limpeza {
    /// Tirar o eco: o som do alto-falante voltando pelo microfone.
    pub eco: bool,
    /// Tirar o ruido constante do microfone (ventilador, ar-condicionado).
    pub ruido: bool,
    /// Limpar o que chega do outro lado.
    pub nitida: bool,
}

/// As tres ligadas (padrao do app desde 27/11).
impl Default for Limpeza {
    fn default() -> Self {
        Self {
            eco: true,
            ruido: true,
            nitida: true,
        }
    }
}

impl Limpeza {
    /// Nada: o som como a placa da (o teste do microfone, o toque).
    pub const NADA: Self = Self {
        eco: false,
        ruido: false,
        nitida: false,
    };
}

/// A referencia mais adiantada que isto (o alto-falante andou mais que o
/// microfone, ou o microfone ainda enchia): o excesso sai e ela volta ao
/// alvo. Com ela a frente do eco, o AEC3 nao acha o atraso.
const REFERENCIA_MAXIMA_MS: u32 = 60;
const REFERENCIA_ALVO_MS: u32 = 20;

/// A ponta do microfone na referencia e a taxa do alto-falante.
type Ponta = (Consumer<f32>, u32);

/// O que o alto-falante tocou, para o microfone tirar o eco. Uma por ligacao;
/// um microfone trocado no meio dela pega a ponta quando o velho a devolve.
#[derive(Clone, Default)]
pub struct Referencia(Arc<Mutex<Option<Ponta>>>);

impl Referencia {
    pub fn nova() -> Self {
        Self::default()
    }

    /// O alto-falante abriu na `taxa`: a ponta dele, com 1 s de fila.
    pub(crate) fn para_o_alto_falante(&self, taxa: u32) -> Producer<f32> {
        let (produtor, consumidor) = RingBuffer::new(usize::try_from(taxa).unwrap_or(48_000));
        *self.ponta() = Some((consumidor, taxa));
        produtor
    }

    fn pega(&self) -> Option<Ponta> {
        self.ponta().take()
    }

    fn devolve(&self, ponta: Ponta) {
        let mut guardada = self.ponta();
        if guardada.is_none() {
            *guardada = Some(ponta);
        }
    }

    fn ponta(&self) -> MutexGuard<'_, Option<Ponta>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// O processador do microfone: eco e ruido, em blocos de 10 ms na taxa da
/// placa. Atrasa um bloco.
pub(crate) struct Limpador {
    limpeza: Limpeza,
    referencia: Option<Referencia>,
    fila: Option<Ponta>,
    apm: Option<AudioProcessing>,
    taxa: u32,
    bloco: usize,
    pendente: Vec<f32>,
    pronto: VecDeque<f32>,
    entrada: Vec<f32>,
    saida: Vec<f32>,
    ref_bloco: Vec<f32>,
    lixo: Vec<f32>,
    /// Deu erro ou panico no processador: o microfone segue sem limpeza.
    desligado: bool,
}

impl Limpador {
    /// None: nada a limpar (eco e ruido desligados).
    pub(crate) fn novo(
        taxa: u32,
        limpeza: Limpeza,
        referencia: Option<Referencia>,
    ) -> Option<Self> {
        if !limpeza.eco && !limpeza.ruido {
            return None;
        }
        let bloco = usize::try_from(taxa / 100).ok().filter(|b| *b > 0)?;
        Some(Self {
            limpeza,
            referencia: referencia.filter(|_| limpeza.eco),
            fila: None,
            apm: None,
            taxa,
            bloco,
            pendente: Vec::with_capacity(bloco * 4),
            pronto: std::iter::repeat_n(0.0, bloco).collect(),
            entrada: vec![0.0; bloco],
            saida: vec![0.0; bloco],
            ref_bloco: Vec::new(),
            lixo: Vec::new(),
            desligado: false,
        })
    }

    /// Pega a ponta da referencia quando ela aparece (o alto-falante abre
    /// depois, ou o microfone de antes devolveu) e monta o processador.
    fn conecta(&mut self) {
        if self.fila.is_none()
            && let Some(ponta) = self.referencia.as_ref().and_then(Referencia::pega)
        {
            let taxa_ref = ponta.1;
            self.ref_bloco = vec![0.0; usize::try_from(taxa_ref / 100).unwrap_or(480)];
            self.lixo = self.ref_bloco.clone();
            self.fila = Some(ponta);
            self.apm = None;
            info!(taxa = taxa_ref, "eco: com a referencia do alto-falante");
        }
        if self.apm.is_none() {
            let taxa_ref = self.fila.as_ref().map(|f| f.1);
            let config = Config {
                echo_canceller: taxa_ref.map(|_| EchoCanceller::default()),
                high_pass_filter: Some(HighPassFilter::default()),
                noise_suppression: self.limpeza.ruido.then(|| NoiseSuppression {
                    level: NoiseSuppressionLevel::Moderate,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let (taxa, taxa_ref) = (self.taxa, taxa_ref.unwrap_or(self.taxa));
            let montado = catch_unwind(|| {
                AudioProcessing::builder()
                    .config(config)
                    .capture_config(StreamConfig::new(taxa, 1))
                    .render_config(StreamConfig::new(taxa_ref, 1))
                    .build()
            });
            match montado {
                Ok(apm) => self.apm = Some(apm),
                Err(_) => self.desliga("nao montou"),
            }
        }
    }

    fn desliga(&mut self, motivo: &str) {
        warn!("a limpeza da voz desligou nesta ligacao: {motivo}");
        self.desligado = true;
    }

    /// Limpa as amostras do microfone, no lugar.
    pub(crate) fn processa(&mut self, amostras: &mut [f32]) {
        if self.desligado {
            return;
        }
        self.conecta();
        self.pendente.extend_from_slice(amostras);
        while !self.desligado && self.pendente.len() >= self.bloco {
            for (e, p) in self
                .entrada
                .iter_mut()
                .zip(self.pendente.drain(..self.bloco))
            {
                *e = p;
            }
            if let Err(e) = self.um_bloco() {
                self.desliga(&e);
                break;
            }
            self.pronto.extend(self.saida.iter().copied());
        }
        if self.desligado {
            return;
        }
        for a in amostras.iter_mut() {
            *a = self.pronto.pop_front().unwrap_or(0.0);
        }
    }

    fn um_bloco(&mut self) -> Result<(), String> {
        let Some(apm) = self.apm.as_mut() else {
            return Err("sem processador".into());
        };
        if let Some((fila, taxa_ref)) = self.fila.as_mut() {
            let por_ms = |ms: u32| usize::try_from(*taxa_ref * ms / 1000).unwrap_or(0);
            let tem = fila.slots();
            if tem > por_ms(REFERENCIA_MAXIMA_MS) + self.ref_bloco.len() {
                descarta(
                    fila,
                    tem - por_ms(REFERENCIA_ALVO_MS) - self.ref_bloco.len(),
                );
            }
            for r in &mut self.ref_bloco {
                // Faltou referencia: o alto-falante nao tocou nada ai.
                *r = fila.pop().unwrap_or(0.0);
            }
        }
        let (entrada, saida) = (&self.entrada, &mut self.saida);
        let (ref_bloco, lixo, com_eco) = (&self.ref_bloco, &mut self.lixo, self.fila.is_some());
        let feito = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
            if com_eco {
                apm.process_render_f32(&[ref_bloco], &mut [lixo])
                    .map_err(|e| format!("referencia: {e:?}"))?;
                // O AEC3 acha o atraso sozinho (ate uns 600 ms).
                let _ = apm.set_stream_delay_ms(0);
            }
            apm.process_capture_f32(&[entrada], &mut [saida])
                .map_err(|e| format!("microfone: {e:?}"))
        }));
        feito.unwrap_or_else(|_| Err("panico no processador".into()))
    }

    /// A folga do microfone jogou fora `n` amostras: a referencia perde o
    /// mesmo tempo, no mesmo ponto.
    pub(crate) fn descarta(&mut self, n: usize) {
        if let Some((fila, taxa_ref)) = self.fila.as_mut() {
            let n_ref =
                u64::try_from(n).unwrap_or(0) * u64::from(*taxa_ref) / u64::from(self.taxa.max(1));
            descarta(fila, usize::try_from(n_ref).unwrap_or(usize::MAX));
        }
    }
}

impl Drop for Limpador {
    fn drop(&mut self) {
        if let (Some(r), Some(f)) = (self.referencia.as_ref(), self.fila.take()) {
            r.devolve(f);
        }
    }
}

fn descarta(fila: &mut Consumer<f32>, n: usize) {
    if let Ok(velho) = fila.read_chunk(n.min(fila.slots())) {
        velho.commit_all();
    }
}

/// A voz mais nitida: o RNNoise no que chega, a 48 kHz, em quadros de 10 ms.
/// Atrasa um quadro.
pub(crate) struct Clareza {
    estado: Box<nnnoiseless::DenoiseState<'static>>,
    pendente: Vec<f32>,
    pronto: VecDeque<f32>,
    entrada: Vec<f32>,
    quadro: Vec<f32>,
    saida: Vec<f32>,
}

impl Clareza {
    const QUADRO: usize = nnnoiseless::DenoiseState::FRAME_SIZE;

    /// So com a saida a 48 kHz, a taxa do RNNoise.
    pub(crate) fn nova(taxa: u32) -> Option<Self> {
        if taxa != 48_000 {
            warn!(
                taxa,
                "a voz mais nitida so funciona com a saida de som a 48 kHz"
            );
            return None;
        }
        Some(Self {
            estado: nnnoiseless::DenoiseState::new(),
            pendente: Vec::with_capacity(Self::QUADRO * 4),
            pronto: std::iter::repeat_n(0.0, Self::QUADRO).collect(),
            entrada: vec![0.0; Self::QUADRO],
            quadro: vec![0.0; Self::QUADRO],
            saida: Vec::new(),
        })
    }

    /// Limpa o que vai tocar; devolve o mesmo tanto de amostras.
    pub(crate) fn processa(&mut self, amostras: &[f32]) -> &[f32] {
        self.pendente.extend_from_slice(amostras);
        while self.pendente.len() >= Self::QUADRO {
            // O RNNoise trabalha na escala do i16.
            for (e, p) in self
                .entrada
                .iter_mut()
                .zip(self.pendente.drain(..Self::QUADRO))
            {
                *e = p * 32_768.0;
            }
            self.estado.process_frame(&mut self.quadro, &self.entrada);
            self.pronto.extend(self.quadro.iter().map(|q| q / 32_768.0));
        }
        self.saida.clear();
        for _ in 0..amostras.len() {
            self.saida.push(self.pronto.pop_front().unwrap_or(0.0));
        }
        &self.saida
    }
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
mod testes {
    use super::*;

    /// Fala de mentira: ruido branco com o envelope de silabas (4 por
    /// segundo), sempre o mesmo.
    fn fala(segundos: usize, taxa: usize, semente: u64) -> Vec<f32> {
        let mut x = semente;
        (0..segundos * taxa)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let ruido = (x % 20_001) as f32 / 10_000.0 - 1.0;
                let t = i as f32 / taxa as f32;
                let envelope = (std::f32::consts::TAU * 2.0 * t).sin().abs();
                ruido * envelope * 0.3
            })
            .collect()
    }

    /// A sala: o eco chega 60 ms depois, a 40%, com duas reflexoes.
    fn sala(x: &[f32], taxa: usize) -> Vec<f32> {
        let caminho = [(60, 0.4), (66, 0.15), (75, -0.08)];
        let mut y = vec![0.0; x.len()];
        for (ms, ganho) in caminho {
            let d = taxa * ms / 1000;
            for i in d..x.len() {
                y[i] += x[i - d] * ganho;
            }
        }
        y
    }

    fn energia(v: &[f32]) -> f64 {
        v.iter().map(|a| f64::from(*a) * f64::from(*a)).sum::<f64>() / v.len() as f64
    }

    /// Roda uma ligacao de mentira: a cada 20 ms o alto-falante toca o
    /// outro lado (a referencia) e o microfone entrega o que ouviu.
    fn liga(outro: &[f32], microfone: &[f32], taxa: usize, limpeza: Limpeza) -> Vec<f32> {
        let referencia = Referencia::nova();
        let mut alto_falante = referencia.para_o_alto_falante(taxa as u32);
        let mut l = Limpador::novo(taxa as u32, limpeza, Some(referencia)).unwrap();
        let passo = taxa / 50;
        let mut saida = Vec::with_capacity(microfone.len());
        for (tocou, ouviu) in outro.chunks(passo).zip(microfone.chunks(passo)) {
            for a in tocou {
                alto_falante.push(*a).unwrap();
            }
            let mut quadro = ouviu.to_vec();
            l.processa(&mut quadro);
            saida.extend_from_slice(&quadro);
        }
        saida
    }

    #[test]
    fn tira_o_eco_do_alto_falante() {
        let taxa = 48_000;
        let outro = fala(5, taxa, 7);
        let microfone = sala(&outro, taxa);
        let so_eco = Limpeza {
            eco: true,
            ruido: false,
            nitida: false,
        };
        let saida = liga(&outro, &microfone, taxa, so_eco);
        let depois = 2 * taxa;
        let tirado = 10.0 * (energia(&microfone[depois..]) / energia(&saida[depois..])).log10();
        assert!(tirado > 20.0, "tirou so {tirado:.1} dB do eco");
    }

    /// Voz de mentira mais parecida com fala: silabas de 180 ms com pausas de
    /// 120 ms, o pitch mudando a cada uma, e os harmonicos. Um tom continuo o
    /// supressor de ruido toma por ruido (com razao).
    fn voz(segundos: usize, taxa: usize) -> Vec<f32> {
        let mut v = Vec::new();
        let mut k = 0u32;
        while v.len() < segundos * taxa {
            let f0 = 110.0 + ((k * 37) % 11) as f32 * 10.0;
            let n = taxa * 180 / 1000;
            for i in 0..n {
                let t = i as f32 / taxa as f32;
                let envelope = (std::f32::consts::PI * i as f32 / n as f32).sin();
                let tom: f32 = (1..16)
                    .map(|h| {
                        let f = f0 * (1.0 + 0.1 * t) * h as f32;
                        (std::f32::consts::TAU * f * t).sin() / h as f32
                    })
                    .sum();
                v.push(tom * envelope * 0.15);
            }
            v.extend(std::iter::repeat_n(0.0, taxa * 120 / 1000));
            k += 1;
        }
        v.truncate(segundos * taxa);
        v
    }

    fn mudou(antes: &[f32], depois: &[f32], taxa: usize) -> f64 {
        // Um bloco (10 ms) de atraso; compara do segundo 1 em diante.
        let (ini, atraso) = (taxa, taxa / 100);
        10.0 * (energia(&antes[ini..antes.len() - atraso]) / energia(&depois[ini + atraso..]))
            .log10()
    }

    #[test]
    fn sem_o_outro_lado_a_voz_passa_pelo_eco() {
        let taxa = 48_000;
        let silencio = vec![0.0; 4 * taxa];
        let v = voz(4, taxa);
        let so_eco = Limpeza {
            eco: true,
            ruido: false,
            nitida: false,
        };
        let saida = liga(&silencio, &v, taxa, so_eco);
        let perdido = mudou(&v, &saida, taxa);
        assert!(perdido.abs() < 1.5, "a voz mudou {perdido:.1} dB");
    }

    #[test]
    fn a_limpeza_toda_deixa_a_voz_e_tira_o_ruido_constante() {
        let taxa = 48_000;
        let silencio = vec![0.0; 4 * taxa];
        let v = voz(4, taxa);
        let saida = liga(&silencio, &v, taxa, Limpeza::default());
        let perdido = mudou(&v, &saida, taxa);
        assert!(perdido.abs() < 3.0, "a voz mudou {perdido:.1} dB");
        // Um ventilador: ruido constante e baixo, sem voz.
        let ruido: Vec<f32> = fala(4, taxa, 5)
            .iter()
            .enumerate()
            .map(|(i, a)| if i % 2 == 0 { a * 0.05 } else { -a * 0.05 })
            .collect();
        let saida = liga(&silencio, &ruido, taxa, Limpeza::default());
        let caiu = mudou(&ruido, &saida, taxa);
        assert!(caiu > 6.0, "o ruido caiu so {caiu:.1} dB");
    }

    #[test]
    fn o_microfone_novo_pega_a_referencia_quando_o_velho_solta() {
        let referencia = Referencia::nova();
        let _alto_falante = referencia.para_o_alto_falante(48_000);
        let mut velho =
            Limpador::novo(48_000, Limpeza::default(), Some(referencia.clone())).unwrap();
        let mut novo =
            Limpador::novo(48_000, Limpeza::default(), Some(referencia.clone())).unwrap();
        let mut q = vec![0.0; 960];
        velho.processa(&mut q);
        novo.processa(&mut q);
        assert!(velho.fila.is_some() && novo.fila.is_none());
        drop(velho);
        novo.processa(&mut q);
        assert!(novo.fila.is_some(), "pegou a que o velho devolveu");
    }

    #[test]
    fn a_clareza_devolve_o_mesmo_tanto_e_tira_ruido() {
        let taxa = 48_000;
        assert!(Clareza::nova(44_100).is_none());
        let mut c = Clareza::nova(taxa as u32).unwrap();
        let ruido = fala(3, taxa, 3).iter().map(|a| a * 0.1).collect::<Vec<_>>();
        let mut saida = Vec::new();
        for pedaco in ruido.chunks(960) {
            let limpo = c.processa(pedaco);
            assert_eq!(limpo.len(), pedaco.len());
            saida.extend_from_slice(limpo);
        }
        let caiu = 10.0 * (energia(&ruido[taxa..]) / energia(&saida[taxa..])).log10();
        assert!(caiu > 3.0, "o ruido caiu so {caiu:.1} dB");
    }
}
