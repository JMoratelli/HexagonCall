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
    /// Tocava aqui e em outros aparelhos (grupo de ramais, fila) e outro
    /// atendeu: o CANCEL veio com o motivo de atendida em outro lugar.
    AtendidaEmOutro,
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

/// O cabecalho Reason (RFC 3326) de um CANCEL diz que a ligacao foi atendida
/// em outro lugar: `SIP;cause=200` (o Asterisk manda com o texto "Call
/// completed elsewhere") ou `Q.850;cause=26`. Podem vir varios motivos
/// separados por virgula, e o texto entre aspas pode ter virgula.
pub fn atendida_em_outro_lugar(reason: &str) -> bool {
    fora_das_aspas(reason, ',').into_iter().any(|motivo| {
        let mut campos = fora_das_aspas(motivo, ';').into_iter();
        let protocolo = campos
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase();
        let causa = campos.find_map(|campo| {
            let (nome, valor) = campo.split_once('=')?;
            nome.trim()
                .eq_ignore_ascii_case("cause")
                .then(|| valor.trim().trim_matches('"').parse::<u16>().ok())
                .flatten()
        });
        matches!(
            (protocolo.as_str(), causa),
            ("SIP", Some(200)) | ("Q.850", Some(26))
        )
    })
}

/// Divide `texto` em `separador`, menos dentro de aspas (com `\"` escapado).
fn fora_das_aspas(texto: &str, separador: char) -> Vec<&str> {
    let mut partes = Vec::new();
    let (mut inicio, mut aspas, mut escapado) = (0, false, false);
    for (i, c) in texto.char_indices() {
        if escapado {
            escapado = false;
        } else if aspas && c == '\\' {
            escapado = true;
        } else if c == '"' {
            aspas = !aspas;
        } else if c == separador && !aspas {
            partes.push(texto.get(inicio..i).unwrap_or_default());
            inicio = i + c.len_utf8();
        }
    }
    partes.push(texto.get(inicio..).unwrap_or_default());
    partes
}

/// O codigo do sipfrag que vem no NOTIFY de uma transferencia (RFC 3515):
/// "SIP/2.0 200 OK" -> 200. A primeira linha basta; o resto, quando vem, sao
/// cabecalhos da resposta.
pub fn codigo_do_sipfrag(corpo: &str) -> Option<u16> {
    let mut partes = corpo.lines().next()?.split_whitespace();
    if !partes.next()?.eq_ignore_ascii_case("SIP/2.0") {
        return None;
    }
    partes
        .next()?
        .parse()
        .ok()
        .filter(|c| (100..700).contains(c))
}

impl Fim {
    pub fn texto(self) -> String {
        match self {
            Self::Desligou => "Você desligou".into(),
            Self::OutroDesligou => "O outro lado desligou".into(),
            Self::Cancelada => "Ligação cancelada".into(),
            Self::Perdida => "Ligação perdida".into(),
            Self::AtendidaEmOutro => "Atendida em outro ramal".into(),
            Self::Recusada => "Ligação recusada".into(),
            Self::Ocupado => "Ocupado: o ramal está em outra ligação (486)".into(),
            // O 480 vem do aparelho desligado e tambem da fila sem ninguem livre:
            // "offline" enganava no segundo caso.
            Self::Offline => "Indisponível agora (480)".into(),
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
    fn o_sipfrag_do_notify_da_transferencia() {
        assert_eq!(codigo_do_sipfrag("SIP/2.0 100 Trying\r\n"), Some(100));
        assert_eq!(codigo_do_sipfrag("SIP/2.0 200 OK"), Some(200));
        assert_eq!(
            codigo_do_sipfrag("SIP/2.0 503 Service Unavailable\r\nRetry-After: 5\r\n"),
            Some(503)
        );
        assert_eq!(codigo_do_sipfrag(""), None);
        assert_eq!(codigo_do_sipfrag("<presence/>"), None);
        assert_eq!(codigo_do_sipfrag("SIP/2.0 OK"), None);
        assert_eq!(codigo_do_sipfrag("SIP/2.0 99 Baixo"), None);
    }

    #[test]
    fn o_reason_de_atendida_em_outro_lugar() {
        // O que o chan_sip e o chan_pjsip mandam.
        assert!(atendida_em_outro_lugar(
            r#"SIP;cause=200;text="Call completed elsewhere""#
        ));
        assert!(atendida_em_outro_lugar("Q.850;cause=26"));
        assert!(atendida_em_outro_lugar(" sip ; CAUSE = 200 "));
        // Mais de um motivo, e virgula e aspas escapadas dentro do texto.
        assert!(atendida_em_outro_lugar(
            r#"Q.850;cause=16;text="a, \"b\", c", SIP;cause=200"#
        ));
        assert!(!atendida_em_outro_lugar("Q.850;cause=16"));
        assert!(!atendida_em_outro_lugar(r#"SIP;text="cause=200""#));
        assert!(!atendida_em_outro_lugar("SIP;cause=487"));
        assert!(!atendida_em_outro_lugar(""));
        assert!(!Fim::AtendidaEmOutro.foi_atendida());
    }

    #[test]
    fn a_frase_leva_o_codigo_quando_ajuda() {
        assert_eq!(Fim::Offline.texto(), "Indisponível agora (480)");
        assert_eq!(Fim::Falha(503).texto(), "A ligação falhou (503)");
        assert!(Fim::Desligou.foi_atendida());
        assert!(!Fim::Ocupado.foi_atendida());
    }
}
