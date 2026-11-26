//! As contas SIP. Um aparelho pode ter varias (pedido de 26/11/2026): cada
//! conta vira um endpoint proprio no motor, com porta e registro separados.
//! A senha nunca fica aqui nem no arquivo: vem do chaveiro do sistema.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// So UDP por enquanto: e o que o PBX de hoje usa (chan_sip, porta 5060).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transporte {
    #[default]
    Udp,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Conta {
    /// O ramal: a parte do usuario no endereco (1001).
    pub usuario: String,
    /// O PBX (pbx.exemplo.com.br). Tambem e para onde os pacotes vao.
    pub dominio: String,
    #[serde(default = "porta_padrao")]
    pub porta: u16,
    /// O nome que o outro lado ve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nome: Option<String>,
    /// Quando o PBX autentica com um nome diferente do ramal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usuario_auth: Option<String>,
    #[serde(default)]
    pub transporte: Transporte,
    /// Sem registro a conta so faz: liga, manda mensagem e assina presenca.
    #[serde(default = "registra_padrao")]
    pub registra: bool,
    #[serde(default = "expira_padrao")]
    pub expira_s: u32,
    /// CRLF para o PBX a cada N segundos. O firewall da maquina esquece uma
    /// conversa UDP em 120 s sem trafego, e os avisos do PBX (NOTIFY, INVITE)
    /// passam a ser barrados: medido na bancada de 26/11/2026.
    #[serde(default = "sinal_padrao")]
    pub sinal_de_vida_s: u32,
}

const fn porta_padrao() -> u16 {
    5060
}
const fn registra_padrao() -> bool {
    true
}
const fn expira_padrao() -> u32 {
    600
}
const fn sinal_padrao() -> u32 {
    20
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErroConta {
    #[error("arquivo de contas invalido: {0}")]
    Arquivo(String),
    #[error("nenhuma conta no arquivo")]
    Vazio,
    #[error("conta sem usuario")]
    SemUsuario,
    #[error("o usuario {0:?} tem caractere que o SIP nao aceita")]
    UsuarioInvalido(String),
    #[error("a conta {0} esta sem dominio")]
    SemDominio(String),
    #[error("a conta {0} esta com a porta 0")]
    PortaZero(String),
    #[error("a conta {0} aparece duas vezes")]
    Repetida(String),
    #[error("a conta {conta}: {campo} fora do intervalo ({min} a {max})")]
    ForaDoIntervalo {
        conta: String,
        campo: &'static str,
        min: u32,
        max: u32,
    },
    #[error("ramal ou numero invalido: {0:?}")]
    RamalInvalido(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arquivo {
    #[serde(default)]
    conta: Vec<Conta>,
}

#[derive(Serialize)]
struct ArquivoSaida<'a> {
    conta: &'a [Conta],
}

/// O comeco do arquivo, para quem abrir no editor (como os .conf do GOnnect).
const CABECALHO: &str = "\
# Contas do Hexagon Call: uma [[conta]] por ramal. O app grava este arquivo
# pelas configuracoes; pode editar a mao com o app fechado.
# A senha nao fica aqui: vai para o chaveiro do sistema.

";

/// O arquivo de contas para gravar: o mesmo formato que `le_contas` le.
pub fn escreve_contas(contas: &[Conta]) -> Result<String, ErroConta> {
    let corpo = toml::to_string(&ArquivoSaida { conta: contas })
        .map_err(|e| ErroConta::Arquivo(e.to_string()))?;
    Ok(format!("{CABECALHO}{corpo}"))
}

/// Le o arquivo de contas (TOML, um `[[conta]]` por conta) e confere tudo.
pub fn le_contas(texto: &str) -> Result<Vec<Conta>, ErroConta> {
    let arquivo: Arquivo = toml::from_str(texto).map_err(|e| ErroConta::Arquivo(e.to_string()))?;
    if arquivo.conta.is_empty() {
        return Err(ErroConta::Vazio);
    }
    let mut vistas: Vec<String> = Vec::new();
    for conta in &arquivo.conta {
        conta.valida()?;
        let id = conta.id();
        if vistas.contains(&id) {
            return Err(ErroConta::Repetida(id));
        }
        vistas.push(id);
    }
    Ok(arquivo.conta)
}

impl Conta {
    pub fn nova(usuario: &str, dominio: &str) -> Self {
        Self {
            usuario: usuario.to_string(),
            dominio: dominio.to_string(),
            porta: porta_padrao(),
            nome: None,
            usuario_auth: None,
            transporte: Transporte::Udp,
            registra: registra_padrao(),
            expira_s: expira_padrao(),
            sinal_de_vida_s: sinal_padrao(),
        }
    }

    /// 1001@pbx.exemplo.com.br: o que identifica a conta no aparelho.
    pub fn id(&self) -> String {
        format!("{}@{}", self.usuario, self.dominio)
    }

    pub fn valida(&self) -> Result<(), ErroConta> {
        if self.usuario.is_empty() {
            return Err(ErroConta::SemUsuario);
        }
        if !self.usuario.chars().all(caractere_de_usuario) {
            return Err(ErroConta::UsuarioInvalido(self.usuario.clone()));
        }
        if self.dominio.trim().is_empty() || self.dominio.contains(char::is_whitespace) {
            return Err(ErroConta::SemDominio(self.usuario.clone()));
        }
        if self.porta == 0 {
            return Err(ErroConta::PortaZero(self.id()));
        }
        intervalo(self, "expira_s", self.expira_s, 60, 3600)?;
        intervalo(self, "sinal_de_vida_s", self.sinal_de_vida_s, 5, 110)?;
        Ok(())
    }

    /// sip:1001@pbx.exemplo.com.br, com a porta so quando nao e a 5060.
    pub fn aor(&self) -> String {
        format!("sip:{}@{}", self.usuario, self.hospedeiro())
    }

    /// sip:pbx.exemplo.com.br: o registrador.
    pub fn servidor(&self) -> String {
        format!("sip:{}", self.hospedeiro())
    }

    /// O endereco de um ramal ou numero neste PBX. `#` vira %23: no SIP ele
    /// so vale escapado, e e assim que URA e servicos do PBX o recebem.
    pub fn uri_de(&self, destino: &str) -> Result<String, ErroConta> {
        let destino = destino.trim();
        if !ramal_valido(destino) {
            return Err(ErroConta::RamalInvalido(destino.to_string()));
        }
        Ok(format!(
            "sip:{}@{}",
            destino.replace('#', "%23"),
            self.hospedeiro()
        ))
    }

    fn hospedeiro(&self) -> String {
        if self.porta == porta_padrao() {
            self.dominio.clone()
        } else {
            format!("{}:{}", self.dominio, self.porta)
        }
    }
}

fn intervalo(
    conta: &Conta,
    campo: &'static str,
    valor: u32,
    min: u32,
    max: u32,
) -> Result<(), ErroConta> {
    if (min..=max).contains(&valor) {
        Ok(())
    } else {
        Err(ErroConta::ForaDoIntervalo {
            conta: conta.id(),
            campo,
            min,
            max,
        })
    }
}

/// Os caracteres que a parte do usuario aceita sem escapar (RFC 3261, 25.1).
fn caractere_de_usuario(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_.!~*'()&=+$,;?/".contains(c)
}

/// O que se digita para ligar ou mandar mensagem: ramal, numero (com * # +)
/// ou um usuario SIP.
pub fn ramal_valido(destino: &str) -> bool {
    !destino.is_empty() && destino.chars().all(|c| caractere_de_usuario(c) || c == '#')
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn grava_e_le_de_volta_igual() {
        let mut a = Conta::nova("1001", "pbx.exemplo");
        a.nome = Some("Jurandir \"TI\"".into());
        let mut b = Conta::nova("1001", "outro.exemplo");
        b.porta = 5070;
        b.registra = false;
        let texto = escreve_contas(&[a.clone(), b.clone()]).unwrap();
        assert!(texto.starts_with("# Contas do Hexagon Call"));
        assert!(
            !texto.contains("usuario_auth"),
            "None nao vai para o arquivo"
        );
        assert_eq!(le_contas(&texto).unwrap(), vec![a, b]);
    }

    const DUAS: &str = r#"
        [[conta]]
        usuario = "1001"
        dominio = "pbx.exemplo.com.br"
        nome = "Jurandir - TI"

        [[conta]]
        usuario = "7001"
        dominio = "10.0.0.5"
        porta = 5080
        registra = false
        sinal_de_vida_s = 15
    "#;

    #[test]
    fn le_duas_contas_com_os_padroes() {
        let contas = le_contas(DUAS).unwrap();
        assert_eq!(contas.len(), 2);
        assert_eq!(contas[0].porta, 5060);
        assert_eq!(contas[0].expira_s, 600);
        assert_eq!(contas[0].sinal_de_vida_s, 20);
        assert!(contas[0].registra);
        assert_eq!(contas[0].nome.as_deref(), Some("Jurandir - TI"));
        assert!(!contas[1].registra);
        assert_eq!(contas[1].sinal_de_vida_s, 15);
    }

    #[test]
    fn enderecos_com_e_sem_porta() {
        let contas = le_contas(DUAS).unwrap();
        assert_eq!(contas[0].aor(), "sip:1001@pbx.exemplo.com.br");
        assert_eq!(contas[0].servidor(), "sip:pbx.exemplo.com.br");
        assert_eq!(contas[1].aor(), "sip:7001@10.0.0.5:5080");
        assert_eq!(
            contas[0].uri_de("1002").unwrap(),
            "sip:1002@pbx.exemplo.com.br"
        );
    }

    #[test]
    fn sustenido_vai_escapado() {
        let conta = Conta::nova("1001", "pbx");
        assert_eq!(conta.uri_de("*72#").unwrap(), "sip:*72%23@pbx");
    }

    #[test]
    fn recusa_destino_com_espaco_ou_arroba() {
        let conta = Conta::nova("1001", "pbx");
        assert!(conta.uri_de("64 02").is_err());
        assert!(conta.uri_de("1002@outro").is_err());
        assert!(conta.uri_de("").is_err());
    }

    #[test]
    fn recusa_conta_repetida() {
        let texto = "[[conta]]\nusuario = \"1\"\ndominio = \"a\"\n[[conta]]\nusuario = \"1\"\ndominio = \"a\"\n";
        assert_eq!(le_contas(texto), Err(ErroConta::Repetida("1@a".into())));
    }

    #[test]
    fn recusa_campo_desconhecido() {
        let texto = "[[conta]]\nusuario = \"1\"\ndominio = \"a\"\nsenha = \"x\"\n";
        assert!(matches!(le_contas(texto), Err(ErroConta::Arquivo(_))));
    }

    #[test]
    fn recusa_arquivo_vazio() {
        assert_eq!(le_contas(""), Err(ErroConta::Vazio));
    }

    #[test]
    fn confere_os_intervalos() {
        let mut conta = Conta::nova("1001", "pbx");
        conta.sinal_de_vida_s = 120;
        assert!(matches!(
            conta.valida(),
            Err(ErroConta::ForaDoIntervalo {
                campo: "sinal_de_vida_s",
                ..
            })
        ));
        conta.sinal_de_vida_s = 20;
        conta.expira_s = 30;
        assert!(matches!(
            conta.valida(),
            Err(ErroConta::ForaDoIntervalo {
                campo: "expira_s",
                ..
            })
        ));
    }

    #[test]
    fn recusa_usuario_invalido() {
        let conta = Conta::nova("44 04", "pbx");
        assert_eq!(
            conta.valida(),
            Err(ErroConta::UsuarioInvalido("44 04".into()))
        );
    }
}
