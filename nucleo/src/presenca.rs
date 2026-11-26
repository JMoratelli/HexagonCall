//! A presenca dos ramais, do jeito que o chan_sip manda no NOTIFY
//! (application/pidf+xml): `basic` open/closed e uma `note` em ingles.
//! No chan_sip, "ocupado" vem como closed + "On the phone": por isso o
//! closed sozinho nao quer dizer offline.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presenca {
    Livre,
    EmLigacao,
    Chamando,
    Offline,
    Desconhecida,
}

impl Presenca {
    pub const fn texto(self) -> &'static str {
        match self {
            Self::Livre => "Livre",
            Self::EmLigacao => "Em ligação",
            Self::Chamando => "Chamando",
            Self::Offline => "Offline",
            Self::Desconhecida => "Desconhecida",
        }
    }
}

/// O texto de dentro de `<nome>` (com ou sem prefixo de namespace).
fn conteudo<'a>(xml: &'a str, nome: &str) -> Option<&'a str> {
    let minusculo = xml.to_ascii_lowercase();
    let mut busca = 0;
    while let Some(pos) = minusculo.get(busca..)?.find(nome) {
        let inicio = busca + pos;
        busca = inicio + nome.len();
        // Tem de ser o nome inteiro da tag: "<basic>" ou "<pidf:basic>".
        let antes = minusculo.get(..inicio)?.chars().next_back();
        let depois = minusculo.get(busca..)?.chars().next();
        if !matches!(antes, Some('<' | ':')) || !matches!(depois, Some('>')) {
            continue;
        }
        let abre = busca + 1;
        let fecha = minusculo.get(abre..)?.find("</")? + abre;
        return xml.get(abre..fecha).map(str::trim);
    }
    None
}

pub fn de_pidf(xml: &str) -> Presenca {
    let basic = conteudo(xml, "basic").map(str::to_ascii_lowercase);
    let nota = conteudo(xml, "note")
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let no_telefone = xml.to_ascii_lowercase().contains("on-the-phone");

    if nota.contains("ringing") {
        return Presenca::Chamando;
    }
    if nota.contains("on the phone") || nota.contains("on hold") || no_telefone {
        return Presenca::EmLigacao;
    }
    match basic.as_deref() {
        Some("open") => Presenca::Livre,
        Some("closed") => Presenca::Offline,
        _ => Presenca::Desconhecida,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O NOTIFY que o PBX mandou na bancada de 26/11/2026 (ramal livre), com os
    /// enderecos trocados pelos de exemplo.
    const LIVRE: &str = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<presence xmlns="urn:ietf:params:xml:ns:pidf"
xmlns:pp="urn:ietf:params:xml:ns:pidf:person"
xmlns:es="urn:ietf:params:xml:ns:pidf:rpid:status:rpid-status"
xmlns:ep="urn:ietf:params:xml:ns:pidf:rpid:rpid-person"
entity="sip:1001@pbx.exemplo.com.br">
<pp:person><status>
</status></pp:person>
<note>Ready</note>
<tuple id="1002">
<contact priority="1">sip:1002@pbx.exemplo.com.br</contact>
<status><basic>open</basic></status>
</tuple>
</presence>"#;

    fn com(nota: &str, basic: &str) -> String {
        LIVRE
            .replace("<note>Ready</note>", &format!("<note>{nota}</note>"))
            .replace("<basic>open</basic>", &format!("<basic>{basic}</basic>"))
    }

    #[test]
    fn livre_como_veio_do_pbx() {
        assert_eq!(de_pidf(LIVRE), Presenca::Livre);
    }

    #[test]
    fn os_estados_do_chan_sip() {
        assert_eq!(de_pidf(&com("Unavailable", "closed")), Presenca::Offline);
        assert_eq!(de_pidf(&com("On the phone", "open")), Presenca::EmLigacao);
        // Ocupado (AST_EXTENSION_BUSY): closed, mas nao e offline.
        assert_eq!(de_pidf(&com("On the phone", "closed")), Presenca::EmLigacao);
        assert_eq!(de_pidf(&com("Ringing", "open")), Presenca::Chamando);
        assert_eq!(de_pidf(&com("On hold", "open")), Presenca::EmLigacao);
    }

    #[test]
    fn rpid_no_telefone_sem_nota() {
        let xml = LIVRE.replace("<note>Ready</note>", "").replace(
            "<status>\n</status>",
            "<ep:activities><ep:on-the-phone/></ep:activities>",
        );
        assert_eq!(de_pidf(&xml), Presenca::EmLigacao);
    }

    #[test]
    fn prefixo_de_namespace_e_maiusculas() {
        let xml = "<pidf:presence><pidf:tuple><pidf:status><pidf:basic>CLOSED</pidf:basic></pidf:status></pidf:tuple></pidf:presence>";
        assert_eq!(de_pidf(xml), Presenca::Offline);
    }

    #[test]
    fn nao_confunde_tag_parecida() {
        // "basics" nao e "basic"; sem estado conhecido, fica desconhecida.
        assert_eq!(de_pidf("<basics>open</basics>"), Presenca::Desconhecida);
        assert_eq!(de_pidf("lixo"), Presenca::Desconhecida);
    }
}
