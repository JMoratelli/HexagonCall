//! Os tons do teclado durante a ligacao (DTMF), como eventos RTP da RFC 4733
//! ("telephone-event"): menu de URA, senha, ramal de destino. Cada tecla vai
//! em pacotes de 20 ms com a duracao crescendo, e o fim vai tres vezes (a RFC
//! pede, porque UDP perde pacote).

/// Quadros (20 ms) com o tom tocando: 100 ms, o que URA e PBX aceitam.
pub const QUADROS_DO_TOM: u16 = 5;
/// Quantas vezes o pacote de fim vai (RFC 4733, 2.5.1.4).
pub const FINS: u16 = 3;
/// Quadros de pausa entre um tom e o seguinte (60 ms).
pub const PAUSA: u16 = 3;
/// O volume do evento: -10 dBm0, o usual.
const VOLUME: u8 = 10;

/// O codigo do evento de uma tecla (0-9, * = 10, # = 11, A-D = 12-15).
pub fn evento(tecla: char) -> Option<u8> {
    match tecla {
        '0'..='9' => u8::try_from(tecla.to_digit(10)?).ok(),
        '*' => Some(10),
        '#' => Some(11),
        'A'..='D' => u8::try_from(u32::from(tecla) - u32::from('A') + 12).ok(),
        'a'..='d' => u8::try_from(u32::from(tecla) - u32::from('a') + 12).ok(),
        _ => None,
    }
}

/// A carga de um pacote: evento, fim (bit E), volume e a duracao em amostras
/// desde o comeco do tom.
pub fn carga(evento: u8, fim: bool, duracao: u16) -> [u8; 4] {
    let [alto, baixo] = duracao.to_be_bytes();
    [evento, if fim { 0x80 } else { 0 } | VOLUME, alto, baixo]
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn teclas_viram_eventos() {
        assert_eq!(evento('0'), Some(0));
        assert_eq!(evento('9'), Some(9));
        assert_eq!(evento('*'), Some(10));
        assert_eq!(evento('#'), Some(11));
        assert_eq!(evento('A'), Some(12));
        assert_eq!(evento('d'), Some(15));
        assert_eq!(evento('x'), None);
    }

    #[test]
    fn carga_da_rfc_4733() {
        // Tecla 5, sem fim, 480 amostras (60 ms a 8 kHz).
        assert_eq!(carga(5, false, 480), [5, 10, 0x01, 0xE0]);
        // O fim liga o bit E.
        assert_eq!(carga(11, true, 800), [11, 0x80 | 10, 0x03, 0x20]);
    }
}
