//! O cabecalho RTP (RFC 3550): o que vai na frente de cada 20 ms de voz.

use thiserror::Error;

pub const TAMANHO_CABECALHO: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cabecalho {
    /// O tipo da carga: 8 = PCMA, 0 = PCMU, 101 = DTMF (telephone-event).
    pub tipo: u8,
    pub marcador: bool,
    pub sequencia: u16,
    pub carimbo: u32,
    pub ssrc: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErroRtp {
    #[error("pacote curto demais para RTP")]
    Curto,
    #[error("versao RTP {0}, esperava 2")]
    Versao(u8),
    #[error("enchimento maior que o pacote")]
    Enchimento,
}

impl Cabecalho {
    /// Escreve cabecalho e carga em `saida`, que e reaproveitada a cada pacote.
    pub fn escreve(&self, carga: &[u8], saida: &mut Vec<u8>) {
        saida.clear();
        saida.reserve(TAMANHO_CABECALHO + carga.len());
        saida.push(0x80);
        saida.push((u8::from(self.marcador) << 7) | (self.tipo & 0x7F));
        saida.extend_from_slice(&self.sequencia.to_be_bytes());
        saida.extend_from_slice(&self.carimbo.to_be_bytes());
        saida.extend_from_slice(&self.ssrc.to_be_bytes());
        saida.extend_from_slice(carga);
    }
}

fn u16_em(p: &[u8], i: usize) -> Result<u16, ErroRtp> {
    match p.get(i..i + 2) {
        Some(&[a, b]) => Ok(u16::from_be_bytes([a, b])),
        _ => Err(ErroRtp::Curto),
    }
}

fn u32_em(p: &[u8], i: usize) -> Result<u32, ErroRtp> {
    match p.get(i..i + 4) {
        Some(&[a, b, c, d]) => Ok(u32::from_be_bytes([a, b, c, d])),
        _ => Err(ErroRtp::Curto),
    }
}

/// Le um pacote: o cabecalho e a carga, ja sem CSRC, extensao e enchimento.
pub fn le(pacote: &[u8]) -> Result<(Cabecalho, &[u8]), ErroRtp> {
    let (&primeiro, &segundo) = match (pacote.first(), pacote.get(1)) {
        (Some(a), Some(b)) if pacote.len() >= TAMANHO_CABECALHO => (a, b),
        _ => return Err(ErroRtp::Curto),
    };
    let versao = primeiro >> 6;
    if versao != 2 {
        return Err(ErroRtp::Versao(versao));
    }
    let cabecalho = Cabecalho {
        tipo: segundo & 0x7F,
        marcador: segundo & 0x80 != 0,
        sequencia: u16_em(pacote, 2)?,
        carimbo: u32_em(pacote, 4)?,
        ssrc: u32_em(pacote, 8)?,
    };
    let mut inicio = TAMANHO_CABECALHO + 4 * usize::from(primeiro & 0x0F);
    if primeiro & 0x10 != 0 {
        let palavras = usize::from(u16_em(pacote, inicio + 2)?);
        inicio += 4 + 4 * palavras;
    }
    let mut fim = pacote.len();
    if primeiro & 0x20 != 0 {
        let enchimento = usize::from(*pacote.last().ok_or(ErroRtp::Curto)?);
        if enchimento == 0 || enchimento > fim.saturating_sub(inicio) {
            return Err(ErroRtp::Enchimento);
        }
        fim -= enchimento;
    }
    let carga = pacote.get(inicio..fim).ok_or(ErroRtp::Curto)?;
    Ok((cabecalho, carga))
}

#[cfg(test)]
mod testes {
    use super::*;

    const CAB: Cabecalho = Cabecalho {
        tipo: 8,
        marcador: true,
        sequencia: 0xBEEF,
        carimbo: 0x0102_0304,
        ssrc: 0xCAFE_BABE,
    };

    #[test]
    fn ida_e_volta() {
        let mut buf = Vec::new();
        CAB.escreve(&[1, 2, 3], &mut buf);
        assert_eq!(buf.len(), 15);
        assert_eq!(&buf[..2], &[0x80, 0x88]);
        let (lido, carga) = le(&buf).unwrap();
        assert_eq!(lido, CAB);
        assert_eq!(carga, &[1, 2, 3]);
    }

    #[test]
    fn pula_csrc_e_extensao() {
        let mut buf = Vec::new();
        CAB.escreve(&[], &mut buf);
        buf[0] = 0x80 | 0x10 | 0x01; // X=1, CC=1
        buf.extend_from_slice(&[0, 0, 0, 1]); // CSRC
        buf.extend_from_slice(&[0xBE, 0xDE, 0, 1]); // extensao de 1 palavra
        buf.extend_from_slice(&[9, 9, 9, 9]);
        buf.extend_from_slice(&[7, 7]); // carga
        let (_, carga) = le(&buf).unwrap();
        assert_eq!(carga, &[7, 7]);
    }

    #[test]
    fn tira_o_enchimento() {
        let mut buf = Vec::new();
        CAB.escreve(&[5, 6, 0, 0, 3], &mut buf);
        buf[0] |= 0x20;
        let (_, carga) = le(&buf).unwrap();
        assert_eq!(carga, &[5, 6]);
    }

    #[test]
    fn recusa_lixo() {
        assert_eq!(le(&[0x80; 5]), Err(ErroRtp::Curto));
        let mut buf = Vec::new();
        CAB.escreve(&[1], &mut buf);
        buf[0] = 0x40;
        assert_eq!(le(&buf), Err(ErroRtp::Versao(1)));
        buf[0] = 0xA0;
        *buf.last_mut().unwrap() = 200;
        assert_eq!(le(&buf), Err(ErroRtp::Enchimento));
        CAB.escreve(&[], &mut buf);
        buf[0] = 0x8F; // 15 CSRC num pacote de 12 bytes
        assert_eq!(le(&buf), Err(ErroRtp::Curto));
    }
}
