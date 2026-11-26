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
//!  2. No Windows o pacote e o instalador (HexagonCallSetup-X.Y.Z.exe, Inno
//!     Setup, pacote/instalador.iss), conferido pelo SHA256SUMS.txt anexado a
//!     mesma release antes de rodar. Sem hash publicado, nao se oferece.
//!  3. O download vai para a pasta temporaria do usuario. Ficam dois
//!     instaladores la (o novo e o anterior): o anterior e o ROLLBACK MANUAL,
//!     sem depender de rede nem de a release velha continuar publicada.
//!  4. O instalador roda silencioso (/VERYSILENT): a tela de espera e a dele,
//!     com a cara do app, e ele reabre o app no fim ([Run] do .iss).
//!
//! A diferenca para o Acessos esta em quem fecha o app: la e o Restart
//! Manager do instalador; aqui e o proprio app, logo depois de disparar o
//! instalador, para desregistrar as contas no PBX e tirar as notificacoes
//! antes de sair (o Restart Manager mataria o processo no meio). O
//! CloseApplications=force do .iss fica de rede de seguranca.
//!
//! No Linux a atualizacao vem com a AppImage (docs/estado.md); ate la,
//! `suportado()` e falso e nada e checado.

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

/// O prefixo e o sufixo do instalador nos anexos da release e no %TEMP%.
const PREFIXO_PACOTE: &str = "HexagonCallSetup-";
const SUFIXO_PACOTE: &str = ".exe";

/// O instalador novo e o anterior (rollback manual).
const PACOTES_MANTIDOS: usize = 2;

pub fn versao_atual() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Este pacote sabe agir na plataforma atual?
pub fn suportado() -> bool {
    cfg!(windows)
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
    if so != "windows" {
        return Ok(None);
    }
    let pacote = r
        .assets
        .iter()
        .find(|a| a.nome.starts_with(PREFIXO_PACOTE) && a.nome.ends_with(SUFIXO_PACOTE));
    let somas = r
        .assets
        .iter()
        .find(|a| a.nome.starts_with("SHA256SUMS") && a.nome.ends_with(".txt"));
    let (Some(pacote), Some(somas)) = (pacote, somas) else {
        return Ok(None);
    };
    // Sem hash publicado para este arquivo nao ha como conferir o instalador
    // antes de rodar: melhor nao oferecer do que baixar um .exe as cegas.
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

/// Baixa o instalador da release, confere o sha256 e dispara a instalacao
/// silenciosa. `progresso` recebe a fracao baixada (0 a 1).
///
/// Voltar sem erro quer dizer "o instalador esta rodando": quem chama
/// desregistra as contas e sai, e o instalador reabre o app no fim.
pub fn instala(rel: &Release, progresso: &(dyn Fn(f64) + Sync)) -> Result<(), String> {
    if !suportado() {
        return Err("a atualização automática ainda não existe neste sistema".into());
    }
    let pasta = std::env::temp_dir();
    let destino = pasta.join(nome_do_pacote(&rel.tag));
    // Baixa num nome provisorio e so poe no nome do pacote depois de
    // conferido: um download cortado ou adulterado nunca toma o lugar de um
    // pacote bom que ja estava ali (e nao conta na cota da limpeza).
    let baixando = pasta.join(format!("{}.baixando", nome_do_pacote(&rel.tag)));
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
    // Limpar AQUI: depois de disparar o instalador este processo sai, e
    // limpeza agendada para depois nunca rodaria.
    limpa_pacotes(&pasta, PACOTES_MANTIDOS, &destino);
    // /SUPPRESSMSGBOXES: nenhuma pergunta do Inno fica esperando alguem numa
    // instalacao que ninguem esta vendo.
    std::process::Command::new(&destino)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .spawn()
        .map_err(|e| format!("não abri o instalador: {e}"))?;
    Ok(())
}

/// HexagonCallSetup-<versao>.exe, sem o "v" da tag: e o nome que o Inno gera.
fn nome_do_pacote(tag: &str) -> String {
    let versao = tag.trim_start_matches(['v', 'V']);
    let limpa: String = versao
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    format!("{PREFIXO_PACOTE}{limpa}{SUFIXO_PACOTE}")
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

/// Apaga instaladores baixados antes, deixando os `manter` mais novos.
/// `preservar` conta na cota e nunca e apagado: e o que acabou de ser
/// conferido e vai rodar (data de arquivo e palpite). So mexe em
/// HexagonCallSetup-*.exe: a pasta e o %TEMP%, cheio de coisa dos outros.
/// Falha ao apagar e ignorada (faxina nao impede atualizacao). Devolve o que
/// apagou.
pub fn limpa_pacotes(pasta: &Path, manter: usize, preservar: &Path) -> Vec<String> {
    let Ok(entradas) = fs::read_dir(pasta) else {
        return Vec::new();
    };
    let mut pacotes: Vec<(PathBuf, SystemTime, String)> = entradas
        .filter_map(Result::ok)
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().to_string();
            let meta = e.metadata().ok()?;
            (meta.is_file() && nome.starts_with(PREFIXO_PACOTE) && nome.ends_with(SUFIXO_PACOTE))
                .then(|| {
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
    let base = preservar
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    // O preservado entra na cota ANTES da varredura: contado so quando
    // aparece, ele ficaria por ultimo quando a data dele e a mais velha, e
    // sobrariam manter + 1 (no Acessos e assim).
    let mut guardados = usize::from(pacotes.iter().any(|p| p.2 == base));
    let mut apagados = Vec::new();
    for (caminho, _, nome) in pacotes {
        if nome == base {
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

#[cfg(not(windows))]
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
        assert_eq!(nome_do_pacote("v0.2.0"), "HexagonCallSetup-0.2.0.exe");
        assert_eq!(
            nome_do_pacote("0.2.0-rc1"),
            "HexagonCallSetup-0.2.0-rc1.exe"
        );
        assert_eq!(
            nome_do_pacote("v1.0/../../x"),
            "HexagonCallSetup-1.0....x.exe"
        );
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
        let apagados = limpa_pacotes(&p, 2, &p.join("HexagonCallSetup-0.3.0.exe"));
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
        limpa_pacotes(&p, 2, &p.join("HexagonCallSetup-0.3.0.exe"));
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
        let apagados = limpa_pacotes(&p, 1, &p.join("HexagonCallSetup-0.2.0.exe"));
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
        assert!(limpa_pacotes(&p, 2, &p.join("HexagonCallSetup-0.2.0.exe")).is_empty());
        cria(&p, &["HexagonCallSetup-0.2.0.exe"]);
        assert!(limpa_pacotes(&p, 2, &p.join("HexagonCallSetup-0.2.0.exe")).is_empty());
        assert!(limpa_pacotes(&p.join("nao-existe"), 2, &p.join("x")).is_empty());
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
