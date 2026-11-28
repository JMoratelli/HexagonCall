//! A qualidade da ligacao, ao vivo (pedido de 28/11): o que chega do outro lado
//! (perda e variacao, pela sequencia do RTP) e o caminho ate o PBX (a sonda,
//! um OPTIONS de 2 em 2 s durante a ligacao), para dizer de quem e o problema.
//! A sonda vai e volta pelo mesmo caminho do audio daqui ate o PBX: se ela
//! falha ou demora, e a conexao daqui; se ela esta boa e o audio chega
//! picotado, o problema esta de la (o outro lado, ou do PBX para la).

/// Conta o que chega pela sequencia do RTP: quantos deviam ter chegado (o
/// avanco da maior sequencia, com a volta do u16) e quantos chegaram.
#[derive(Debug, Default, Clone, Copy)]
pub struct Contagem {
    ultima: Option<u16>,
    pub esperados: u32,
    pub recebidos: u32,
}

impl Contagem {
    pub fn chegou(&mut self, sequencia: u16) {
        self.recebidos = self.recebidos.wrapping_add(1);
        let Some(ultima) = self.ultima else {
            self.ultima = Some(sequencia);
            self.esperados = 1;
            return;
        };
        // Mais nova (ate meia volta a frente): avanca. Repetida ou atrasada: so
        // conta como recebida.
        let avanco = sequencia.wrapping_sub(ultima);
        if avanco != 0 && avanco < 0x8000 {
            self.esperados = self.esperados.wrapping_add(u32::from(avanco));
            self.ultima = Some(sequencia);
        }
    }
}

/// Uma janela de medicao (2 s): o que devia ter chegado, o que chegou, a
/// variacao entre chegadas e quanto o PBX levou para responder a sonda (None:
/// nao respondeu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Janela {
    pub esperados: u32,
    pub recebidos: u32,
    pub variacao_ms: u32,
    pub pbx_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Culpa {
    /// A conexao daqui com o PBX.
    Aqui,
    /// O audio chega ruim com o caminho ate o PBX bom: o outro lado, ou do PBX
    /// para la.
    La,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Avaliacao {
    /// 3 boa, 2 fraca, 1 ruim.
    pub nivel: u8,
    pub culpa: Option<Culpa>,
    /// A perda do que chega, em por cento.
    pub perda: u32,
    /// A maior variacao entre chegadas, em ms.
    pub variacao_ms: u32,
    /// A resposta mais lenta da sonda; None se alguma nao veio.
    pub pbx_ms: Option<u32>,
}

/// Quantas janelas a avaliacao olha (10 s): uma rajada so nao vira alarme, e o
/// alarme nao fica muito depois de passar.
pub const JANELAS: usize = 5;

/// A avaliacao das ultimas janelas (a mais nova no fim). None com menos de
/// duas: ainda nao da para dizer.
pub fn avalia(janelas: &[Janela]) -> Option<Avaliacao> {
    let inicio = janelas.len().saturating_sub(JANELAS);
    let janelas = janelas.get(inicio..)?;
    if janelas.len() < 2 {
        return None;
    }
    let esperados: u32 = janelas.iter().map(|j| j.esperados).sum();
    let recebidos: u32 = janelas.iter().map(|j| j.recebidos.min(j.esperados)).sum();
    let perda = ((esperados - recebidos) * 100)
        .checked_div(esperados)
        .unwrap_or(100);
    // Uma janela inteira sem nada chegando e o audio que parou.
    let parou = janelas.iter().any(|j| j.esperados == 0);
    let variacao_ms = janelas.iter().map(|j| j.variacao_ms).max().unwrap_or(0);
    let respostas: Option<Vec<u32>> = janelas.iter().map(|j| j.pbx_ms).collect();
    let pbx_ms = respostas.as_ref().and_then(|r| r.iter().max().copied());
    let espalhado = respostas.as_ref().map_or(0, |r| {
        r.iter().max().unwrap_or(&0) - r.iter().min().unwrap_or(&0)
    });

    // O caminho ate o PBX: sem resposta ou muito lento e ruim; lento ou
    // irregular (a fila de um Wi-Fi ocupado) e fraco.
    let caminho = match pbx_ms {
        None => 1,
        Some(ms) if ms >= 400 || espalhado >= 200 => 1,
        Some(ms) if ms >= 200 || espalhado >= 100 => 2,
        Some(_) => 3,
    };
    // O que chega: o G.711 com o buffer de variacao aguenta um pouco de perda e
    // de variacao sem se ouvir.
    let chegada = if parou || perda >= 5 || variacao_ms >= 80 {
        1
    } else if perda >= 2 || variacao_ms >= 40 {
        2
    } else {
        3
    };
    let nivel = caminho.min(chegada);
    let culpa = if nivel == 3 {
        None
    } else if caminho < 3 {
        Some(Culpa::Aqui)
    } else {
        Some(Culpa::La)
    };
    Some(Avaliacao {
        nivel,
        culpa,
        perda,
        variacao_ms,
        pbx_ms,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn janela(esperados: u32, recebidos: u32, variacao_ms: u32, pbx_ms: Option<u32>) -> Janela {
        Janela {
            esperados,
            recebidos,
            variacao_ms,
            pbx_ms,
        }
    }

    #[test]
    fn a_contagem_segue_a_sequencia_com_perda_atraso_e_volta() {
        let mut c = Contagem::default();
        for s in [65_530u16, 65_531, 65_533, 65_532, 65_532, 65_535, 0, 1, 3] {
            c.chegou(s);
        }
        // De 65530 a 3 (com a volta) deviam ser 10; chegaram 9 pacotes, um deles
        // repetido: faltaram 65534 e 2.
        assert_eq!(c.esperados, 10);
        assert_eq!(c.recebidos, 9);
    }

    #[test]
    fn tudo_bem_e_nivel_tres_sem_culpa() {
        let j = [janela(100, 100, 8, Some(18)); 5];
        let a = avalia(&j).unwrap();
        assert_eq!((a.nivel, a.culpa, a.perda), (3, None, 0));
    }

    #[test]
    fn com_menos_de_duas_janelas_ainda_nao_diz() {
        assert_eq!(avalia(&[janela(100, 100, 8, Some(18))]), None);
    }

    #[test]
    fn sonda_sem_resposta_e_a_conexao_daqui() {
        let mut j = [janela(100, 90, 30, Some(20)); 5];
        j[3].pbx_ms = None;
        let a = avalia(&j).unwrap();
        assert_eq!((a.nivel, a.culpa, a.pbx_ms), (1, Some(Culpa::Aqui), None));
    }

    #[test]
    fn sonda_irregular_e_perda_e_a_conexao_daqui() {
        let j = [
            janela(100, 97, 30, Some(20)),
            janela(100, 96, 50, Some(260)),
            janela(100, 98, 20, Some(40)),
        ];
        let a = avalia(&j).unwrap();
        assert_eq!((a.nivel, a.culpa), (1, Some(Culpa::Aqui)));
    }

    #[test]
    fn audio_picotado_com_o_caminho_bom_e_de_la() {
        let j = [janela(100, 92, 20, Some(18)); 5];
        let a = avalia(&j).unwrap();
        assert_eq!((a.nivel, a.culpa, a.perda), (1, Some(Culpa::La), 8));
    }

    #[test]
    fn audio_que_parou_e_ruim() {
        let j = [janela(100, 100, 8, Some(18)), janela(0, 0, 8, Some(18))];
        let a = avalia(&j).unwrap();
        assert_eq!((a.nivel, a.culpa), (1, Some(Culpa::La)));
    }

    #[test]
    fn variacao_media_e_fraca() {
        let j = [janela(100, 100, 45, Some(18)); 3];
        assert_eq!(avalia(&j).unwrap().nivel, 2);
    }

    #[test]
    fn olha_so_as_ultimas_cinco() {
        let mut j = vec![janela(100, 50, 8, None); 3];
        j.extend([janela(100, 100, 8, Some(18)); 5]);
        assert_eq!(avalia(&j).unwrap().nivel, 3);
    }
}
