//! "Nao te ouco": o microfone so manda silencio. Primeiro, esta mudo no
//! sistema (ou com o volume em 0%)? Isso se resolve com um clique. Se nao
//! esta, alguma outra entrada esta captando som? Ai vale sugerir trocar
//! (pedido de 26/11: so quando a principal nao capta).
//!
//! Mudo e volume vem do pactl (Linux); no Windows, por enquanto, `None`.

use crate::{ErroSom, Microfone, microfones, pactl};
use motor::midia::{AMOSTRAS, Fonte, Quadro};
use std::time::{Duration, Instant};

/// Abaixo disso nao conta como "captando": o ruido de uma entrada sem nada
/// ligado fica perto de -90 dBFS; um microfone de verdade numa sala, de -60
/// para cima.
const CAPTANDO_DB: f32 = -65.0;

/// Uma entrada que esta captando som, com o nivel medido.
#[derive(Debug, Clone, PartialEq)]
pub struct Captando {
    pub id: String,
    pub nome: String,
    pub nivel_db: f32,
}

/// A entrada esta muda no sistema ou com o volume em 0%? `None`: nao da para
/// saber (Windows, ALSA cru, pactl ausente).
pub fn muda_no_sistema(id: Option<&str>) -> Option<bool> {
    let fonte = pactl::nome(id, "@DEFAULT_SOURCE@")?;
    let mudo = pactl::roda(&["get-source-mute", &fonte]).map(|m| pactl::esta_mudo(&m))?;
    let zero = pactl::roda(&["get-source-volume", &fonte])
        .and_then(|v| pactl::porcentagem(&v))
        .is_some_and(|p| p == "0%");
    Some(mudo || zero)
}

/// Tira a entrada do mudo e, se o volume estiver em 0%, sobe para 100%.
pub fn tira_do_mudo(id: Option<&str>) -> Result<(), ErroSom> {
    let fonte = pactl::nome(id, "@DEFAULT_SOURCE@")
        .ok_or_else(|| ErroSom::NaoAchei("entrada sem controle de mudo".into()))?;
    pactl::roda(&["set-source-mute", &fonte, "0"])
        .ok_or_else(|| ErroSom::NaoAchei("o sistema nao deixou tirar do mudo".into()))?;
    let zero = pactl::roda(&["get-source-volume", &fonte])
        .and_then(|v| pactl::porcentagem(&v))
        .is_some_and(|p| p == "0%");
    if zero {
        let _ = pactl::roda(&["set-source-volume", &fonte, "100%"]);
    }
    Ok(())
}

/// O nivel de um quadro em dBFS (silencio absoluto da -120).
pub fn nivel_db(quadro: &Quadro) -> f32 {
    let soma: f64 = quadro.iter().map(|a| f64::from(*a).powi(2)).sum();
    #[allow(clippy::cast_precision_loss)]
    let rms = (soma / AMOSTRAS as f64).sqrt() / 32768.0;
    #[allow(clippy::cast_possible_truncation)]
    let db = (20.0 * rms.max(1e-6).log10()) as f32;
    db
}

/// Escuta as outras entradas ao mesmo tempo por `duracao` (sem mexer na que
/// esta na ligacao) e devolve as que captam som, da mais alta para a mais
/// baixa. Bloqueia: chamar fora da tarefa do telefone.
pub fn outras_captando(atual: Option<&str>, duracao: Duration) -> Vec<Captando> {
    let Ok(todas) = microfones() else {
        return Vec::new();
    };
    let mut abertas: Vec<(String, String, Microfone, f32)> = todas
        .into_iter()
        // A que esta na ligacao fica de fora (o padrao, quando nao ha escolha).
        .filter(|d| match atual {
            Some(a) => d.id != a,
            None => !d.padrao,
        })
        .filter_map(|d| {
            let m = Microfone::abre(Some(&d.id)).ok()?;
            Some((d.id, d.nome, m, -120.0f32))
        })
        .collect();
    let mut quadro = [0i16; AMOSTRAS];
    let fim = Instant::now() + duracao;
    while Instant::now() < fim {
        std::thread::sleep(Duration::from_millis(20));
        for (_, _, m, maior) in &mut abertas {
            m.le(&mut quadro);
            *maior = maior.max(nivel_db(&quadro));
        }
    }
    let mut captando: Vec<Captando> = abertas
        .into_iter()
        .filter(|(_, _, _, db)| *db > CAPTANDO_DB)
        .map(|(id, nome, _, nivel_db)| Captando { id, nome, nivel_db })
        .collect();
    captando.sort_by(|a, b| b.nivel_db.total_cmp(&a.nivel_db));
    captando
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nivel_de_silencio_e_de_cheio() {
        assert!(nivel_db(&[0; AMOSTRAS]) <= -119.0);
        let cheio = [i16::MAX; AMOSTRAS];
        assert!(nivel_db(&cheio) > -0.1);
        let meio = [8192i16; AMOSTRAS];
        assert!((nivel_db(&meio) + 12.0).abs() < 0.2, "{}", nivel_db(&meio));
    }
}
