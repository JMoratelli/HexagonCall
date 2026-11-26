//! G.711, lei A (PCMA) e lei mu (PCMU): o codec dos troncos e do PBX.
//! Segue o g711.c da Sun (dominio publico), o mesmo que o Asterisk usa.
//! As contas trabalham em i32 e os resultados sao mascarados para 8 bits
//! antes de virar u8, entao as conversoes nao perdem nada.

const SEG_A: [i32; 8] = [0x1F, 0x3F, 0x7F, 0xFF, 0x1FF, 0x3FF, 0x7FF, 0xFFF];
const SEG_U: [i32; 8] = [0x3F, 0x7F, 0xFF, 0x1FF, 0x3FF, 0x7FF, 0xFFF, 0x1FFF];
const BIAS: i32 = 0x84;
const CLIP: i32 = 8159;

fn segmento(valor: i32, tabela: &[i32; 8]) -> i32 {
    let mut seg = 0;
    for fim in tabela {
        if valor <= *fim {
            return seg;
        }
        seg += 1;
    }
    seg
}

fn byte(valor: i32) -> u8 {
    u8::try_from(valor & 0xFF).unwrap_or(0)
}

fn amostra(valor: i32) -> i16 {
    i16::try_from(valor.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).unwrap_or(0)
}

pub fn alaw_de_linear(pcm: i16) -> u8 {
    let mut valor = i32::from(pcm) >> 3;
    let mascara = if valor >= 0 {
        0xD5
    } else {
        valor = -valor - 1;
        0x55
    };
    let seg = segmento(valor, &SEG_A);
    if seg >= 8 {
        return byte(0x7F ^ mascara);
    }
    let quantizado = if seg < 2 {
        (valor >> 1) & 0x0F
    } else {
        (valor >> seg) & 0x0F
    };
    byte(((seg << 4) | quantizado) ^ mascara)
}

pub fn linear_de_alaw(codigo: u8) -> i16 {
    let a = i32::from(codigo ^ 0x55);
    let mut t = (a & 0x0F) << 4;
    let seg = (a & 0x70) >> 4;
    match seg {
        0 => t += 8,
        1 => t += 0x108,
        _ => {
            t += 0x108;
            t <<= seg - 1;
        }
    }
    amostra(if a & 0x80 != 0 { t } else { -t })
}

pub fn ulaw_de_linear(pcm: i16) -> u8 {
    let mut valor = i32::from(pcm) >> 2;
    let mascara = if valor < 0 {
        valor = -valor;
        0x7F
    } else {
        0xFF
    };
    valor = valor.min(CLIP) + (BIAS >> 2);
    let seg = segmento(valor, &SEG_U);
    if seg >= 8 {
        return byte(0x7F ^ mascara);
    }
    byte(((seg << 4) | ((valor >> (seg + 1)) & 0x0F)) ^ mascara)
}

pub fn linear_de_ulaw(codigo: u8) -> i16 {
    let u = i32::from(!codigo);
    let mut t = ((u & 0x0F) << 3) + BIAS;
    t <<= (u & 0x70) >> 4;
    amostra(if u & 0x80 != 0 { BIAS - t } else { t - BIAS })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn silencio_tem_o_codigo_conhecido() {
        assert_eq!(alaw_de_linear(0), 0xD5);
        assert_eq!(ulaw_de_linear(0), 0xFF);
        assert_eq!(linear_de_alaw(0xD5), 8);
        assert_eq!(linear_de_ulaw(0xFF), 0);
    }

    #[test]
    fn os_extremos_saturam() {
        assert_eq!(alaw_de_linear(i16::MAX), 0xAA);
        assert_eq!(alaw_de_linear(i16::MIN), 0x2A);
        assert_eq!(ulaw_de_linear(i16::MAX), 0x80);
        assert_eq!(ulaw_de_linear(i16::MIN), 0x00);
    }

    #[test]
    fn alaw_codifica_de_volta_o_mesmo_codigo() {
        for codigo in 0..=255u8 {
            assert_eq!(
                alaw_de_linear(linear_de_alaw(codigo)),
                codigo,
                "{codigo:#04x}"
            );
        }
    }

    #[test]
    fn ulaw_codifica_de_volta_o_mesmo_codigo() {
        // 0x7F e o "zero negativo": decodifica para 0, que volta como 0xFF.
        for codigo in (0..=255u8).filter(|c| *c != 0x7F) {
            assert_eq!(
                ulaw_de_linear(linear_de_ulaw(codigo)),
                codigo,
                "{codigo:#04x}"
            );
        }
    }

    #[test]
    fn o_erro_cabe_no_passo_do_segmento() {
        for pcm in (i16::MIN..=i16::MAX).step_by(7) {
            let a = i32::from(linear_de_alaw(alaw_de_linear(pcm)));
            let u = i32::from(linear_de_ulaw(ulaw_de_linear(pcm)));
            let x = i32::from(pcm);
            // Passo maximo: 1024 na lei A, 1024 na lei mu (segmento de cima).
            assert!((a - x).abs() <= 1024, "alaw {pcm} -> {a}");
            assert!((u - x).abs() <= 1024 + 132, "ulaw {pcm} -> {u}");
        }
    }

    #[test]
    fn cada_codigo_alaw_e_um_nivel_diferente() {
        let mut valores: Vec<i16> = (0..=255u8).map(linear_de_alaw).collect();
        valores.sort_unstable();
        valores.dedup();
        assert_eq!(valores.len(), 256);
    }
}
