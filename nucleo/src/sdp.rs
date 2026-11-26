//! O SDP (RFC 4566) do jeito que o PBX usa: uma linha de audio, G.711 e o
//! DTMF por RTP (telephone-event, RFC 4733). Monta a nossa oferta ou resposta
//! e le a do outro lado.

use std::net::IpAddr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// G.711 lei A: o padrao dos troncos no Brasil, vai primeiro.
    Pcma,
    Pcmu,
}

impl Codec {
    pub const fn tipo(self) -> u8 {
        match self {
            Self::Pcmu => 0,
            Self::Pcma => 8,
        }
    }

    pub const fn nome(self) -> &'static str {
        match self {
            Self::Pcmu => "PCMU",
            Self::Pcma => "PCMA",
        }
    }

    pub const fn de_tipo(tipo: u8) -> Option<Self> {
        match tipo {
            0 => Some(Self::Pcmu),
            8 => Some(Self::Pcma),
            _ => None,
        }
    }
}

pub const NOSSOS_CODECS: [Codec; 2] = [Codec::Pcma, Codec::Pcmu];
/// O tipo que oferecemos para o DTMF. Na resposta vale o do outro lado.
pub const TIPO_DTMF: u8 = 101;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direcao {
    #[default]
    EnviaERecebe,
    SoEnvia,
    SoRecebe,
    Inativa,
}

impl Direcao {
    pub const fn envia(self) -> bool {
        matches!(self, Self::EnviaERecebe | Self::SoEnvia)
    }

    pub const fn recebe(self) -> bool {
        matches!(self, Self::EnviaERecebe | Self::SoRecebe)
    }

    pub const fn de(envia: bool, recebe: bool) -> Self {
        match (envia, recebe) {
            (true, true) => Self::EnviaERecebe,
            (true, false) => Self::SoEnvia,
            (false, true) => Self::SoRecebe,
            (false, false) => Self::Inativa,
        }
    }

    /// Dois limites juntos: so envia (ou recebe) se os dois deixam. Ex.: em
    /// espera (so envia) e o outro lado pedindo sendrecv, a resposta e sendonly.
    pub const fn junta(self, outra: Self) -> Self {
        Self::de(
            self.envia() && outra.envia(),
            self.recebe() && outra.recebe(),
        )
    }

    const fn atributo(self) -> &'static str {
        match self {
            Self::EnviaERecebe => "sendrecv",
            Self::SoEnvia => "sendonly",
            Self::SoRecebe => "recvonly",
            Self::Inativa => "inactive",
        }
    }

    fn de_atributo(a: &str) -> Option<Self> {
        match a {
            "sendrecv" => Some(Self::EnviaERecebe),
            "sendonly" => Some(Self::SoEnvia),
            "recvonly" => Some(Self::SoRecebe),
            "inactive" => Some(Self::Inativa),
            _ => None,
        }
    }
}

/// O nosso lado: para onde o outro manda o audio e o que aceitamos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    pub ip: IpAddr,
    pub porta: u16,
    pub codecs: Vec<Codec>,
    /// O tipo do telephone-event, ou None para nao oferecer DTMF por RTP.
    pub dtmf: Option<u8>,
    pub direcao: Direcao,
}

/// O lado de la, lido do SDP que chegou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remota {
    pub ip: IpAddr,
    pub porta: u16,
    /// Os tipos da linha m=, na ordem de preferencia de quem mandou.
    pub formatos: Vec<u8>,
    pub dtmf: Option<u8>,
    pub direcao: Direcao,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErroSdp {
    #[error("SDP sem linha de audio")]
    SemAudio,
    #[error("SDP sem endereco de conexao")]
    SemConexao,
    #[error("o outro lado recusou o audio (porta 0)")]
    AudioRecusado,
    #[error("linha de SDP invalida: {0:?}")]
    Linha(String),
    #[error("nenhum codec em comum (o outro lado ofereceu {0:?})")]
    SemCodecEmComum(Vec<u8>),
}

/// Monta o SDP do nosso lado. `sessao` fica fixo na ligacao e `versao` sobe
/// a cada nova oferta (espera, retomada).
pub fn monta(sessao: u64, versao: u64, local: &Local) -> String {
    let familia = if local.ip.is_ipv6() { "IP6" } else { "IP4" };
    let mut tipos: Vec<String> = local.codecs.iter().map(|c| c.tipo().to_string()).collect();
    if let Some(dtmf) = local.dtmf {
        tipos.push(dtmf.to_string());
    }
    let mut s = String::new();
    let mut linha = |l: String| {
        s.push_str(&l);
        s.push_str("\r\n");
    };
    linha("v=0".into());
    linha(format!("o=- {sessao} {versao} IN {familia} {}", local.ip));
    linha("s=Hexagon Call".into());
    linha(format!("c=IN {familia} {}", local.ip));
    linha("t=0 0".into());
    linha(format!(
        "m=audio {} RTP/AVP {}",
        local.porta,
        tipos.join(" ")
    ));
    for codec in &local.codecs {
        linha(format!("a=rtpmap:{} {}/8000", codec.tipo(), codec.nome()));
    }
    if let Some(dtmf) = local.dtmf {
        linha(format!("a=rtpmap:{dtmf} telephone-event/8000"));
        linha(format!("a=fmtp:{dtmf} 0-16"));
    }
    linha("a=ptime:20".into());
    linha(format!("a={}", local.direcao.atributo()));
    s
}

/// Le o SDP do outro lado: a primeira linha de audio.
pub fn le(texto: &str) -> Result<Remota, ErroSdp> {
    let mut ip_sessao: Option<IpAddr> = None;
    let mut direcao_sessao = Direcao::EnviaERecebe;
    let mut audio: Option<(u16, Vec<u8>)> = None;
    let mut ip_midia: Option<IpAddr> = None;
    let mut direcao_midia: Option<Direcao> = None;
    let mut dtmf: Option<u8> = None;
    // Dentro de outra linha m= que nao e a nossa (video), tudo e ignorado.
    let mut na_outra_midia = false;

    for linha in texto.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (chave, valor) = linha
            .split_once('=')
            .ok_or_else(|| ErroSdp::Linha(linha.to_string()))?;
        match chave {
            "m" => {
                if audio.is_some() {
                    // A primeira linha de audio basta; o resto e de outra midia.
                    na_outra_midia = true;
                    continue;
                }
                let mut partes = valor.split_whitespace();
                if partes.next() != Some("audio") {
                    na_outra_midia = true;
                    continue;
                }
                na_outra_midia = false;
                let porta = partes
                    .next()
                    .and_then(|p| p.split('/').next())
                    .and_then(|p| p.parse::<u16>().ok())
                    .ok_or_else(|| ErroSdp::Linha(linha.to_string()))?;
                let _protocolo = partes.next();
                let formatos = partes.filter_map(|f| f.parse::<u8>().ok()).collect();
                audio = Some((porta, formatos));
            }
            _ if na_outra_midia => {}
            "c" => {
                let ip = conexao(valor).ok_or_else(|| ErroSdp::Linha(linha.to_string()))?;
                if audio.is_some() {
                    ip_midia = Some(ip);
                } else {
                    ip_sessao = Some(ip);
                }
            }
            "a" => {
                if let Some(d) = Direcao::de_atributo(valor) {
                    if audio.is_some() {
                        direcao_midia = Some(d);
                    } else {
                        direcao_sessao = d;
                    }
                } else if let Some(resto) = valor.strip_prefix("rtpmap:")
                    && let Some((tipo, nome)) = resto.split_once(' ')
                    && nome.to_ascii_lowercase().starts_with("telephone-event/")
                {
                    dtmf = tipo.parse().ok();
                }
            }
            _ => {}
        }
    }

    let (porta, formatos) = audio.ok_or(ErroSdp::SemAudio)?;
    if porta == 0 {
        return Err(ErroSdp::AudioRecusado);
    }
    let ip = ip_midia.or(ip_sessao).ok_or(ErroSdp::SemConexao)?;
    // Espera a moda antiga (RFC 2543): c=0.0.0.0 em vez de sendonly.
    let direcao = if ip.is_unspecified() {
        Direcao::Inativa
    } else {
        direcao_midia.unwrap_or(direcao_sessao)
    };
    let dtmf = dtmf.filter(|t| formatos.contains(t));
    Ok(Remota {
        ip,
        porta,
        formatos,
        dtmf,
        direcao,
    })
}

fn conexao(valor: &str) -> Option<IpAddr> {
    let mut partes = valor.split_whitespace();
    if partes.next() != Some("IN") {
        return None;
    }
    let _familia = partes.next()?;
    // "224.2.1.1/127" (multicast com TTL) tambem cabe aqui.
    partes.next()?.split('/').next()?.parse().ok()
}

/// O codec da ligacao: o primeiro da lista de quem ofereceu que nos tambem
/// temos (RFC 3264: quem responde segue a ordem da oferta).
pub fn escolhe(remota: &Remota, nossos: &[Codec]) -> Result<Codec, ErroSdp> {
    remota
        .formatos
        .iter()
        .filter_map(|t| Codec::de_tipo(*t))
        .find(|c| nossos.contains(c))
        .ok_or_else(|| ErroSdp::SemCodecEmComum(remota.formatos.clone()))
}

/// A direcao da nossa resposta para a direcao que o outro ofereceu.
pub const fn direcao_da_resposta(oferecida: Direcao) -> Direcao {
    match oferecida {
        Direcao::EnviaERecebe => Direcao::EnviaERecebe,
        Direcao::SoEnvia => Direcao::SoRecebe,
        Direcao::SoRecebe => Direcao::SoEnvia,
        Direcao::Inativa => Direcao::Inativa,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A resposta de um Asterisk com chan_sip, como chega do PBX.
    const ASTERISK: &str = "v=0\r\n\
        o=root 1797 1797 IN IP4 192.0.2.10\r\n\
        s=Asterisk PBX 13.38.3\r\n\
        c=IN IP4 192.0.2.10\r\n\
        t=0 0\r\n\
        m=audio 13552 RTP/AVP 8 101\r\n\
        a=rtpmap:8 PCMA/8000\r\n\
        a=rtpmap:101 telephone-event/8000\r\n\
        a=fmtp:101 0-16\r\n\
        a=ptime:20\r\n\
        a=maxptime:150\r\n\
        a=sendrecv\r\n";

    fn local() -> Local {
        Local {
            ip: "192.0.2.50".parse().unwrap(),
            porta: 40000,
            codecs: NOSSOS_CODECS.to_vec(),
            dtmf: Some(TIPO_DTMF),
            direcao: Direcao::EnviaERecebe,
        }
    }

    #[test]
    fn monta_a_oferta() {
        let sdp = monta(7, 1, &local());
        assert!(sdp.contains("o=- 7 1 IN IP4 192.0.2.50\r\n"));
        assert!(sdp.contains("c=IN IP4 192.0.2.50\r\n"));
        assert!(sdp.contains("m=audio 40000 RTP/AVP 8 0 101\r\n"));
        assert!(sdp.contains("a=rtpmap:101 telephone-event/8000\r\n"));
        assert!(sdp.ends_with("a=sendrecv\r\n"));
    }

    #[test]
    fn a_nossa_oferta_le_de_volta() {
        let remota = le(&monta(7, 1, &local())).unwrap();
        assert_eq!(remota.porta, 40000);
        assert_eq!(remota.formatos, vec![8, 0, 101]);
        assert_eq!(remota.dtmf, Some(101));
        assert_eq!(escolhe(&remota, &NOSSOS_CODECS), Ok(Codec::Pcma));
    }

    #[test]
    fn le_a_resposta_do_asterisk() {
        let remota = le(ASTERISK).unwrap();
        assert_eq!(remota.ip, "192.0.2.10".parse::<IpAddr>().unwrap());
        assert_eq!(remota.porta, 13552);
        assert_eq!(remota.dtmf, Some(101));
        assert_eq!(remota.direcao, Direcao::EnviaERecebe);
        assert_eq!(escolhe(&remota, &NOSSOS_CODECS), Ok(Codec::Pcma));
    }

    #[test]
    fn segue_a_ordem_de_quem_ofereceu() {
        let remota = le(&ASTERISK.replace("RTP/AVP 8 101", "RTP/AVP 0 8 101")).unwrap();
        assert_eq!(escolhe(&remota, &NOSSOS_CODECS), Ok(Codec::Pcmu));
    }

    #[test]
    fn sem_codec_em_comum() {
        let remota = le(&ASTERISK.replace("RTP/AVP 8 101", "RTP/AVP 9 101")).unwrap();
        assert_eq!(
            escolhe(&remota, &NOSSOS_CODECS),
            Err(ErroSdp::SemCodecEmComum(vec![9, 101]))
        );
    }

    #[test]
    fn conexao_da_midia_vence_a_da_sessao() {
        let sdp = ASTERISK.replace("a=ptime:20", "c=IN IP4 10.1.1.1\r\na=ptime:20");
        assert_eq!(le(&sdp).unwrap().ip, "10.1.1.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn espera_pelos_dois_jeitos() {
        let sendonly = le(&ASTERISK.replace("a=sendrecv", "a=sendonly")).unwrap();
        assert_eq!(sendonly.direcao, Direcao::SoEnvia);
        assert_eq!(direcao_da_resposta(sendonly.direcao), Direcao::SoRecebe);
        let antiga = le(&ASTERISK.replace("c=IN IP4 192.0.2.10", "c=IN IP4 0.0.0.0")).unwrap();
        assert_eq!(antiga.direcao, Direcao::Inativa);
    }

    #[test]
    fn ignora_o_video_e_recusa_audio_com_porta_zero() {
        let com_video = format!("{ASTERISK}m=video 0 RTP/AVP 99\r\na=inactive\r\n");
        assert_eq!(le(&com_video).unwrap().direcao, Direcao::EnviaERecebe);
        let recusado = ASTERISK.replace("m=audio 13552", "m=audio 0");
        assert_eq!(le(&recusado), Err(ErroSdp::AudioRecusado));
    }

    #[test]
    fn recusa_sdp_sem_audio_ou_sem_conexao() {
        assert_eq!(le("v=0\r\nc=IN IP4 1.2.3.4\r\n"), Err(ErroSdp::SemAudio));
        assert_eq!(
            le("v=0\r\nm=audio 1000 RTP/AVP 8\r\n"),
            Err(ErroSdp::SemConexao)
        );
        assert!(matches!(le("lixo"), Err(ErroSdp::Linha(_))));
    }

    #[test]
    fn espera_junta_com_o_que_o_outro_pede() {
        use Direcao::*;
        // Em espera (so envia), o Asterisk renovando com sendrecv: sendonly.
        assert_eq!(SoEnvia.junta(direcao_da_resposta(EnviaERecebe)), SoEnvia);
        // Os dois em espera um do outro: ninguem manda nada.
        assert_eq!(SoEnvia.junta(direcao_da_resposta(SoEnvia)), Inativa);
        // Fora de espera, o que o outro pedir.
        assert_eq!(EnviaERecebe.junta(direcao_da_resposta(SoEnvia)), SoRecebe);
        assert_eq!(EnviaERecebe.junta(direcao_da_resposta(Inativa)), Inativa);
        for d in [EnviaERecebe, SoEnvia, SoRecebe, Inativa] {
            assert_eq!(Direcao::de(d.envia(), d.recebe()), d);
        }
    }
}
