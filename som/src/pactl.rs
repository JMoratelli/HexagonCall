//! O pactl (do PipeWire ou do PulseAudio), para o que o cpal nao faz: ver e
//! mexer no mudo e no volume de uma saida ou entrada do sistema. So no Linux;
//! fora dele, `roda` devolve None e quem chama segue sem essa informacao.

/// Roda o pactl e devolve a saida, se deu certo.
#[cfg(target_os = "linux")]
pub fn roda(args: &[&str]) -> Option<String> {
    let saida = std::process::Command::new("pactl")
        .args(args)
        .output()
        .ok()?;
    saida
        .status
        .success()
        .then(|| String::from_utf8_lossy(&saida.stdout).into_owned())
}

#[cfg(not(target_os = "linux"))]
pub fn roda(_args: &[&str]) -> Option<String> {
    None
}

/// O primeiro porcentual de `pactl get-*-volume` ("... /  80% / ...").
pub fn porcentagem(texto: &str) -> Option<String> {
    texto
        .split('/')
        .map(str::trim)
        .find(|p| p.ends_with('%') && p.trim_end_matches('%').parse::<u32>().is_ok())
        .map(str::to_string)
}

/// `pactl get-*-mute` diz "Mute: yes" (ou "sim", se vier traduzido).
pub fn esta_mudo(texto: &str) -> bool {
    texto.contains("yes") || texto.contains("sim")
}

/// O nome no pactl: o id do PulseAudio sem o prefixo, ou o padrao do sistema
/// (`padrao`: @DEFAULT_SINK@ ou @DEFAULT_SOURCE@). Um id do ALSA cru nao
/// corresponde a nada no pactl.
pub fn nome(id: Option<&str>, padrao: &str) -> Option<String> {
    match id {
        None => Some(padrao.to_string()),
        Some(id) => id.strip_prefix("pulseaudio:").map(str::to_string),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_que_o_pactl_responde() {
        let volume = "Volume: front-left: 52428 /  80% / -5,81 dB,   front-right: 52428 /  80% / -5,81 dB\n        balance 0,00\n";
        assert_eq!(porcentagem(volume).as_deref(), Some("80%"));
        assert_eq!(porcentagem("lixo"), None);
        assert!(esta_mudo("Mute: yes\n"));
        assert!(esta_mudo("Mudo: sim\n"));
        assert!(!esta_mudo("Mute: no\n"));
    }

    #[test]
    fn nome_no_pactl() {
        assert_eq!(
            nome(None, "@DEFAULT_SOURCE@").as_deref(),
            Some("@DEFAULT_SOURCE@")
        );
        assert_eq!(
            nome(
                Some("pulseaudio:alsa_input.usb-x.mono-fallback"),
                "@DEFAULT_SOURCE@"
            )
            .as_deref(),
            Some("alsa_input.usb-x.mono-fallback")
        );
        assert_eq!(
            nome(Some("alsa:sysdefault:CARD=X"), "@DEFAULT_SOURCE@"),
            None
        );
    }
}
