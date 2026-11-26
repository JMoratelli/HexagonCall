//! O toque de quem liga, tocado pelo app (e nao pela tela): assim ele sai no
//! alto-falante escolhido ou em todos ao mesmo tempo, e no volume que se quer.
//! Os toques sao sintetizados aqui, sem arquivo de som: notas com subida curta
//! e queda suave, repetidas a cada `periodo`.
//!
//! Duas opcoes separadas (pedido de 26/11): "tocar em todos os
//! alto-falantes" e "volume maximo". O maximo, no Linux, sobe a 100% e tira o
//! mudo das saidas que tocam enquanto toca, e devolve o volume de antes
//! quando para (pactl, do PipeWire ou do PulseAudio). No Windows, por
//! enquanto, so o volume cheio do proprio toque.

use crate::{ErroSom, configuracao_de_saida, saida, todas_as_saidas};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use std::f32::consts::TAU;
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct Nota {
    pub freqs: Vec<f32>,
    pub inicio: f32,
    pub dur: f32,
    pub vol: f32,
}

#[derive(Debug, Clone)]
pub struct Toque {
    pub id: &'static str,
    pub nome: &'static str,
    pub periodo: f32,
    pub notas: Vec<Nota>,
}

fn nota(freqs: &[f32], inicio: f32, dur: f32, vol: f32) -> Nota {
    Nota {
        freqs: freqs.to_vec(),
        inicio,
        dur,
        vol,
    }
}

/// Os toques de quem liga. O primeiro e o padrao: o aprovado em 26/11.
pub fn toques() -> Vec<Toque> {
    // O "telefone antigo": a campainha batendo entre duas notas, 1 s sim, 1 s nao, duas vezes.
    let mut campainha = Vec::new();
    for rajada in [0.0f32, 2.0] {
        for i in 0..20u8 {
            let f = if i % 2 == 0 { 1000.0 } else { 1250.0 };
            campainha.push(nota(
                &[f, f * 2.01],
                rajada + f32::from(i) * 0.05,
                0.06,
                0.8,
            ));
        }
    }
    vec![
        Toque {
            id: "colmeia",
            nome: "Colmeia",
            periodo: 3.0,
            notas: vec![
                nota(&[660.0], 0.0, 0.24, 1.0),
                nota(&[880.0], 0.28, 0.24, 1.0),
            ],
        },
        Toque {
            id: "marimba",
            nome: "Marimba",
            periodo: 2.6,
            notas: vec![
                nota(&[523.25, 1046.5], 0.0, 0.32, 0.9),
                nota(&[659.25, 1318.5], 0.14, 0.32, 0.9),
                nota(&[783.99, 1567.98], 0.28, 0.32, 0.9),
                nota(&[1046.5, 2093.0], 0.42, 0.45, 0.8),
            ],
        },
        Toque {
            id: "suave",
            nome: "Suave",
            periodo: 4.0,
            notas: vec![
                nota(&[440.0, 554.37, 659.25], 0.0, 1.2, 0.9),
                nota(&[493.88, 622.25, 739.99], 1.3, 1.2, 0.9),
            ],
        },
        Toque {
            id: "digital",
            nome: "Digital",
            periodo: 2.0,
            notas: vec![
                nota(&[1200.0], 0.0, 0.08, 0.7),
                nota(&[1200.0], 0.12, 0.08, 0.7),
                nota(&[1500.0], 0.24, 0.08, 0.7),
                nota(&[1500.0], 0.36, 0.08, 0.7),
            ],
        },
        Toque {
            id: "antigo",
            nome: "Campainha",
            periodo: 6.0,
            notas: campainha,
        },
    ]
}

pub fn toque(id: &str) -> Toque {
    let todos = toques();
    let padrao = todos.first().cloned();
    todos
        .into_iter()
        .find(|t| t.id == id)
        .or(padrao)
        .unwrap_or(Toque {
            id: "silencio",
            nome: "Silencio",
            periodo: 1.0,
            notas: Vec::new(),
        })
}

/// O valor do toque no instante `t` (segundos desde que comecou), de -1 a 1.
pub fn amostra(toque: &Toque, t: f32) -> f32 {
    let t = t.rem_euclid(toque.periodo.max(0.01));
    let mut soma = 0.0;
    for n in &toque.notas {
        let dt = t - n.inicio;
        if dt < 0.0 || dt > n.dur {
            continue;
        }
        // Sobe em 15 ms e cai em curva ate zero no fim da nota: sem estalo no
        // corte, e bipe curto continua audivel.
        let queda = 1.0 - dt / n.dur;
        let envelope = (dt / 0.015).min(1.0) * queda * queda;
        let peso = n.vol / n.freqs.len().max(1) as f32;
        for f in &n.freqs {
            soma += peso * envelope * (TAU * f * dt).sin();
        }
    }
    soma.clamp(-1.0, 1.0)
}

/// Onde tocar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Onde {
    /// Um alto-falante (o id da lista; None e o padrao do sistema).
    Um(Option<String>),
    /// Todas as saidas.
    Todas,
}

/// Toca enquanto existir. Ao sair (atendeu, desligaram), devolve o volume.
pub struct Tocador {
    _fluxos: Vec<cpal::Stream>,
    volumes: Vec<volume::Antes>,
}

impl Tocador {
    /// `intensidade` de 0 a 1 (o volume do toque nas preferencias); com
    /// `maximo`, o toque vai cheio e as saidas sobem a 100% enquanto toca.
    pub fn toca(id: &str, onde: &Onde, intensidade: f32, maximo: bool) -> Result<Self, ErroSom> {
        let toque = Arc::new(toque(id));
        let (dispositivos, saidas) = match onde {
            Onde::Um(d) => (
                vec![saida(d.as_deref())?],
                volume::nome_da_saida(d.as_deref()),
            ),
            Onde::Todas => (todas_as_saidas(), volume::todas()),
        };
        let (amplitude, volumes) = if maximo {
            (0.9, volume::sobe(&saidas))
        } else {
            (intensidade.clamp(0.0, 1.0), Vec::new())
        };
        let mut fluxos = Vec::new();
        for d in &dispositivos {
            match abre(d, Arc::clone(&toque), amplitude) {
                Ok(f) => fluxos.push(f),
                Err(e) => debug!("toque: uma saida nao abriu: {e}"),
            }
        }
        if fluxos.is_empty() {
            volume::devolve(&volumes);
            return Err(ErroSom::SemDispositivo("alto-falante"));
        }
        info!(toque = id, saidas = fluxos.len(), "tocando");
        Ok(Self {
            _fluxos: fluxos,
            volumes,
        })
    }
}

impl Drop for Tocador {
    fn drop(&mut self) {
        volume::devolve(&self.volumes);
    }
}

fn abre(d: &cpal::Device, toque: Arc<Toque>, amplitude: f32) -> Result<cpal::Stream, ErroSom> {
    let config = configuracao_de_saida(d)?;
    let formato = d.default_output_config()?.sample_format();
    let fluxo = match formato {
        SampleFormat::F32 => fluxo::<f32>(d, config, toque, amplitude)?,
        SampleFormat::I16 => fluxo::<i16>(d, config, toque, amplitude)?,
        SampleFormat::I32 => fluxo::<i32>(d, config, toque, amplitude)?,
        SampleFormat::U16 => fluxo::<u16>(d, config, toque, amplitude)?,
        outro => return Err(ErroSom::Formato(outro.to_string())),
    };
    fluxo.play()?;
    Ok(fluxo)
}

fn fluxo<T>(
    d: &cpal::Device,
    config: StreamConfig,
    toque: Arc<Toque>,
    amplitude: f32,
) -> Result<cpal::Stream, ErroSom>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let canais = usize::from(config.channels.max(1));
    #[allow(clippy::cast_precision_loss)]
    let passo = 1.0 / config.sample_rate.max(1) as f32;
    let mut t = 0.0f32;
    Ok(d.build_output_stream(
        config,
        move |dados: &mut [T], _: &cpal::OutputCallbackInfo| {
            for quadro in dados.chunks_exact_mut(canais) {
                let v = T::from_sample(amplitude * amostra(&toque, t));
                quadro.fill(v);
                // Volta no fim de cada periodo: o tempo nao cresce sem limite.
                t = (t + passo).rem_euclid(toque.periodo.max(0.01));
            }
        },
        |e| warn!("toque: {e}"),
        None,
    )?)
}

/// O volume de cada saida no "maximo": sobe, guarda o de antes e devolve.
mod volume {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Antes {
        pub saida: String,
        pub volume: String,
        pub mudo: bool,
    }

    use crate::pactl::{esta_mudo, nome, porcentagem, roda as pactl};

    /// O nome da saida para o pactl: o id do PulseAudio sem o prefixo, ou a
    /// saida padrao. Um id do ALSA cru nao tem como mexer no volume.
    pub fn nome_da_saida(id: Option<&str>) -> Vec<String> {
        nome(id, "@DEFAULT_SINK@").into_iter().collect()
    }

    pub fn todas() -> Vec<String> {
        pactl(&["list", "short", "sinks"])
            .map(|lista| {
                lista
                    .lines()
                    .filter_map(|l| l.split('\t').nth(1))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn sobe(saidas: &[String]) -> Vec<Antes> {
        let mut antes = Vec::new();
        for nome in saidas.iter().map(String::as_str) {
            let volume = pactl(&["get-sink-volume", nome]).and_then(|v| porcentagem(&v));
            let mudo = pactl(&["get-sink-mute", nome]).is_some_and(|m| esta_mudo(&m));
            if let Some(volume) = volume {
                antes.push(Antes {
                    saida: nome.to_string(),
                    volume,
                    mudo,
                });
                let _ = pactl(&["set-sink-volume", nome, "100%"]);
                let _ = pactl(&["set-sink-mute", nome, "0"]);
            }
        }
        antes
    }

    pub fn devolve(antes: &[Antes]) {
        for a in antes {
            let _ = pactl(&["set-sink-volume", &a.saida, &a.volume]);
            let _ = pactl(&["set-sink-mute", &a.saida, if a.mudo { "1" } else { "0" }]);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todo_toque_fica_entre_menos_um_e_um_e_se_repete() {
        for t in toques() {
            let mut maior = 0.0f32;
            for i in 0..48_000 {
                #[allow(clippy::cast_precision_loss)]
                let s = i as f32 / 8000.0;
                let v = amostra(&t, s);
                assert!((-1.0..=1.0).contains(&v), "{}: {v}", t.id);
                maior = maior.max(v.abs());
                let depois = amostra(&t, s + t.periodo);
                // f32: somar o periodo erra uns us, que nas notas agudas viram centesimos.
                assert!((v - depois).abs() < 0.05, "{}: nao se repete em {s}", t.id);
            }
            assert!(maior > 0.2, "{}: quase mudo ({maior})", t.id);
        }
    }

    #[test]
    fn colmeia_e_o_padrao_e_tem_pausa() {
        assert_eq!(toques()[0].id, "colmeia");
        assert_eq!(toque("nao-existe").id, "colmeia");
        let t = toque("colmeia");
        // Entre as notas e o proximo periodo, silencio.
        assert_eq!(amostra(&t, 1.5), 0.0);
        assert!(amostra(&t, 0.05).abs() > 0.0);
    }

    #[test]
    fn nome_da_saida_para_o_pactl() {
        assert_eq!(
            volume::nome_da_saida(None),
            vec!["@DEFAULT_SINK@".to_string()]
        );
        assert_eq!(
            volume::nome_da_saida(Some("pulseaudio:alsa_output.usb-x.analog-stereo")),
            vec!["alsa_output.usb-x.analog-stereo".to_string()]
        );
        assert!(volume::nome_da_saida(Some("alsa:sysdefault:CARD=X")).is_empty());
    }
}
