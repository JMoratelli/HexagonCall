//! A ponte entre a placa de som (48 kHz ou 44,1 kHz, estereo, amostras f32) e
//! o telefone (8 kHz, mono, 16 bits). Sem dispositivo nenhum: os testes rodam
//! em qualquer lugar, inclusive no CI.
//!
//! O rubato faz a troca de taxa por FFT, com o filtro que tira o que passa de
//! 4 kHz antes de descer para 8 kHz (sem ele, o chiado agudo volta dobrado
//! para dentro da voz).

use crate::ErroSom;
use motor::midia::{AMOSTRAS, Quadro, TAXA};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

fn taxa_usize(taxa: u32) -> Result<usize, ErroSom> {
    usize::try_from(taxa).map_err(|_| ErroSom::Conversao(format!("taxa {taxa}")))
}

/// Da placa para o telefone: cada chamada devolve um quadro de 20 ms.
pub struct ParaTelefone {
    conversor: Fft<f32>,
    saida: Vec<f32>,
}

impl ParaTelefone {
    pub fn novo(taxa_da_placa: u32) -> Result<Self, ErroSom> {
        let conversor = Fft::<f32>::new(
            taxa_usize(taxa_da_placa)?,
            taxa_usize(TAXA)?,
            AMOSTRAS,
            1,
            FixedSync::Output,
        )
        .map_err(|e| ErroSom::Conversao(e.to_string()))?;
        Ok(Self {
            conversor,
            saida: vec![0.0; AMOSTRAS],
        })
    }

    /// Quantas amostras (mono, na taxa da placa) o proximo quadro pede.
    pub fn precisa(&self) -> usize {
        self.conversor.input_frames_next()
    }

    /// `entrada` tem de ter `precisa()` amostras.
    pub fn converte(&mut self, entrada: &[f32], quadro: &mut Quadro) -> Result<(), ErroSom> {
        let erro = |e: &dyn std::fmt::Display| ErroSom::Conversao(e.to_string());
        let ent = InterleavedSlice::new(entrada, 1, entrada.len()).map_err(|e| erro(&e))?;
        let mut sai =
            InterleavedSlice::new_mut(&mut self.saida, 1, AMOSTRAS).map_err(|e| erro(&e))?;
        self.conversor
            .process_into_buffer(&ent, &mut sai, None)
            .map_err(|e| erro(&e))?;
        for (q, s) in quadro.iter_mut().zip(&self.saida) {
            *q = para_i16(*s);
        }
        Ok(())
    }
}

/// Do telefone para a placa: cada quadro de 20 ms vira amostras na taxa dela.
pub struct DoTelefone {
    conversor: Fft<f32>,
    entrada: Vec<f32>,
    saida: Vec<f32>,
}

impl DoTelefone {
    pub fn novo(taxa_da_placa: u32) -> Result<Self, ErroSom> {
        let conversor = Fft::<f32>::new(
            taxa_usize(TAXA)?,
            taxa_usize(taxa_da_placa)?,
            AMOSTRAS,
            1,
            FixedSync::Input,
        )
        .map_err(|e| ErroSom::Conversao(e.to_string()))?;
        let saida = vec![0.0; conversor.output_frames_max()];
        Ok(Self {
            conversor,
            entrada: vec![0.0; AMOSTRAS],
            saida,
        })
    }

    pub fn converte(&mut self, quadro: &Quadro) -> Result<&[f32], ErroSom> {
        let erro = |e: &dyn std::fmt::Display| ErroSom::Conversao(e.to_string());
        for (e, q) in self.entrada.iter_mut().zip(quadro) {
            *e = f32::from(*q) / 32768.0;
        }
        let ent = InterleavedSlice::new(&self.entrada, 1, AMOSTRAS).map_err(|e| erro(&e))?;
        let cabe = self.saida.len();
        let mut sai = InterleavedSlice::new_mut(&mut self.saida, 1, cabe).map_err(|e| erro(&e))?;
        let (_, n) = self
            .conversor
            .process_into_buffer(&ent, &mut sai, None)
            .map_err(|e| erro(&e))?;
        Ok(self.saida.get(..n).unwrap_or_default())
    }
}

pub fn para_i16(amostra: f32) -> i16 {
    // Ja dentro do alcance do i16 pelo clamp: o `as` so tira a parte decimal.
    #[allow(clippy::cast_possible_truncation)]
    let v = (amostra * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
    v
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::f32::consts::TAU;

    fn tom(freq: f32, taxa: u32, n: usize) -> Vec<f32> {
        #[allow(clippy::cast_precision_loss)]
        (0..n)
            .map(|i| 0.5 * (TAU * freq * i as f32 / taxa as f32).sin())
            .collect()
    }

    /// Frequencia pelos cruzamentos de zero e o pico.
    fn mede(amostras: &[f32], taxa: u32) -> (f32, f32) {
        let cruzamentos = amostras
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let freq = cruzamentos as f32 / 2.0 / (amostras.len() as f32 / taxa as f32);
        let pico = amostras.iter().fold(0.0f32, |m, a| m.max(a.abs()));
        (freq, pico)
    }

    /// Do microfone a 48 kHz (ou 44,1 kHz) para 8 kHz: a voz (1 kHz) passa
    /// inteira, e o que nao cabe em 8 kHz (6 kHz) some em vez de virar 2 kHz.
    fn desce(taxa: u32) {
        let mut p = ParaTelefone::novo(taxa).unwrap();
        for (freq, deve_passar) in [(1000.0, true), (6000.0, false)] {
            let fonte = tom(freq, taxa, taxa as usize * 2);
            let mut lido = 0;
            let mut saida = Vec::new();
            let mut quadro = [0i16; AMOSTRAS];
            while lido + p.precisa() <= fonte.len() {
                let n = p.precisa();
                p.converte(&fonte[lido..lido + n], &mut quadro).unwrap();
                saida.extend(quadro.iter().map(|a| f32::from(*a) / 32768.0));
                lido += n;
            }
            // 2 s viram uns 100 quadros; o comeco tem o atraso do filtro.
            assert!(saida.len() >= 95 * AMOSTRAS, "so {} amostras", saida.len());
            let (f, pico) = mede(&saida[AMOSTRAS * 10..], TAXA);
            if deve_passar {
                assert!((950.0..1050.0).contains(&f), "{taxa} Hz: saiu {f} Hz");
                assert!((0.45..0.55).contains(&pico), "{taxa} Hz: pico {pico}");
            } else {
                assert!(pico < 0.02, "{taxa} Hz: 6 kHz vazou com pico {pico}");
            }
        }
    }

    #[test]
    fn desce_de_48_khz() {
        desce(48_000);
    }

    #[test]
    fn desce_de_44_1_khz() {
        desce(44_100);
    }

    #[test]
    fn sobe_para_48_khz_no_ritmo_certo() {
        let mut d = DoTelefone::novo(48_000).unwrap();
        let fonte = tom(1000.0, TAXA, AMOSTRAS * 100);
        let mut saida = Vec::new();
        for pedaco in fonte.as_chunks::<AMOSTRAS>().0 {
            let mut quadro = [0i16; AMOSTRAS];
            for (q, a) in quadro.iter_mut().zip(pedaco) {
                *q = para_i16(*a);
            }
            saida.extend_from_slice(d.converte(&quadro).unwrap());
        }
        // 2 s de telefone viram 2 s de placa: 96 mil amostras, fora o atraso.
        assert!((94_000..=96_000).contains(&saida.len()), "{}", saida.len());
        let (f, pico) = mede(&saida[48_000 / 5..], 48_000);
        assert!((950.0..1050.0).contains(&f), "saiu {f} Hz");
        assert!((0.45..0.55).contains(&pico), "pico {pico}");
    }

    #[test]
    fn para_i16_satura_sem_dar_a_volta() {
        assert_eq!(para_i16(2.0), i16::MAX);
        assert_eq!(para_i16(-2.0), i16::MIN);
        assert_eq!(para_i16(0.0), 0);
    }
}
