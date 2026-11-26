//! O que fica no disco, em texto, no estilo do GOnnect: as contas num TOML
//! (`contas.toml`, o formato que o nucleo le), as preferencias em outro, e o
//! historico de ligacoes e as conversas em linhas de texto, uma por evento.
//! Tudo em ~/.config/hexagon-call (no Windows, %APPDATA%\hexagon-call).
//!
//! A senha vai para o chaveiro do sistema (KWallet ou GNOME Keyring pelo
//! Secret Service; Gerenciador de Credenciais no Windows), como o GOnnect
//! faz. Numa maquina sem chaveiro, ela vai para `senhas.toml`, que so o
//! proprio usuario le.

use nucleo::conta::{Conta, ErroConta, escreve_contas, le_contas};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

const SERVICO_DO_CHAVEIRO: &str = "hexagon-call";
/// Linhas do historico e das conversas que a janela carrega ao abrir.
const LINHAS_CARREGADAS: usize = 500;

pub struct Guarda {
    pasta: PathBuf,
}

/// Onde a senha ficou: a tela mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OndeASenha {
    Chaveiro,
    Arquivo,
}

/// O flavor: um binario so, e a personalizacao liga pelo arquivo de texto
/// (`flavor = "mcalls"` no preferencias.toml, decisao de 26/11), sem
/// compilar duas vezes. Sem nada, e o Hexagon Call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// O nome do produto: verde.
    #[default]
    Hexagon,
    /// O de uso interno: vermelho.
    Mcalls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tema {
    /// O que o sistema usa (claro ou escuro).
    #[default]
    Sistema,
    Claro,
    Escuro,
}

/// Os sons do app (sintetizados na tela): o toque de quem liga e o mesmo nos
/// dois; muda o de chamando, as teclas e os avisos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sons {
    #[default]
    Modernos,
    /// O tom de chamada brasileiro: 425 Hz, 1 s ligado e 4 s desligado.
    Classicos,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferencias {
    /// So pelo arquivo; a tela nao muda. `marca` era o nome de antes.
    #[serde(alias = "marca")]
    pub flavor: Flavor,
    pub tema: Tema,
    /// O id da lista de dispositivos; None e o padrao do sistema.
    pub microfone: Option<String>,
    pub alto_falante: Option<String>,
    /// Ligacao chegando traz a janela para a frente (a notificacao no canto
    /// aparece sempre; isto se desliga, pedido de 26/11).
    pub trazer_para_frente: bool,
    pub sons: Sons,
    /// O toque de quem liga (id de `som::toque::toques`).
    pub toque: String,
    /// De 0 a 100.
    pub volume_toque: u8,
    /// Tocar em todos os alto-falantes (opcional, 26/11).
    pub toque_em_todos: bool,
    /// Subir as saidas ao volume maximo enquanto toca (separado do "todos").
    pub toque_no_maximo: bool,
    /// Pausar musica e video ao atender, e voltar ao desligar.
    pub pausar_musica: bool,
}

impl Default for Preferencias {
    fn default() -> Self {
        Self {
            flavor: Flavor::default(),
            tema: Tema::default(),
            microfone: None,
            alto_falante: None,
            trazer_para_frente: true,
            sons: Sons::default(),
            toque: "colmeia".into(),
            volume_toque: 50,
            toque_em_todos: false,
            toque_no_maximo: false,
            pausar_musica: true,
        }
    }
}

/// Uma ligacao que acabou, como vai para o historico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemHistorico {
    /// Segundos desde 1970 (UTC); a tela mostra na hora local.
    pub quando: u64,
    pub conta: String,
    pub entrada: bool,
    pub ramal: String,
    pub nome: Option<String>,
    pub atendida: bool,
    pub duracao: u64,
    /// O motivo do fim, em texto ("Ocupado", "Ninguem atendeu").
    pub fim: String,
}

/// Uma mensagem, enviada ou recebida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemConversa {
    pub quando: u64,
    pub conta: String,
    pub ramal: String,
    pub recebida: bool,
    pub nome: Option<String>,
    pub texto: String,
}

pub fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Guarda {
    pub fn abre() -> Result<Self, String> {
        let base = dirs::config_dir().ok_or("sem pasta de configuração no sistema")?;
        let pasta = base.join("hexagon-call");
        fs::create_dir_all(&pasta).map_err(|e| format!("{}: {e}", pasta.display()))?;
        Ok(Self { pasta })
    }

    #[cfg(test)]
    pub fn em(pasta: &Path) -> Self {
        Self {
            pasta: pasta.to_path_buf(),
        }
    }

    pub fn pasta(&self) -> &Path {
        &self.pasta
    }

    fn arquivo(&self, nome: &str) -> PathBuf {
        self.pasta.join(nome)
    }

    // ---- Contas ----

    /// Sem arquivo (primeira vez) ou sem conta nenhuma: lista vazia.
    pub fn contas(&self) -> Result<Vec<Conta>, String> {
        let caminho = self.arquivo("contas.toml");
        let texto = match fs::read_to_string(&caminho) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{}: {e}", caminho.display())),
        };
        match le_contas(&texto) {
            Ok(c) => Ok(c),
            Err(ErroConta::Vazio) => Ok(Vec::new()),
            Err(e) => Err(format!("{}: {e}", caminho.display())),
        }
    }

    pub fn grava_contas(&self, contas: &[Conta]) -> Result<(), String> {
        let texto = escreve_contas(contas).map_err(|e| e.to_string())?;
        grava_inteiro(&self.arquivo("contas.toml"), &texto, false)
    }

    // ---- Preferencias ----

    pub fn preferencias(&self) -> Preferencias {
        fs::read_to_string(self.arquivo("preferencias.toml"))
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn grava_preferencias(&self, p: &Preferencias) -> Result<(), String> {
        let corpo = toml::to_string(p).map_err(|e| e.to_string())?;
        let texto = format!(
            "# Preferencias do Hexagon Call (gravadas pelo app).\n\
             # flavor = \"mcalls\" liga o M Calls (vermelho); \"hexagon\" e o padrao.\n\n{corpo}"
        );
        grava_inteiro(&self.arquivo("preferencias.toml"), &texto, false)
    }

    // ---- Senha ----

    pub fn tem_chaveiro() -> bool {
        keyring::Entry::store_status().is_ok()
    }

    pub fn senha(&self, conta: &str) -> Option<String> {
        if Self::tem_chaveiro()
            && let Ok(entrada) = keyring::Entry::new(SERVICO_DO_CHAVEIRO, conta)
        {
            match entrada.get_password() {
                Ok(s) => return Some(s),
                Err(e) => debug!(conta, "chaveiro sem a senha: {e}"),
            }
        }
        self.senhas_do_arquivo().remove(conta)
    }

    pub fn onde_a_senha(&self, conta: &str) -> Option<OndeASenha> {
        if Self::tem_chaveiro()
            && keyring::Entry::new(SERVICO_DO_CHAVEIRO, conta)
                .and_then(|e| e.get_password())
                .is_ok()
        {
            return Some(OndeASenha::Chaveiro);
        }
        self.senhas_do_arquivo()
            .contains_key(conta)
            .then_some(OndeASenha::Arquivo)
    }

    pub fn grava_senha(&self, conta: &str, senha: &str) -> Result<OndeASenha, String> {
        if Self::tem_chaveiro() {
            match keyring::Entry::new(SERVICO_DO_CHAVEIRO, conta)
                .and_then(|e| e.set_password(senha))
            {
                Ok(()) => {
                    // Se antes estava no arquivo, sai de la.
                    self.tira_do_arquivo(conta)?;
                    return Ok(OndeASenha::Chaveiro);
                }
                Err(e) => warn!(conta, "chaveiro recusou a senha, vai para o arquivo: {e}"),
            }
        }
        let mut senhas = self.senhas_do_arquivo();
        senhas.insert(conta.to_string(), senha.to_string());
        self.grava_senhas_no_arquivo(&senhas)?;
        Ok(OndeASenha::Arquivo)
    }

    pub fn apaga_senha(&self, conta: &str) -> Result<(), String> {
        if Self::tem_chaveiro()
            && let Ok(e) = keyring::Entry::new(SERVICO_DO_CHAVEIRO, conta)
        {
            let _ = e.delete_credential();
        }
        self.tira_do_arquivo(conta)
    }

    fn senhas_do_arquivo(&self) -> BTreeMap<String, String> {
        #[derive(Deserialize, Default)]
        struct Arquivo {
            #[serde(default)]
            senhas: BTreeMap<String, String>,
        }
        fs::read_to_string(self.arquivo("senhas.toml"))
            .ok()
            .and_then(|t| toml::from_str::<Arquivo>(&t).ok())
            .unwrap_or_default()
            .senhas
    }

    fn tira_do_arquivo(&self, conta: &str) -> Result<(), String> {
        let mut senhas = self.senhas_do_arquivo();
        if senhas.remove(conta).is_some() {
            self.grava_senhas_no_arquivo(&senhas)?;
        }
        Ok(())
    }

    fn grava_senhas_no_arquivo(&self, senhas: &BTreeMap<String, String>) -> Result<(), String> {
        #[derive(Serialize)]
        struct Arquivo<'a> {
            senhas: &'a BTreeMap<String, String>,
        }
        let corpo = toml::to_string(&Arquivo { senhas }).map_err(|e| e.to_string())?;
        let texto = format!(
            "# Senhas das contas: so existe numa maquina sem chaveiro do sistema.\n\
             # Leitura so para o seu usuario.\n\n{corpo}"
        );
        grava_inteiro(&self.arquivo("senhas.toml"), &texto, true)
    }

    // ---- Historico e conversas: uma linha por evento, campos por tabulacao ----

    pub fn historico(&self) -> Vec<ItemHistorico> {
        self.linhas("historico.txt")
            .filter_map(|c| {
                let [quando, conta, sentido, ramal, nome, atendida, duracao, fim] =
                    <[String; 8]>::try_from(c).ok()?;
                Some(ItemHistorico {
                    quando: quando.parse().ok()?,
                    conta,
                    entrada: sentido == "entrada",
                    ramal,
                    nome: Some(nome).filter(|n| !n.is_empty()),
                    atendida: atendida == "1",
                    duracao: duracao.parse().unwrap_or(0),
                    fim,
                })
            })
            .collect()
    }

    pub fn acrescenta_historico(&self, i: &ItemHistorico) -> Result<(), String> {
        let campos = [
            i.quando.to_string(),
            i.conta.clone(),
            if i.entrada { "entrada" } else { "saida" }.into(),
            i.ramal.clone(),
            i.nome.clone().unwrap_or_default(),
            if i.atendida { "1" } else { "0" }.into(),
            i.duracao.to_string(),
            i.fim.clone(),
        ];
        self.acrescenta(
            "historico.txt",
            "# quando\tconta\tsentido\tramal\tnome\tatendida\tduracao_s\tfim",
            &campos,
        )
    }

    pub fn conversas(&self) -> Vec<ItemConversa> {
        self.linhas("conversas.txt")
            .filter_map(|c| {
                let [quando, conta, ramal, sentido, nome, texto] =
                    <[String; 6]>::try_from(c).ok()?;
                Some(ItemConversa {
                    quando: quando.parse().ok()?,
                    conta,
                    ramal,
                    recebida: sentido == "recebida",
                    nome: Some(nome).filter(|n| !n.is_empty()),
                    texto,
                })
            })
            .collect()
    }

    pub fn acrescenta_conversa(&self, i: &ItemConversa) -> Result<(), String> {
        let campos = [
            i.quando.to_string(),
            i.conta.clone(),
            i.ramal.clone(),
            if i.recebida { "recebida" } else { "enviada" }.into(),
            i.nome.clone().unwrap_or_default(),
            i.texto.clone(),
        ];
        self.acrescenta(
            "conversas.txt",
            "# quando\tconta\tramal\tsentido\tnome\ttexto",
            &campos,
        )
    }

    /// As ultimas linhas do arquivo, ja separadas em campos.
    fn linhas(&self, nome: &str) -> impl Iterator<Item = Vec<String>> {
        let texto = fs::read_to_string(self.arquivo(nome)).unwrap_or_default();
        let linhas: Vec<Vec<String>> = texto
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| l.split('\t').map(desescapa).collect())
            .collect();
        let pular = linhas.len().saturating_sub(LINHAS_CARREGADAS);
        linhas.into_iter().skip(pular)
    }

    fn acrescenta(&self, nome: &str, cabecalho: &str, campos: &[String]) -> Result<(), String> {
        let caminho = self.arquivo(nome);
        let novo = !caminho.exists();
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&caminho)
            .map_err(|e| format!("{}: {e}", caminho.display()))?;
        let linha: Vec<String> = campos.iter().map(|c| escapa(c)).collect();
        let mut texto = String::new();
        if novo {
            texto.push_str(cabecalho);
            texto.push('\n');
        }
        texto.push_str(&linha.join("\t"));
        texto.push('\n');
        f.write_all(texto.as_bytes())
            .map_err(|e| format!("{}: {e}", caminho.display()))
    }
}

/// Grava num temporario e troca: o arquivo nunca fica pela metade.
fn grava_inteiro(caminho: &Path, texto: &str, so_do_usuario: bool) -> Result<(), String> {
    let temporario = caminho.with_extension("novo");
    let erro = |e: std::io::Error| format!("{}: {e}", caminho.display());
    {
        let mut opcoes = fs::OpenOptions::new();
        opcoes.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if so_do_usuario {
            use std::os::unix::fs::OpenOptionsExt;
            opcoes.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = so_do_usuario;
        let mut f = opcoes.open(&temporario).map_err(erro)?;
        f.write_all(texto.as_bytes()).map_err(erro)?;
        f.sync_all().map_err(erro)?;
    }
    fs::rename(&temporario, caminho).map_err(erro)
}

/// Tabulacao, quebra de linha e a barra viram `\t`, `\n` e `\\`: um campo
/// nunca quebra a linha nem a coluna.
fn escapa(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => saida.push_str("\\\\"),
            '\t' => saida.push_str("\\t"),
            '\n' => saida.push_str("\\n"),
            '\r' => {}
            c => saida.push(c),
        }
    }
    saida
}

fn desescapa(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            saida.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => saida.push('\t'),
            Some('n') => saida.push('\n'),
            Some(outro) => saida.push(outro),
            None => saida.push('\\'),
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("hexagon-call-teste-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn escapa_e_volta() {
        for s in [
            "simples",
            "com\ttab",
            "duas\nlinhas",
            "barra \\ e \\t literal",
            "",
        ] {
            assert_eq!(desescapa(&escapa(s)), s);
            assert!(!escapa(s).contains('\t') && !escapa(s).contains('\n'));
        }
    }

    #[test]
    fn contas_sem_arquivo_e_lista_vazia() {
        let g = Guarda::em(&pasta_de_teste("vazia"));
        assert_eq!(g.contas().unwrap(), Vec::new());
        g.grava_contas(&[]).unwrap();
        assert_eq!(g.contas().unwrap(), Vec::new());
    }

    #[test]
    fn historico_e_conversa_vao_e_voltam() {
        let g = Guarda::em(&pasta_de_teste("historico"));
        let h = ItemHistorico {
            quando: 1_790_000_000,
            conta: "1001@pbx".into(),
            entrada: true,
            ramal: "1002".into(),
            nome: Some("Fulano\tde Tal".into()),
            atendida: false,
            duracao: 0,
            fim: "Perdida".into(),
        };
        g.acrescenta_historico(&h).unwrap();
        g.acrescenta_historico(&h).unwrap();
        assert_eq!(g.historico(), vec![h.clone(), h]);
        let c = ItemConversa {
            quando: 1_790_000_001,
            conta: "1001@pbx".into(),
            ramal: "1002".into(),
            recebida: false,
            nome: None,
            texto: "duas\nlinhas e um \\ no meio".into(),
        };
        g.acrescenta_conversa(&c).unwrap();
        assert_eq!(g.conversas(), vec![c]);
        let texto = fs::read_to_string(g.pasta().join("conversas.txt")).unwrap();
        assert!(
            texto.starts_with("# quando"),
            "o cabecalho explica as colunas"
        );
    }

    #[test]
    fn senha_no_arquivo_e_so_do_usuario() {
        let g = Guarda::em(&pasta_de_teste("senha"));
        let mut senhas = BTreeMap::new();
        senhas.insert("1001@pbx".to_string(), "s3nh@ \"x\"".to_string());
        g.grava_senhas_no_arquivo(&senhas).unwrap();
        assert_eq!(g.senhas_do_arquivo(), senhas);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = fs::metadata(g.pasta().join("senhas.toml"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(modo & 0o777, 0o600);
        }
        g.tira_do_arquivo("1001@pbx").unwrap();
        assert!(g.senhas_do_arquivo().is_empty());
    }

    #[test]
    fn preferencias_ausentes_sao_o_padrao() {
        let g = Guarda::em(&pasta_de_teste("prefs"));
        assert_eq!(g.preferencias(), Preferencias::default());
        let p = Preferencias {
            flavor: Flavor::Mcalls,
            tema: Tema::Escuro,
            microfone: Some("alsa:default".into()),
            alto_falante: None,
            trazer_para_frente: false,
            sons: Sons::Classicos,
            toque: "marimba".into(),
            volume_toque: 80,
            toque_em_todos: true,
            toque_no_maximo: false,
            pausar_musica: false,
        };
        g.grava_preferencias(&p).unwrap();
        assert_eq!(g.preferencias(), p);
    }

    #[test]
    fn marca_de_antes_vira_flavor() {
        let g = Guarda::em(&pasta_de_teste("marca"));
        fs::write(
            g.pasta().join("preferencias.toml"),
            "marca = \"mcalls\"\nsons = \"modernos\"\n",
        )
        .unwrap();
        assert_eq!(g.preferencias().flavor, Flavor::Mcalls);
        fs::write(g.pasta().join("preferencias.toml"), "flavor = \"mcalls\"\n").unwrap();
        assert_eq!(g.preferencias().flavor, Flavor::Mcalls);
        fs::write(g.pasta().join("preferencias.toml"), "sons = \"modernos\"\n").unwrap();
        assert_eq!(g.preferencias().flavor, Flavor::Hexagon);
    }
}
