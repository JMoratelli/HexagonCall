//! Checa se ha versao nova nas releases do GitHub e instala por cima,
//! reabrindo o app. O mesmo motor do Atalaia Acessos (JMoratelli/acessos,
//! internal/atualizador), com as mesmas decisoes de fundo:
//!
//!  1. A release marcada como "latest" NAO e o ponto de partida: uma versao
//!     pode sair so para uma plataforma, e a "latest" ficaria sem pacote para
//!     a outra, escondendo inclusive as anteriores que tinham. A checagem
//!     varre a LISTA de releases, ordena por versao (nao pela data da API) e
//!     fica com a mais nova que tenha pacote PARA ESTA plataforma. Rascunho e
//!     previa ficam de fora.
//!  2. O pacote de cada sistema e conferido pelo SHA256SUMS.txt anexado a
//!     mesma release antes de rodar. Sem hash publicado, nao se oferece. No
//!     Windows e o instalador (HexagonCallSetup-X.Y.Z.exe, Inno Setup,
//!     pacote/instalador.iss); no Linux, a AppImage
//!     (HexagonCall-X.Y.Z-x86_64.AppImage, pacote/linux.sh).
//!  3. O download vai para a pasta temporaria do usuario (no Linux, o cache
//!     do app). Ficam dois pacotes la (o novo e o anterior): o anterior e o
//!     ROLLBACK MANUAL, sem depender de rede nem de a release velha continuar
//!     publicada.
//!  4. No Windows o instalador roda silencioso (/VERYSILENT): a tela de
//!     espera e a dele, com a cara do app, e ele reabre o app no fim ([Run]
//!     do .iss). No Linux a AppImage nova toma o lugar do arquivo da que esta
//!     rodando, e um `sh` abre a nova quando este processo sair.
//!
//! A diferenca para o Acessos esta em quem fecha o app: la e o Restart
//! Manager do instalador; aqui e o proprio app, logo depois de disparar o
//! instalador, para desregistrar as contas no PBX e tirar as notificacoes
//! antes de sair (o Restart Manager mataria o processo no meio). O
//! CloseApplications=force do .iss fica de rede de seguranca.
//!
//! No Linux so a AppImage se atualiza sozinha: fora dela (cargo run, binario
//! solto) nao ha arquivo para trocar, e `suportado()` e falso.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::warn;

/// Onde estao as releases. `HEXAGON_CALL_RELEASES` troca o endereco (teste
/// de ponta a ponta com um servidor local): quem consegue mudar o ambiente
/// do usuario ja consegue trocar o executavel, entao nao abre porta nova.
const URL_RELEASES: &str =
    "https://api.github.com/repos/JMoratelli/HexagonCall/releases?per_page=30";

/// O anexo da release que um sistema instala: o nome e
/// `<prefixo><versao><sufixo>`, nos anexos e na pasta dos baixados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacote {
    prefixo: &'static str,
    sufixo: &'static str,
}

const WINDOWS: Pacote = Pacote {
    prefixo: "HexagonCallSetup-",
    sufixo: ".exe",
};

/// So x86_64: e a unica AppImage que o pacote gera.
const LINUX: Pacote = Pacote {
    prefixo: "HexagonCall-",
    sufixo: "-x86_64.AppImage",
};

impl Pacote {
    fn de(so: &str) -> Option<Self> {
        match so {
            "windows" => Some(WINDOWS),
            "linux" => Some(LINUX),
            _ => None,
        }
    }

    fn e_dele(self, nome: &str) -> bool {
        nome.len() > self.prefixo.len() + self.sufixo.len()
            && nome.starts_with(self.prefixo)
            && nome.ends_with(self.sufixo)
    }

    /// O nome do pacote de uma tag, sem o "v": e o nome que o pacote gera.
    fn nome(self, tag: &str) -> String {
        let versao = tag.trim_start_matches(['v', 'V']);
        let limpa: String = versao
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect();
        format!("{}{limpa}{}", self.prefixo, self.sufixo)
    }
}

/// O pacote novo e o anterior (rollback manual).
const PACOTES_MANTIDOS: usize = 2;

pub fn versao_atual() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Este pacote sabe agir na plataforma atual?
pub fn suportado() -> bool {
    cfg!(windows) || appimage().is_some()
}

/// A AppImage que esta rodando: o runtime dela poe o caminho do arquivo em
/// `APPIMAGE` e a montagem em `APPDIR`. Fora de uma AppImage nao ha o que
/// trocar.
pub fn appimage() -> Option<PathBuf> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return None;
    }
    // So vale com este binario dentro da montagem: aberto de um terminal de
    // outra AppImage (um editor), o APPIMAGE herdado e o arquivo dela, e o
    // atalho, o abrir com o sistema e a atualizacao iriam para o lugar errado.
    let appdir = PathBuf::from(std::env::var_os("APPDIR")?);
    let exe = std::env::current_exe().ok()?;
    if appdir.as_os_str().is_empty() || !exe.starts_with(&appdir) {
        return None;
    }
    let caminho = PathBuf::from(std::env::var_os("APPIMAGE")?);
    caminho.is_file().then_some(caminho)
}

fn url_releases() -> String {
    std::env::var("HEXAGON_CALL_RELEASES")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| URL_RELEASES.to_string())
}

/// O que a tela precisa mostrar e o que a instalacao precisa usar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub tag: String,
    /// Nome do anexo (HexagonCallSetup-X.Y.Z.exe).
    pub nome: String,
    pub url: String,
    pub sha256: String,
    pub notas: String,
}

/// Le o SHA256SUMS de uma release: (url do SHA256SUMS, nome do pacote) -> o
/// hash, quando publicado.
pub type LeSoma<'a> = &'a dyn Fn(&str, &str) -> Result<Option<String>, String>;

/// O recorte da API do GitHub que interessa.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseGitHub {
    #[serde(rename = "tag_name", default)]
    pub tag: String,
    #[serde(rename = "body", default)]
    pub corpo: Option<String>,
    #[serde(rename = "draft", default)]
    pub rascunho: bool,
    #[serde(rename = "prerelease", default)]
    pub previa: bool,
    #[serde(default)]
    pub assets: Vec<AnexoGitHub>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnexoGitHub {
    #[serde(rename = "name")]
    pub nome: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

/// Procura a versao mais nova que tenha pacote para esta plataforma. `None`:
/// nada a oferecer (nenhuma mais nova, ou nenhuma com o pacote daqui).
pub fn checa() -> Result<Option<Release>, String> {
    if !suportado() {
        return Ok(None);
    }
    let texto = rede::texto(&url_releases(), true)?;
    let lista: Vec<ReleaseGitHub> =
        serde_json::from_str(&texto).map_err(|e| format!("a lista de versões veio errada: {e}"))?;
    escolhe(
        &lista,
        versao_atual(),
        std::env::consts::OS,
        &|url, nome| rede::texto(url, false).map(|t| acha_soma(&t, nome)),
    )
}

/// Fica com a primeira release, da mais nova para a mais velha, que seja mais
/// nova que `atual` E tenha pacote para `so`. `soma_de` le o SHA256SUMS.txt
/// (separado para o teste nao precisar de rede).
///
/// Erro ao ler o SHA256SUMS de uma candidata nao para a varredura (a
/// seguinte ainda pode servir), mas e devolvido se no fim nada servir: falha
/// de rede nao pode passar por "esta tudo atualizado".
pub fn escolhe(
    lista: &[ReleaseGitHub],
    atual: &str,
    so: &str,
    soma_de: LeSoma<'_>,
) -> Result<Option<Release>, String> {
    let mut ordem: Vec<&ReleaseGitHub> = lista.iter().collect();
    ordem.sort_by(|a, b| compara(&b.tag, &a.tag));
    let mut primeiro_erro = None;
    for r in ordem {
        if r.tag.is_empty() || r.rascunho || r.previa {
            continue;
        }
        if !mais_nova(&r.tag, atual) {
            // Lista ordenada: daqui para baixo so vem coisa mais velha.
            break;
        }
        match monta(r, so, soma_de) {
            Ok(Some(rel)) => return Ok(Some(rel)),
            Ok(None) => {}
            Err(e) => {
                primeiro_erro.get_or_insert(e);
            }
        }
    }
    primeiro_erro.map_or(Ok(None), Err)
}

/// A release no que esta plataforma instala, ou `None` se ela nao trouxe
/// pacote para ca.
fn monta(r: &ReleaseGitHub, so: &str, soma_de: LeSoma<'_>) -> Result<Option<Release>, String> {
    let Some(daqui) = Pacote::de(so) else {
        return Ok(None);
    };
    let pacote = r.assets.iter().find(|a| daqui.e_dele(&a.nome));
    let somas = r
        .assets
        .iter()
        .find(|a| a.nome.starts_with("SHA256SUMS") && a.nome.ends_with(".txt"));
    let (Some(pacote), Some(somas)) = (pacote, somas) else {
        return Ok(None);
    };
    // Sem hash publicado para este arquivo nao ha como conferir o pacote
    // antes de rodar: melhor nao oferecer do que baixar um executavel as cegas.
    let Some(sha256) = soma_de(&somas.url, &pacote.nome)? else {
        return Ok(None);
    };
    Ok(Some(Release {
        tag: r.tag.clone(),
        nome: pacote.nome.clone(),
        url: pacote.url.clone(),
        sha256,
        notas: r.corpo.clone().unwrap_or_default(),
    }))
}

/// O hash de `nome` num SHA256SUMS (o formato do `sha256sum`: "hash  nome",
/// com "*" opcional antes do nome).
pub fn acha_soma(texto: &str, nome: &str) -> Option<String> {
    texto.lines().find_map(|linha| {
        let mut campos = linha.split_whitespace();
        let (Some(hash), Some(arquivo), None) = (campos.next(), campos.next(), campos.next())
        else {
            return None;
        };
        (arquivo.trim_start_matches('*') == nome && hash.len() == 64)
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Versoes tolerando "v" na frente e sufixo de pre-lancamento ("2.0.0-rc1"):
/// cada parte vale ate o primeiro caractere que nao e digito, entao um rc
/// vale como a versao que ele antecede e nunca e empurrado por cima dela.
pub fn mais_nova(remota: &str, local: &str) -> bool {
    compara(remota, local) == std::cmp::Ordering::Greater
}

fn compara(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (partes(a), partes(b));
    let n = a.len().max(b.len());
    for i in 0..n {
        let va = a.get(i).copied().unwrap_or(0);
        let vb = b.get(i).copied().unwrap_or(0);
        if va != vb {
            return va.cmp(&vb);
        }
    }
    std::cmp::Ordering::Equal
}

fn partes(v: &str) -> Vec<u64> {
    let v = v.trim().to_ascii_lowercase();
    v.trim_start_matches('v')
        .split('.')
        .map(|p| {
            let digitos: String = p.chars().take_while(char::is_ascii_digit).collect();
            digitos.parse().unwrap_or(0)
        })
        .collect()
}

/// Baixa o pacote da release, confere o sha256 e instala: no Windows
/// dispara o instalador silencioso; no Linux poe a AppImage nova no lugar da
/// que esta rodando. `progresso` recebe a fracao baixada (0 a 1).
///
/// `pode_trocar` e perguntado logo antes do passo sem volta (disparar o
/// instalador, trocar o arquivo): o download leva minutos, e uma ligacao
/// atendida nesse meio seria cortada pela saida do app.
///
/// Voltar sem erro quer dizer "a versao nova abre quando este processo
/// sair": quem chama desregistra as contas e sai.
pub fn instala(
    rel: &Release,
    progresso: &(dyn Fn(f64) + Sync),
    pode_trocar: &(dyn Fn() -> bool + Sync),
) -> Result<(), String> {
    let Some(pacote) = Pacote::de(std::env::consts::OS).filter(|_| suportado()) else {
        return Err("a atualização automática ainda não existe neste sistema".into());
    };
    // Antes do download: uma AppImage numa pasta sem escrita (do root) nao se
    // troca, e nao adianta baixar para descobrir no fim.
    sistema::confere_destino()?;
    let pasta = sistema::pasta_dos_pacotes()?;
    let destino = pasta.join(pacote.nome(&rel.tag));
    // Baixa num nome provisorio e so poe no nome do pacote depois de
    // conferido: um download cortado ou adulterado nunca toma o lugar de um
    // pacote bom que ja estava ali (e nao conta na cota da limpeza).
    let baixando = pasta.join(format!("{}.baixando", pacote.nome(&rel.tag)));
    let baixado = rede::baixa(&rel.url, &baixando, progresso)
        .map_err(|e| format!("não baixei a atualização: {e}"))
        .and_then(|()| confere_sha256(&baixando, &rel.sha256));
    if let Err(e) = baixado {
        let _ = fs::remove_file(&baixando);
        return Err(e);
    }
    fs::rename(&baixando, &destino).map_err(|e| {
        let _ = fs::remove_file(&baixando);
        format!("{}: {e}", destino.display())
    })?;
    // A versao que vai ser trocada fica tambem (no Windows o instalador dela
    // ja esta na pasta; no Linux a AppImage dela so existe no lugar que a
    // nova vai tomar).
    let atual = sistema::guarda_a_atual(&pasta, pacote);
    // Limpar AQUI: depois de instalar este processo sai, e limpeza agendada
    // para depois nunca rodaria. A que esta rodando tambem fica (aberta do
    // proprio cache, para voltar de versao).
    let rodando = appimage();
    let preservar: Vec<&Path> = [
        Some(destino.as_path()),
        atual.as_deref(),
        rodando.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    limpa_pacotes(&pasta, PACOTES_MANTIDOS, &preservar, pacote);
    if !pode_trocar() {
        return Err("chegou uma ligação; atualize quando ela acabar".into());
    }
    sistema::instala(&destino)
}

pub fn confere_sha256(caminho: &Path, esperado: &str) -> Result<(), String> {
    let mut f = fs::File::open(caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", caminho.display()))?;
        let Some(lido) = buf.get(..n) else { break };
        if lido.is_empty() {
            break;
        }
        h.update(lido);
    }
    let obtido: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if obtido.eq_ignore_ascii_case(esperado.trim()) {
        Ok(())
    } else {
        // Os hashes vao para o log; na tela, uma frase que caiba na faixa.
        warn!(arquivo = %caminho.display(), esperado, obtido, "sha256 nao confere");
        Err("o arquivo baixado não confere com o publicado (sha256), e nada foi instalado".into())
    }
}

/// Apaga pacotes baixados antes, deixando os `manter` mais novos.
/// Os de `preservar` contam na cota e nunca sao apagados: o que acabou de ser
/// conferido e vai rodar (data de arquivo e palpite), a copia da versao que
/// vai ser trocada e a AppImage que esta rodando, se for uma do cache. So
/// mexe nos pacotes do app (`pacote`): no Windows a pasta e o %TEMP%, cheio
/// de coisa dos outros. Falha ao apagar e ignorada (faxina nao impede
/// atualizacao). Devolve o que apagou.
pub fn limpa_pacotes(
    pasta: &Path,
    manter: usize,
    preservar: &[&Path],
    pacote: Pacote,
) -> Vec<String> {
    let Ok(entradas) = fs::read_dir(pasta) else {
        return Vec::new();
    };
    let mut pacotes: Vec<(PathBuf, SystemTime, String)> = entradas
        .filter_map(Result::ok)
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().to_string();
            let meta = e.metadata().ok()?;
            (meta.is_file() && pacote.e_dele(&nome)).then(|| {
                (
                    e.path(),
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    nome,
                )
            })
        })
        .collect();
    // Mais novo primeiro; o desempate pelo nome tira a ordem do diretorio da
    // decisao de quem e apagado.
    pacotes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
    // Comparados pelo caminho de verdade: o cache pode estar atras de um link.
    let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let preservados: Vec<PathBuf> = preservar.iter().map(|p| real(p)).collect();
    let e_preservado = |caminho: &Path| preservados.contains(&real(caminho));
    // Os preservados entram na cota ANTES da varredura: contados so quando
    // aparecem, ficariam por ultimo quando a data deles e a mais velha, e
    // sobrariam manter + 1 (no Acessos e assim).
    let mut guardados = pacotes.iter().filter(|p| e_preservado(&p.0)).count();
    let mut apagados = Vec::new();
    for (caminho, _, nome) in pacotes {
        if e_preservado(&caminho) {
            continue;
        }
        if guardados < manter {
            guardados += 1;
            continue;
        }
        if fs::remove_file(&caminho).is_ok() {
            apagados.push(nome);
        }
    }
    apagados
}

/// O ambiente para abrir a AppImage nova sem o que a velha deixou nele: o
/// AppRun poe caminhos de dentro da montagem dela (`appdir`, que some quando
/// este processo sai) em variaveis como `LD_LIBRARY_PATH`, `XDG_DATA_DIRS` e
/// `GSETTINGS_SCHEMA_DIR`, e o runtime poe `APPDIR`, `APPIMAGE`, `ARGV0` e
/// `OWD`. A nova poe os dela; um caminho velho que ela nao repusesse
/// apontaria para o nada. Devolve o que mudar (`None` tira a variavel).
#[cfg(any(target_os = "linux", test))]
pub fn ambiente_sem_appdir(
    vars: impl IntoIterator<Item = (String, String)>,
    appdir: &str,
) -> Vec<(String, Option<String>)> {
    const DO_RUNTIME: [&str; 4] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"];
    vars.into_iter()
        .filter_map(|(chave, valor)| {
            if DO_RUNTIME.contains(&chave.as_str()) {
                return Some((chave, None));
            }
            if appdir.is_empty() || !valor.contains(appdir) {
                return None;
            }
            // Lista de caminhos: fica o que nao e da montagem velha.
            let resto: Vec<&str> = valor
                .split(':')
                .filter(|p| !p.is_empty() && !p.contains(appdir))
                .collect();
            Some((chave, (!resto.is_empty()).then(|| resto.join(":"))))
        })
        .collect()
}

/// Onde o pacote baixado fica e como ele e instalado, em cada sistema.
#[cfg(windows)]
mod sistema {
    use std::path::{Path, PathBuf};

    /// O %TEMP% do usuario: o instalador novo e o anterior ficam la.
    pub fn pasta_dos_pacotes() -> Result<PathBuf, String> {
        Ok(std::env::temp_dir())
    }

    /// O instalador e do proprio usuario (sem administrador): nada a
    /// conferir antes.
    pub fn confere_destino() -> Result<(), String> {
        Ok(())
    }

    /// O instalador da versao atual, quando veio por atualizacao, ja esta na
    /// pasta; o da primeira instalacao nao se recria.
    pub fn guarda_a_atual(_pasta: &Path, _pacote: super::Pacote) -> Option<PathBuf> {
        None
    }

    /// Dispara o instalador silencioso; ele reabre o app no fim.
    pub fn instala(pacote: &Path) -> Result<(), String> {
        // /SUPPRESSMSGBOXES: nenhuma pergunta do Inno fica esperando alguem
        // numa instalacao que ninguem esta vendo.
        std::process::Command::new(pacote)
            .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
            .spawn()
            .map_err(|e| format!("não abri o instalador: {e}"))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod sistema {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use tracing::warn;

    /// O cache do app (~/.cache/hexagon-call): a AppImage nova e a anterior
    /// ficam la, como os instaladores no %TEMP% do Windows.
    pub fn pasta_dos_pacotes() -> Result<PathBuf, String> {
        let pasta = dirs::cache_dir()
            .ok_or("não achei a pasta de cache do usuário")?
            .join("hexagon-call");
        fs::create_dir_all(&pasta).map_err(|e| format!("{}: {e}", pasta.display()))?;
        Ok(pasta)
    }

    fn atual() -> Result<PathBuf, String> {
        super::appimage().ok_or_else(|| "o app não está rodando de uma AppImage".to_string())
    }

    /// O arquivo provisorio ao lado da AppImage: o mesmo disco, para a troca
    /// ser um rename (de uma vez so).
    fn provisorio(atual: &Path) -> PathBuf {
        let nome = atual
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        atual.with_file_name(format!(".{nome}.novo"))
    }

    /// Cria `caminho` do zero: tira o que houver ali antes (um link seria
    /// seguido, e o arquivo de outro lugar seria escrito) e abre exclusivo.
    fn cria_exclusivo(caminho: &Path) -> std::io::Result<fs::File> {
        let _ = fs::remove_file(caminho);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(caminho)
    }

    /// A pasta da AppImage aceita um arquivo novo? (Uma AppImage em /opt, do
    /// root, nao se troca sozinha.)
    pub fn confere_destino() -> Result<(), String> {
        let atual = atual()?;
        let teste = provisorio(&atual);
        cria_exclusivo(&teste)
            .and_then(|_| fs::remove_file(&teste))
            .map_err(|e| {
                let pasta = atual.parent().unwrap_or(&atual);
                format!("não posso trocar o app em {}: {e}", pasta.display())
            })
    }

    /// A AppImage da versao que esta rodando, no cache com o nome do pacote
    /// dela: o rename da troca apaga o unico lugar em que ela existia, e sem
    /// ela nao ha como voltar se a nova nao abrir nesta maquina. Um link no
    /// mesmo disco (sem copiar nada) ou uma copia. Falhar nao impede a
    /// atualizacao.
    pub fn guarda_a_atual(pasta: &Path, pacote: super::Pacote) -> Option<PathBuf> {
        let atual = super::appimage()?;
        let nome = pacote.nome(super::versao_atual());
        let copia = pasta.join(&nome);
        if copia.is_file() {
            return Some(copia);
        }
        let provisoria = pasta.join(format!("{nome}.guardando"));
        let _ = fs::remove_file(&provisoria);
        let feito = fs::hard_link(&atual, &provisoria)
            .or_else(|_| fs::copy(&atual, &provisoria).map(drop))
            .and_then(|()| fs::rename(&provisoria, &copia));
        match feito {
            Ok(()) => Some(copia),
            Err(e) => {
                let _ = fs::remove_file(&provisoria);
                warn!(arquivo = %copia.display(), "guardar a versao atual: {e}");
                None
            }
        }
    }

    /// Poe a AppImage conferida no lugar da que esta rodando e agenda a
    /// abertura da nova. A que roda continua de pe: o runtime dela segura o
    /// arquivo velho aberto ate sair.
    pub fn instala(pacote: &Path) -> Result<(), String> {
        let atual = atual()?;
        let novo = provisorio(&atual);
        let trocou = (|| -> std::io::Result<()> {
            {
                let mut f = cria_exclusivo(&novo)?;
                std::io::copy(&mut fs::File::open(pacote)?, &mut f)?;
                f.set_permissions(fs::Permissions::from_mode(0o755))?;
                f.sync_all()?;
            }
            fs::rename(&novo, &atual)
        })();
        if let Err(e) = trocou {
            let _ = fs::remove_file(&novo);
            return Err(format!("não troquei {}: {e}", atual.display()));
        }
        reabre(&atual)
    }

    /// Abre a AppImage nova so depois que este processo sair: antes, a
    /// instancia unica mandaria a nova embora, e o registro no PBX tem de sair
    /// antes de o novo entrar (o chan_sip guarda um contato por ramal). Um
    /// shell espera ate 30 s pelo fim deste processo e abre.
    ///
    /// O que vem herdado deste processo prende a montagem velha da AppImage
    /// (o runtime dela some so quando ninguem mais usa a montagem, e segura o
    /// arquivo velho, ja apagado, ate la): a pasta, que e a usr/ de dentro
    /// dela, e o fd 1023, que o runtime deixa aberto na montagem sem
    /// close-on-exec. Sem `unsafe` para fechar aqui, quem fecha e o shell:
    /// o bash fecha tudo acima de 2 (o dash so sabe fechar ate o 9; sem
    /// bash, vai o sh, e a montagem velha fica ate o app novo sair).
    ///
    /// Aberto como servico do systemd (o abrir com o sistema do GNOME e do
    /// KDE vira um app-...@autostart.service), o fim deste processo leva
    /// junto o resto do grupo dele num systemd sem `ExitType=cgroup` (ate o
    /// 249, o do Ubuntu 22.04), e o shell iria embora sem abrir a nova. Ai o
    /// shell vai num escopo proprio (`systemd-run --scope`), fora do grupo.
    /// Do 254 em diante o systemd-run expande `$VAR` no comando, como no
    /// ExecStart, e o script chegaria mutilado: vai com
    /// `--expand-environment=no`, que o 249 nao conhece (e la nao expande).
    fn reabre(appimage: &Path) -> Result<(), String> {
        const ESPERA: &str = r#"if [ -n "$BASH_VERSION" ]; then
    for f in /proc/$$/fd/*; do n=${f##*/}; [ "$n" -gt 2 ] && eval "exec $n<&-"; done
fi
i=0
while kill -0 "$1" 2>/dev/null && [ "$i" -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
exec "$2""#;
        const ESCOPO: [&str; 4] = ["--user", "--scope", "--quiet", "--collect"];
        let appdir = std::env::var("APPDIR").unwrap_or_default();
        let mudancas = super::ambiente_sem_appdir(
            std::env::vars_os()
                .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))),
            &appdir,
        );
        // Num servico o grupo deste processo termina em .service (aberto
        // pelo menu, e um escopo). O INVOCATION_ID nao serve: quem descende de
        // um servico herda.
        let num_servico = fs::read_to_string("/proc/self/cgroup")
            .is_ok_and(|g| g.lines().any(|l| l.trim_end().ends_with(".service")));
        let escopo = num_servico
            .then(|| {
                [Some("--expand-environment=no"), None]
                    .into_iter()
                    .find(|extra| {
                        Command::new("systemd-run")
                            .args(ESCOPO)
                            .args(*extra)
                            .args(["--", "true"])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                            .is_ok_and(|s| s.success())
                    })
            })
            .flatten();
        let mut erro = None;
        for shell in ["bash", "sh"] {
            let mut c = if let Some(extra) = escopo {
                let mut c = Command::new("systemd-run");
                c.args(ESCOPO).args(extra).arg("--").arg(shell);
                c
            } else {
                Command::new(shell)
            };
            c.arg("-c")
                .arg(ESPERA)
                .arg("hexagon-call-reabre")
                .arg(std::process::id().to_string())
                .arg(appimage)
                .current_dir("/")
                // A saida fica a mesma deste processo (o journal, quando
                // aberto pelo menu): o log da versao nova continua no mesmo
                // lugar.
                .stdin(Stdio::null())
                // Fora do grupo deste processo: um Ctrl+C no terminal que
                // abriu o app nao leva a volta junto.
                .process_group(0);
            for (chave, valor) in &mudancas {
                match valor {
                    Some(v) => c.env(chave, v),
                    None => c.env_remove(chave),
                };
            }
            match c.spawn() {
                Ok(_) => return Ok(()),
                Err(e) => erro = Some(e),
            }
        }
        Err(format!(
            "não agendei a abertura da versão nova: {}",
            erro.map(|e| e.to_string()).unwrap_or_default()
        ))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod sistema {
    use std::path::{Path, PathBuf};

    const NAO: &str = "sem atualização automática neste sistema";

    pub fn pasta_dos_pacotes() -> Result<PathBuf, String> {
        Err(NAO.into())
    }

    pub fn confere_destino() -> Result<(), String> {
        Err(NAO.into())
    }

    pub fn guarda_a_atual(_pasta: &Path, _pacote: super::Pacote) -> Option<PathBuf> {
        None
    }

    pub fn instala(_pacote: &Path) -> Result<(), String> {
        Err(NAO.into())
    }
}

/// A rede pela pilha do proprio Windows (Windows.Web.Http): usa o proxy e os
/// certificados do sistema e nao traz TLS de fora para dentro do binario.
#[cfg(windows)]
mod rede {
    use std::io::Write;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;
    use windows::Foundation::Uri;
    use windows::Storage::Streams::{DataReader, IBuffer};
    use windows::Web::Http::Filters::{
        HttpBaseProtocolFilter, HttpCacheReadBehavior, HttpCacheWriteBehavior,
    };
    use windows::Web::Http::{HttpClient, HttpMethod, HttpProgress, HttpRequestMessage};
    use windows::core::HSTRING;
    use windows_future::{AsyncOperationProgressHandler, IAsyncOperationWithProgress};

    const AGENTE: &str = "HexagonCall-Atualizador";

    fn cliente() -> windows::core::Result<HttpClient> {
        // Sem cache: a lista de versoes tem de vir do servidor sempre.
        let filtro = HttpBaseProtocolFilter::new()?;
        let cache = filtro.CacheControl()?;
        cache.SetReadBehavior(HttpCacheReadBehavior::NoCache)?;
        cache.SetWriteBehavior(HttpCacheWriteBehavior::NoCache)?;
        let c = HttpClient::Create(&filtro)?;
        c.DefaultRequestHeaders()?
            .UserAgent()?
            .TryParseAdd(&HSTRING::from(AGENTE))?;
        Ok(c)
    }

    fn erro(e: &windows::core::Error) -> String {
        let m = e.message();
        if m.is_empty() {
            format!("{:#010x}", e.code().0)
        } else {
            m.trim().to_string()
        }
    }

    /// GET de um texto. `api`: pede JSON da API do GitHub.
    pub fn texto(url: &str, api: bool) -> Result<String, String> {
        (|| -> windows::core::Result<Result<String, String>> {
            let c = cliente()?;
            let req = HttpRequestMessage::Create(
                &HttpMethod::Get()?,
                &Uri::CreateUri(&HSTRING::from(url))?,
            )?;
            if api {
                req.Headers()?.TryAppendWithoutValidation(
                    &HSTRING::from("Accept"),
                    &HSTRING::from("application/vnd.github+json"),
                )?;
            }
            let resp = c.SendRequestAsync(&req)?.join()?;
            if !resp.IsSuccessStatusCode()? {
                return Ok(Err(format!(
                    "o servidor respondeu {}",
                    resp.StatusCode()?.0
                )));
            }
            Ok(Ok(resp.Content()?.ReadAsStringAsync()?.join()?.to_string()))
        })()
        .map_err(|e| erro(&e))?
    }

    /// Baixa `url` para `destino`, avisando o progresso.
    pub fn baixa(
        url: &str,
        destino: &Path,
        progresso: &(dyn Fn(f64) + Sync),
    ) -> Result<(), String> {
        let bytes = (|| -> windows::core::Result<Vec<u8>> {
            let c = cliente()?;
            let op: IAsyncOperationWithProgress<IBuffer, HttpProgress> =
                c.GetBufferAsync(&Uri::CreateUri(&HSTRING::from(url))?)?;
            let (tx, rx) = std::sync::mpsc::channel::<f64>();
            op.SetProgress(
                &AsyncOperationProgressHandler::<IBuffer, HttpProgress>::new(move |_, p| {
                    if let Some(p) = p.as_ref() {
                        let total = p
                            .TotalBytesToReceive
                            .as_ref()
                            .and_then(|t| t.Value().ok())
                            .unwrap_or(0);
                        if total > 0 {
                            #[allow(clippy::cast_precision_loss)]
                            let fracao = p.BytesReceived as f64 / total as f64;
                            let _ = tx.send(fracao.min(1.0));
                        }
                    }
                    Ok(())
                }),
            )?;
            // O progresso chega numa thread do Windows; o callback de quem
            // chama roda numa thread daqui, enquanto esta espera o download.
            let acabou = AtomicBool::new(false);
            let buffer = std::thread::scope(|s| {
                let acabou = &acabou;
                s.spawn(move || {
                    loop {
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(f) => progresso(f),
                            Err(RecvTimeoutError::Timeout) if !acabou.load(Ordering::Acquire) => {}
                            Err(_) => break,
                        }
                    }
                });
                let r = op.join();
                acabou.store(true, Ordering::Release);
                r
            })?;
            let leitor = DataReader::FromBuffer(&buffer)?;
            let n = usize::try_from(leitor.UnconsumedBufferLength()?).unwrap_or(0);
            let mut bytes = vec![0u8; n];
            leitor.ReadBytes(&mut bytes)?;
            Ok(bytes)
        })()
        .map_err(|e| erro(&e))?;
        if bytes.is_empty() {
            return Err("o servidor mandou um arquivo vazio".into());
        }
        progresso(1.0);
        let mut f =
            std::fs::File::create(destino).map_err(|e| format!("{}: {e}", destino.display()))?;
        f.write_all(&bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("{}: {e}", destino.display()))
    }
}

/// No Linux, pelo ureq (Rust puro, so HTTP/1.1). Ate a 1.1.0 era o libsoup do
/// WebKitGTK, e o HTTP/2 do libsoup 3.0 que vai na AppImage (o do Ubuntu 22.04)
/// abortava o app ao falar com o GitHub, na checagem de 30 s depois de abrir.
#[cfg(target_os = "linux")]
mod rede {
    use std::io::{Read, Write};
    use std::path::Path;
    use std::time::Duration;

    const AGENTE: &str = "HexagonCall-Atualizador";
    /// A lista de versoes e o SHA256SUMS sao pequenos; mais que isto e erro.
    const TEXTO_MAXIMO: u64 = 8 * 1024 * 1024;

    /// `corpo`: o tempo para o corpo inteiro chegar (o pacote leva mais).
    fn agente(corpo: Duration) -> ureq::Agent {
        ureq::Agent::config_builder()
            .user_agent(AGENTE)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .timeout_recv_body(Some(corpo))
            .build()
            .into()
    }

    fn pede(
        url: &str,
        api: bool,
        corpo: Duration,
    ) -> Result<ureq::http::Response<ureq::Body>, String> {
        let mut pedido = agente(corpo)
            .get(url)
            // Sem cache: a lista de versoes tem de vir do servidor sempre.
            .header("Cache-Control", "no-cache");
        if api {
            pedido = pedido.header("Accept", "application/vnd.github+json");
        }
        let resposta = pedido.call().map_err(|e| e.to_string())?;
        let codigo = resposta.status().as_u16();
        if (200..300).contains(&codigo) {
            Ok(resposta)
        } else {
            Err(format!("o servidor respondeu {codigo}"))
        }
    }

    /// GET de um texto. `api`: pede JSON da API do GitHub.
    pub fn texto(url: &str, api: bool) -> Result<String, String> {
        let mut resposta = pede(url, api, Duration::from_secs(60))?;
        resposta
            .body_mut()
            .with_config()
            .limit(TEXTO_MAXIMO)
            .read_to_string()
            .map_err(|e| e.to_string())
    }

    /// Baixa `url` para `destino`, avisando o progresso. O ureq segue os
    /// redirecionamentos (o anexo da release vai para outro servidor).
    pub fn baixa(
        url: &str,
        destino: &Path,
        progresso: &(dyn Fn(f64) + Sync),
    ) -> Result<(), String> {
        let mut resposta = pede(url, false, Duration::from_secs(30 * 60))?;
        let total = resposta.body().content_length().unwrap_or(0);
        let local = |e: std::io::Error| format!("{}: {e}", destino.display());
        let mut f = std::fs::File::create(destino).map_err(local)?;
        let mut leitor = resposta.body_mut().as_reader();
        let mut buf = vec![0u8; 1 << 16];
        let mut lidos: u64 = 0;
        loop {
            let n = leitor.read(&mut buf).map_err(|e| e.to_string())?;
            let Some(pedaco) = buf.get(..n).filter(|p| !p.is_empty()) else {
                break;
            };
            f.write_all(pedaco).map_err(local)?;
            lidos += pedaco.len() as u64;
            if total > 0 {
                #[allow(clippy::cast_precision_loss)]
                progresso((lidos as f64 / total as f64).min(1.0));
            }
        }
        if lidos == 0 {
            return Err("o servidor mandou um arquivo vazio".into());
        }
        if total > 0 && lidos != total {
            return Err("o download parou no meio".into());
        }
        progresso(1.0);
        f.sync_all().map_err(local)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod rede {
    use std::path::Path;

    pub fn texto(_url: &str, _api: bool) -> Result<String, String> {
        Err("sem atualização automática neste sistema".into())
    }

    pub fn baixa(_url: &str, _destino: &Path, _p: &(dyn Fn(f64) + Sync)) -> Result<(), String> {
        Err("sem atualização automática neste sistema".into())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::cell::RefCell;

    fn rel(tag: &str, anexos: &[&str]) -> ReleaseGitHub {
        ReleaseGitHub {
            tag: tag.into(),
            corpo: Some(format!("notas da {tag}")),
            rascunho: false,
            previa: false,
            assets: anexos
                .iter()
                .map(|n| AnexoGitHub {
                    nome: (*n).into(),
                    url: format!("https://exemplo/{tag}/{n}"),
                })
                .collect(),
        }
    }

    fn windows(tag: &str) -> ReleaseGitHub {
        let v = tag.trim_start_matches('v');
        rel(
            tag,
            &[&format!("HexagonCallSetup-{v}.exe"), "SHA256SUMS.txt"],
        )
    }

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn sempre_hash(_url: &str, _nome: &str) -> Result<Option<String>, String> {
        Ok(Some(HASH.into()))
    }

    #[test]
    fn mais_nova_tolera_v_e_sufixo() {
        assert!(mais_nova("v0.2.0", "0.1.9"));
        assert!(mais_nova("0.10.0", "0.9.9"));
        assert!(mais_nova("1.0", "0.99.99"));
        assert!(!mais_nova("v0.1.0", "0.1.0"));
        assert!(!mais_nova("0.1.0", "0.1.1"));
        // Um rc vale como a versao que antecede: nunca por cima da final.
        assert!(!mais_nova("2.0.0-rc1", "2.0.0"));
        assert!(mais_nova("2.0.1-rc1", "2.0.0"));
        assert!(!mais_nova("0.1", "0.1.0"));
    }

    #[test]
    fn cai_na_anterior_quando_a_mais_nova_nao_tem_o_pacote_daqui() {
        let lista = vec![
            rel("v0.3.0", &["HexagonCall-0.3.0.AppImage"]),
            windows("v0.2.0"),
        ];
        let r = escolhe(&lista, "0.1.0", "windows", &sempre_hash)
            .unwrap()
            .unwrap();
        assert_eq!(r.tag, "v0.2.0");
        assert_eq!(r.nome, "HexagonCallSetup-0.2.0.exe");
        assert_eq!(r.sha256, HASH);
        assert_eq!(r.notas, "notas da v0.2.0");
    }

    #[test]
    fn ignora_rascunho_previa_e_o_que_nao_e_mais_novo() {
        let mut rascunho = windows("v0.4.0");
        rascunho.rascunho = true;
        let mut previa = windows("v0.3.0");
        previa.previa = true;
        let lista = vec![rascunho, previa, windows("v0.1.0"), windows("v0.0.9")];
        assert_eq!(
            escolhe(&lista, "0.1.0", "windows", &sempre_hash).unwrap(),
            None
        );
    }

    #[test]
    fn ordena_por_versao_e_nao_pela_ordem_da_api() {
        // A API ordena por data: uma correcao da linha velha publicada depois
        // vem primeiro.
        let lista = vec![windows("v0.1.5"), windows("v0.2.0"), windows("v0.1.4")];
        let r = escolhe(&lista, "0.1.0", "windows", &sempre_hash)
            .unwrap()
            .unwrap();
        assert_eq!(r.tag, "v0.2.0");
    }

    #[test]
    fn sem_pacote_desta_plataforma_nao_oferece_nada() {
        let lista = vec![windows("v0.2.0")];
        assert_eq!(
            escolhe(&lista, "0.1.0", "linux", &sempre_hash).unwrap(),
            None
        );
        let sem_somas = vec![rel("v0.2.0", &["HexagonCallSetup-0.2.0.exe"])];
        assert_eq!(
            escolhe(&sem_somas, "0.1.0", "windows", &sempre_hash).unwrap(),
            None
        );
    }

    #[test]
    fn pula_release_sem_hash_publicado_do_instalador() {
        let lista = vec![windows("v0.3.0"), windows("v0.2.0")];
        let r = escolhe(&lista, "0.1.0", "windows", &|url, _| {
            Ok(url.contains("v0.2.0").then(|| HASH.to_string()))
        })
        .unwrap()
        .unwrap();
        assert_eq!(r.tag, "v0.2.0");
    }

    #[test]
    fn falha_de_rede_nao_vira_esta_tudo_atualizado() {
        let lista = vec![windows("v0.3.0"), windows("v0.2.0")];
        let pedidos = RefCell::new(0);
        let r = escolhe(&lista, "0.1.0", "windows", &|_, _| {
            *pedidos.borrow_mut() += 1;
            Err("sem rede".to_string())
        });
        assert_eq!(r, Err("sem rede".to_string()));
        // Tentou as duas antes de desistir.
        assert_eq!(*pedidos.borrow(), 2);
        // Uma falha seguida de uma que serve: serve.
        let r = escolhe(&lista, "0.1.0", "windows", &|url, _| {
            if url.contains("v0.3.0") {
                Err("sem rede".to_string())
            } else {
                Ok(Some(HASH.to_string()))
            }
        });
        assert_eq!(r.unwrap().unwrap().tag, "v0.2.0");
    }

    #[test]
    fn le_a_lista_como_a_api_manda() {
        let json = r#"[
          {"tag_name": "v0.2.0", "body": null, "draft": false, "prerelease": false,
           "assets": [
             {"name": "HexagonCallSetup-0.2.0.exe", "browser_download_url": "https://github.com/x/HexagonCallSetup-0.2.0.exe", "size": 1},
             {"name": "SHA256SUMS.txt", "browser_download_url": "https://github.com/x/SHA256SUMS.txt"}
           ], "html_url": "https://github.com/x"},
          {"tag_name": "v0.1.0", "draft": false, "prerelease": false, "assets": []}
        ]"#;
        let lista: Vec<ReleaseGitHub> = serde_json::from_str(json).unwrap();
        let r = escolhe(&lista, "0.1.0", "windows", &sempre_hash)
            .unwrap()
            .unwrap();
        assert_eq!(r.url, "https://github.com/x/HexagonCallSetup-0.2.0.exe");
        assert_eq!(r.notas, "");
        // A versao instalada nao e oferecida de novo.
        assert_eq!(
            escolhe(&lista, "0.2.0", "windows", &sempre_hash).unwrap(),
            None
        );
    }

    #[test]
    fn acha_a_soma_no_formato_do_sha256sum() {
        let texto = format!(
            "{}  HexagonCall.exe\n{} *HexagonCallSetup-0.2.0.exe\nlixo\n",
            "f".repeat(64),
            HASH.to_uppercase()
        );
        assert_eq!(
            acha_soma(&texto, "HexagonCallSetup-0.2.0.exe").as_deref(),
            Some(HASH)
        );
        assert_eq!(acha_soma(&texto, "HexagonCall.exe"), Some("f".repeat(64)));
        assert_eq!(acha_soma(&texto, "outro.exe"), None);
        assert_eq!(
            acha_soma(
                "curto  HexagonCallSetup-0.2.0.exe",
                "HexagonCallSetup-0.2.0.exe"
            ),
            None
        );
    }

    #[test]
    fn nome_do_pacote_sem_o_v_e_sem_caminho() {
        assert_eq!(WINDOWS.nome("v0.2.0"), "HexagonCallSetup-0.2.0.exe");
        assert_eq!(WINDOWS.nome("0.2.0-rc1"), "HexagonCallSetup-0.2.0-rc1.exe");
        assert_eq!(
            WINDOWS.nome("v1.0/../../x"),
            "HexagonCallSetup-1.0....x.exe"
        );
        assert_eq!(LINUX.nome("v0.2.0"), "HexagonCall-0.2.0-x86_64.AppImage");
    }

    #[test]
    fn cada_sistema_reconhece_so_o_pacote_dele() {
        assert!(WINDOWS.e_dele("HexagonCallSetup-0.2.0.exe"));
        assert!(!WINDOWS.e_dele("HexagonCall.exe"));
        assert!(!WINDOWS.e_dele("HexagonCallSetup-.exe"));
        assert!(LINUX.e_dele("HexagonCall-0.2.0-x86_64.AppImage"));
        assert!(!LINUX.e_dele("HexagonCall-0.2.0-x86_64.AppImage.zsync"));
        assert!(!LINUX.e_dele("HexagonCallSetup-0.2.0.exe"));
        assert!(!LINUX.e_dele("HexagonCall-0.2.0-aarch64.AppImage"));
        assert_eq!(Pacote::de("macos"), None);
    }

    fn com_tudo(tag: &str) -> ReleaseGitHub {
        let v = tag.trim_start_matches('v');
        rel(
            tag,
            &[
                &format!("HexagonCallSetup-{v}.exe"),
                "HexagonCall.exe",
                &format!("HexagonCall-{v}-x86_64.AppImage"),
                &format!("HexagonCall-{v}-x86_64.AppImage.zsync"),
                "SHA256SUMS.txt",
            ],
        )
    }

    #[test]
    fn no_linux_fica_com_a_appimage() {
        let lista = vec![windows("v0.3.0"), com_tudo("v0.2.0")];
        let r = escolhe(&lista, "0.1.0", "linux", &sempre_hash)
            .unwrap()
            .unwrap();
        assert_eq!(r.tag, "v0.2.0");
        assert_eq!(r.nome, "HexagonCall-0.2.0-x86_64.AppImage");
        assert_eq!(
            r.url,
            "https://exemplo/v0.2.0/HexagonCall-0.2.0-x86_64.AppImage"
        );
        // A mesma release, vista do Windows: o instalador.
        let r = escolhe(&[com_tudo("v0.2.0")], "0.1.0", "windows", &sempre_hash)
            .unwrap()
            .unwrap();
        assert_eq!(r.nome, "HexagonCallSetup-0.2.0.exe");
    }

    #[test]
    fn limpa_no_cache_do_linux_so_as_appimages() {
        let p = pasta_de_teste("linux");
        cria(
            &p,
            &[
                "HexagonCall-0.1.0-x86_64.AppImage",
                "HexagonCall-0.2.0-x86_64.AppImage.baixando",
                "HexagonCall-0.2.0-x86_64.AppImage",
                "HexagonCall-0.3.0-x86_64.AppImage",
            ],
        );
        let apagados = limpa_pacotes(
            &p,
            2,
            &[&p.join("HexagonCall-0.3.0-x86_64.AppImage")],
            LINUX,
        );
        assert_eq!(apagados, vec!["HexagonCall-0.1.0-x86_64.AppImage"]);
    }

    #[test]
    fn limpa_nao_apaga_a_appimage_que_esta_rodando_do_cache() {
        // Voltou de versao abrindo a do cache: a 0.2.0 e a que roda, e a mais
        // velha das que estao la.
        let p = pasta_de_teste("rodando");
        cria(
            &p,
            &[
                "HexagonCall-0.2.0-x86_64.AppImage",
                "HexagonCall-0.3.0-x86_64.AppImage",
                "HexagonCall-0.4.0-x86_64.AppImage",
            ],
        );
        let novo = p.join("HexagonCall-0.4.0-x86_64.AppImage");
        let rodando = p.join("HexagonCall-0.2.0-x86_64.AppImage");
        let apagados = limpa_pacotes(&p, 2, &[&novo, &rodando], LINUX);
        assert_eq!(apagados, vec!["HexagonCall-0.3.0-x86_64.AppImage"]);
    }

    #[test]
    fn a_appimage_nova_nao_herda_os_caminhos_da_velha() {
        let appdir = "/tmp/.mount_HexagoAbC123";
        let vars = [
            ("APPDIR", appdir.to_string()),
            ("APPIMAGE", "/home/x/HexagonCall.AppImage".to_string()),
            ("ARGV0", "./HexagonCall.AppImage".to_string()),
            ("OWD", "/home/x".to_string()),
            ("HOME", "/home/x".to_string()),
            (
                "LD_LIBRARY_PATH",
                format!("{appdir}/usr/lib:/opt/minha/lib"),
            ),
            (
                "XDG_DATA_DIRS",
                format!("{appdir}/usr/share:/usr/local/share:/usr/share"),
            ),
            (
                "GSETTINGS_SCHEMA_DIR",
                format!("{appdir}/usr/share/glib-2.0/schemas"),
            ),
            ("PATH", "/usr/bin:/bin".to_string()),
        ]
        .map(|(k, v)| (k.to_string(), v));
        let mut mudou = ambiente_sem_appdir(vars.clone(), appdir);
        mudou.sort();
        let esperado: Vec<(String, Option<String>)> = vec![
            ("APPDIR", None),
            ("APPIMAGE", None),
            ("ARGV0", None),
            ("GSETTINGS_SCHEMA_DIR", None),
            ("LD_LIBRARY_PATH", Some("/opt/minha/lib")),
            ("OWD", None),
            ("XDG_DATA_DIRS", Some("/usr/local/share:/usr/share")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
        .collect();
        assert_eq!(mudou, esperado);
        // Sem APPDIR conhecido, so o que o runtime poe sai.
        assert_eq!(ambiente_sem_appdir(vars, "").len(), 4);
    }

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "hexagon-call-atualizador-{nome}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// Cria os arquivos com datas em ordem: o primeiro e o mais velho.
    fn cria(pasta: &Path, nomes: &[&str]) {
        let base = SystemTime::now() - std::time::Duration::from_secs(3600);
        for (i, n) in nomes.iter().enumerate() {
            let c = pasta.join(n);
            fs::write(&c, n).unwrap();
            let f = fs::File::options().write(true).open(&c).unwrap();
            f.set_modified(base + std::time::Duration::from_secs(60 * i as u64))
                .unwrap();
        }
    }

    fn sobrou(pasta: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(pasta)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn limpa_deixa_o_novo_e_um_anterior() {
        let p = pasta_de_teste("dois");
        cria(
            &p,
            &[
                "HexagonCallSetup-0.1.0.exe",
                "HexagonCallSetup-0.2.0.exe",
                "HexagonCallSetup-0.3.0.exe",
            ],
        );
        let apagados = limpa_pacotes(&p, 2, &[&p.join("HexagonCallSetup-0.3.0.exe")], WINDOWS);
        assert_eq!(apagados, vec!["HexagonCallSetup-0.1.0.exe"]);
        assert_eq!(
            sobrou(&p),
            vec!["HexagonCallSetup-0.2.0.exe", "HexagonCallSetup-0.3.0.exe"]
        );
    }

    #[test]
    fn limpa_nunca_apaga_o_que_vai_rodar() {
        // O que vai rodar e o mais velho pela data (copia, relogio que voltou).
        let p = pasta_de_teste("preserva");
        cria(
            &p,
            &[
                "HexagonCallSetup-0.3.0.exe",
                "HexagonCallSetup-0.1.0.exe",
                "HexagonCallSetup-0.2.0.exe",
            ],
        );
        limpa_pacotes(&p, 2, &[&p.join("HexagonCallSetup-0.3.0.exe")], WINDOWS);
        assert_eq!(
            sobrou(&p),
            vec!["HexagonCallSetup-0.2.0.exe", "HexagonCallSetup-0.3.0.exe"]
        );
    }

    #[test]
    fn limpa_so_mexe_nos_pacotes_do_app() {
        let p = pasta_de_teste("alheios");
        cria(
            &p,
            &[
                "AcessosSetup-1.0.exe",
                "HexagonCallSetup-0.1.0.exe",
                "HexagonCall.exe",
                "nota.txt",
                "HexagonCallSetup-0.2.0.exe",
            ],
        );
        let apagados = limpa_pacotes(&p, 1, &[&p.join("HexagonCallSetup-0.2.0.exe")], WINDOWS);
        assert_eq!(apagados, vec!["HexagonCallSetup-0.1.0.exe"]);
        assert_eq!(
            sobrou(&p),
            vec![
                "AcessosSetup-1.0.exe",
                "HexagonCall.exe",
                "HexagonCallSetup-0.2.0.exe",
                "nota.txt"
            ]
        );
    }

    #[test]
    fn limpa_com_pouco_ou_nada_nao_faz_nada() {
        let p = pasta_de_teste("pouco");
        assert!(limpa_pacotes(&p, 2, &[&p.join("HexagonCallSetup-0.2.0.exe")], WINDOWS).is_empty());
        cria(&p, &["HexagonCallSetup-0.2.0.exe"]);
        assert!(limpa_pacotes(&p, 2, &[&p.join("HexagonCallSetup-0.2.0.exe")], WINDOWS).is_empty());
        assert!(limpa_pacotes(&p.join("nao-existe"), 2, &[&p.join("x")], WINDOWS).is_empty());
    }

    /// Um servidor HTTP em 127.0.0.1 que da uma resposta pronta por conexao.
    #[cfg(target_os = "linux")]
    fn servidor(respostas: Vec<Vec<u8>>) -> String {
        use std::io::Write;
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endereco = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for resposta in respostas {
                let (mut s, _) = l.accept().unwrap();
                let mut pedido = Vec::new();
                let mut b = [0u8; 1024];
                while !pedido.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = s.read(&mut b).unwrap();
                    if n == 0 {
                        break;
                    }
                    pedido.extend_from_slice(&b[..n]);
                }
                let _ = s.write_all(&resposta);
            }
        });
        format!("http://{endereco}")
    }

    #[cfg(target_os = "linux")]
    fn resposta(status: &str, tamanho: usize, corpo: &[u8]) -> Vec<u8> {
        let mut r =
            format!("HTTP/1.1 {status}\r\nContent-Length: {tamanho}\r\nConnection: close\r\n\r\n")
                .into_bytes();
        r.extend_from_slice(corpo);
        r
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_rede_do_linux_pelo_libsoup() {
        let corpo: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let base = servidor(vec![
            resposta("200 OK", 2, b"[]"),
            resposta("200 OK", corpo.len(), &corpo),
            resposta("404 Not Found", 0, b""),
            resposta("200 OK", 1000, &[1u8; 500]),
        ]);
        assert_eq!(rede::texto(&format!("{base}/lista"), true).unwrap(), "[]");
        let p = pasta_de_teste("rede");
        let destino = p.join("baixado");
        let fracoes = std::sync::Mutex::new(Vec::new());
        rede::baixa(&format!("{base}/pacote"), &destino, &|f| {
            fracoes.lock().unwrap().push(f);
        })
        .unwrap();
        assert_eq!(fs::read(&destino).unwrap(), corpo);
        let f = fracoes.lock().unwrap().clone();
        assert_eq!(f.last(), Some(&1.0));
        assert!(f.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(
            rede::texto(&format!("{base}/sumiu"), false),
            Err("o servidor respondeu 404".to_string())
        );
        // A conexao cai antes do Content-Length: nao vale como baixado.
        assert!(rede::baixa(&format!("{base}/cortado"), &p.join("cortado"), &|_| {}).is_err());
    }

    /// Um proxy que comprime mesmo pedindo sem: o Content-Length e o do
    /// comprimido, e o que chega e o descomprimido.
    #[cfg(target_os = "linux")]
    #[test]
    fn download_comprimido_no_caminho_nao_vira_parou_no_meio() {
        const GZIP: [u8; 39] = [
            31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 51, 48, 52, 50, 54, 49, 53, 51, 183, 176, 52, 24,
            101, 141, 178, 70, 89, 195, 148, 5, 0, 241, 143, 133, 124, 232, 3, 0, 0,
        ];
        let mut r = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            GZIP.len()
        )
        .into_bytes();
        r.extend_from_slice(&GZIP);
        let base = servidor(vec![r]);
        let destino = pasta_de_teste("gzip").join("baixado");
        rede::baixa(&format!("{base}/pacote"), &destino, &|_| {}).unwrap();
        assert_eq!(fs::read(&destino).unwrap(), b"0123456789".repeat(100));
    }

    #[test]
    fn confere_o_sha256() {
        let p = pasta_de_teste("sha");
        let c = p.join("HexagonCallSetup-0.2.0.exe");
        fs::write(&c, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(confere_sha256(&c, abc).is_ok());
        assert!(confere_sha256(&c, &abc.to_uppercase()).is_ok());
        assert!(confere_sha256(&c, HASH).is_err());
    }
}
