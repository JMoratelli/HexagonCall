//! Por que uma ligacao acabou, em palavras de quem usa. O codigo SIP vai
//! junto (o suporte reconhece 480 e 486), mas a frase vem primeiro.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fim {
    /// Quem usa o app desligou.
    Desligou,
    /// O outro lado desligou.
    OutroDesligou,
    /// Desistimos antes de atenderem.
    Cancelada,
    /// O outro lado desistiu antes de atendermos, ou ninguem atendeu.
    Perdida,
    Recusada,
    Ocupado,
    Offline,
    NaoExiste,
    SemResposta,
    /// Transferida para outro ramal.
    Transferida,
    Falha(u16),
}

/// O fim de uma ligacao que nao chegou a ser atendida, pela resposta final.
pub const fn fim_por_status(codigo: u16) -> Fim {
    match codigo {
        404 | 484 | 604 => Fim::NaoExiste,
        408 => Fim::SemResposta,
        480 => Fim::Offline,
        486 | 600 => Fim::Ocupado,
        487 => Fim::Cancelada,
        603 => Fim::Recusada,
        _ => Fim::Falha(codigo),
    }
}

impl Fim {
    pub fn texto(self) -> String {
        match self {
            Self::Desligou => "Você desligou".into(),
            Self::OutroDesligou => "O outro lado desligou".into(),
            Self::Cancelada => "Ligação cancelada".into(),
            Self::Perdida => "Ligação perdida".into(),
            Self::Recusada => "Ligação recusada".into(),
            Self::Ocupado => "Ocupado: o ramal está em outra ligação (486)".into(),
            Self::Offline => "O ramal está offline (480)".into(),
            Self::NaoExiste => "Esse ramal ou número não existe (404)".into(),
            Self::SemResposta => "Ninguém respondeu: o PBX desistiu (408)".into(),
            Self::Transferida => "Ligação transferida".into(),
            // 0: a falha nao veio do PBX (rede, resposta que nao chegou).
            Self::Falha(0) => "A ligação falhou".into(),
            Self::Falha(codigo) => format!("A ligação falhou ({codigo})"),
        }
    }

    /// Se a ligacao chegou a ser atendida.
    pub const fn foi_atendida(self) -> bool {
        matches!(
            self,
            Self::Desligou | Self::OutroDesligou | Self::Transferida
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_codigos_que_o_pbx_manda() {
        assert_eq!(fim_por_status(480), Fim::Offline);
        assert_eq!(fim_por_status(486), Fim::Ocupado);
        assert_eq!(fim_por_status(600), Fim::Ocupado);
        assert_eq!(fim_por_status(603), Fim::Recusada);
        assert_eq!(fim_por_status(404), Fim::NaoExiste);
        assert_eq!(fim_por_status(487), Fim::Cancelada);
        assert_eq!(fim_por_status(408), Fim::SemResposta);
        assert_eq!(fim_por_status(503), Fim::Falha(503));
    }

    #[test]
    fn a_frase_leva_o_codigo_quando_ajuda() {
        assert_eq!(Fim::Offline.texto(), "O ramal está offline (480)");
        assert_eq!(Fim::Falha(503).texto(), "A ligação falhou (503)");
        assert!(Fim::Desligou.foi_atendida());
        assert!(!Fim::Ocupado.foi_atendida());
    }
}
