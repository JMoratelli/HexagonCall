//! As gravacoes das ligacoes (preferencia "Gravar as ligacoes", pedido de
//! 27/11): a pasta, o nome de cada arquivo, a retencao e a leitura para a tela
//! ouvir. O arquivo em si, Opus com as duas vozes, e do `som::gravacao`.
//!
//! Ficam em `~/.local/share/hexagon-call/gravacoes` e, no Windows, em
//! `%LOCALAPPDATA%\hexagon-call\gravacoes`: a pasta local, para um perfil movel
//! do dominio nao levar os audios de maquina em maquina. O nome diz quando, o
//! sentido e o ramal: `2026-11-27_10-30-00_recebida_1002.opus`.
//!
//! A retencao apaga so o que tem esse formato de nome, direto na pasta: o que
//! mais alguem puser la fica.

use serde::Serialize;
use som::gravacao::{Etiquetas, Gravacao};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing::{info, warn};

const DIA: u64 = 86_400;
/// 18/11/2026: antes disto o app nao gravava. Um arquivo com data anterior foi
/// gravado com o relogio atrasado (bateria da placa-mae, sem sincronizar):
/// a retencao nunca o apaga.
const ANTES_DE_GRAVAR: u64 = 1_795_000_000;

pub struct Gravacoes {
    pasta: PathBuf,
}

/// O que as configuracoes mostram.
#[derive(Debug, Clone, Serialize)]
pub struct Resumo {
    pub pasta: String,
    pub quantidade: usize,
    pub bytes: u64,
}

struct Arquivo {
    caminho: PathBuf,
    idade: Duration,
    bytes: u64,
}

impl Gravacoes {
    pub fn abre() -> Result<Self, String> {
        let base = dirs::data_local_dir().ok_or("sem pasta de dados no sistema")?;
        Ok(Self {
            pasta: base.join("hexagon-call").join("gravacoes"),
        })
    }

    #[cfg(test)]
    pub fn em(pasta: &std::path::Path) -> Self {
        Self {
            pasta: pasta.to_path_buf(),
        }
    }

    /// Comeca o arquivo de uma ligacao atendida. `inicio` e o comeco dela (o
    /// mesmo `quando` do historico). Devolve o nome do arquivo e a gravacao.
    pub fn nova(
        &self,
        inicio: u64,
        entrada: bool,
        ramal: &str,
        nome: Option<&str>,
    ) -> Result<(String, Gravacao), String> {
        fs::create_dir_all(&self.pasta).map_err(|e| format!("{}: {e}", self.pasta.display()))?;
        let quando = i64::try_from(inicio)
            .ok()
            .and_then(|s| jiff::Timestamp::from_second(s).ok())
            .unwrap_or_else(jiff::Timestamp::now)
            .to_zoned_datetime(jiff::tz::TimeZone::system());
        let sentido = if entrada { "recebida" } else { "feita" };
        let quem = match nome {
            Some(n) if !n.is_empty() => format!("{n} ({ramal})"),
            _ => ramal.to_string(),
        };
        let etiquetas = Etiquetas {
            titulo: if entrada {
                format!("Ligação recebida de {quem}")
            } else {
                format!("Ligação feita para {quem}")
            },
            data: quando.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        };
        let base = format!(
            "{}_{sentido}_{}",
            quando.strftime("%Y-%m-%d_%H-%M-%S"),
            limpa_ramal(ramal)
        );
        // Duas no mesmo segundo com o mesmo ramal: -2, -3... (nunca por cima).
        for n in 1..=9 {
            let arquivo = if n == 1 {
                format!("{base}.opus")
            } else {
                format!("{base}-{n}.opus")
            };
            let caminho = self.pasta.join(&arquivo);
            // Sem seguir atalho: um atalho quebrado tambem ocupa o nome.
            if fs::symlink_metadata(&caminho).is_ok() {
                continue;
            }
            let gravacao = Gravacao::cria(&caminho, &etiquetas)?;
            return Ok((arquivo, gravacao));
        }
        Err(format!("já há gravações demais com o nome {base}"))
    }

    pub fn existe(&self, arquivo: &str) -> bool {
        e_gravacao(arquivo) && arquivo_comum(&self.pasta.join(arquivo))
    }

    /// As gravacoes da pasta (so arquivos comuns com o nosso nome, sem
    /// entrar em subpastas nem seguir atalhos).
    fn arquivos(&self, agora: SystemTime) -> Vec<Arquivo> {
        let Ok(entradas) = fs::read_dir(&self.pasta) else {
            return Vec::new();
        };
        entradas
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_str().is_some_and(e_gravacao))
            .filter_map(|e| {
                let meta = fs::symlink_metadata(e.path()).ok()?;
                if !meta.file_type().is_file() {
                    return None;
                }
                // Data no futuro (relogio adiantado quando gravou): idade zero.
                // Data de antes do app gravar (relogio atrasado): nunca vence.
                let idade = meta
                    .modified()
                    .ok()
                    .filter(|m| {
                        m.duration_since(std::time::UNIX_EPOCH)
                            .is_ok_and(|d| d.as_secs() >= ANTES_DE_GRAVAR)
                    })
                    .and_then(|m| agora.duration_since(m).ok())
                    .unwrap_or_default();
                Some(Arquivo {
                    caminho: e.path(),
                    idade,
                    bytes: meta.len(),
                })
            })
            .collect()
    }

    pub fn resumo(&self) -> Resumo {
        let arquivos = self.arquivos(SystemTime::now());
        Resumo {
            pasta: self.pasta.display().to_string(),
            quantidade: arquivos.len(),
            bytes: arquivos.iter().map(|a| a.bytes).sum(),
        }
    }

    /// Quantas a retencao de `dias` apagaria agora (0 dias: guarda sempre).
    pub fn mais_velhas(&self, dias: u32) -> usize {
        self.mais_velhas_em(dias, SystemTime::now())
    }

    fn mais_velhas_em(&self, dias: u32, agora: SystemTime) -> usize {
        self.arquivos(agora)
            .iter()
            .filter(|a| passou(a.idade, dias))
            .count()
    }

    /// Apaga as gravacoes com mais de `dias` (pela data do arquivo, que e a
    /// do fim da ligacao). 0 guarda sempre. Devolve quantas apagou.
    pub fn aplica_retencao(&self, dias: u32) -> usize {
        self.aplica_retencao_em(dias, SystemTime::now())
    }

    fn aplica_retencao_em(&self, dias: u32, agora: SystemTime) -> usize {
        let mut apagadas = 0;
        for a in self
            .arquivos(agora)
            .iter()
            .filter(|a| passou(a.idade, dias))
        {
            match fs::remove_file(&a.caminho) {
                Ok(()) => apagadas += 1,
                Err(e) => warn!(arquivo = %a.caminho.display(), "retencao: {e}"),
            }
        }
        if apagadas > 0 {
            info!(apagadas, dias, "gravacoes vencidas apagadas");
        }
        apagadas
    }

    /// O arquivo de uma gravacao, para a tela tocar. So um nome de gravacao,
    /// de dentro da pasta.
    pub fn le(&self, arquivo: &str) -> Result<Vec<u8>, String> {
        let caminho = self.pasta.join(arquivo);
        if !e_gravacao(arquivo) {
            return Err("isso não é uma gravação".into());
        }
        // So arquivo comum: um atalho levaria para fora da pasta, e um FIFO
        // prenderia a leitura para sempre.
        if fs::symlink_metadata(&caminho).is_ok_and(|m| !m.file_type().is_file()) {
            return Err("isso não é uma gravação".into());
        }
        fs::read(&caminho).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "a gravação não existe mais".into(),
            _ => e.to_string(),
        })
    }

    /// Abre a pasta no gerenciador de arquivos.
    pub fn mostra_pasta(&self) -> Result<(), String> {
        fs::create_dir_all(&self.pasta).map_err(|e| format!("{}: {e}", self.pasta.display()))?;
        sistema::mostra_pasta(&self.pasta)
    }
}

/// Um arquivo de verdade: nem atalho, nem pasta, nem FIFO.
fn arquivo_comum(caminho: &std::path::Path) -> bool {
    fs::symlink_metadata(caminho).is_ok_and(|m| m.file_type().is_file())
}

fn passou(idade: Duration, dias: u32) -> bool {
    dias > 0 && idade > Duration::from_secs(u64::from(dias) * DIA)
}

/// O que pode ir do ramal no nome do arquivo (nada de barra nem do que o
/// Windows recusa, como `*`).
fn caractere_do_ramal(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '.' | '-')
}

fn limpa_ramal(ramal: &str) -> String {
    let limpo: String = ramal
        .chars()
        .map(|c| if caractere_do_ramal(c) { c } else { '-' })
        .take(40)
        .collect();
    if limpo.is_empty() {
        "sem-numero".into()
    } else {
        limpo
    }
}

/// `AAAA-MM-DD_HH-MM-SS_<recebida|feita>_<ramal>.opus`, como `nova` faz.
fn e_gravacao(nome: &str) -> bool {
    let Some(base) = nome.strip_suffix(".opus") else {
        return false;
    };
    let mut partes = base.splitn(4, '_');
    let (Some(data), Some(hora), Some(sentido), Some(ramal)) =
        (partes.next(), partes.next(), partes.next(), partes.next())
    else {
        return false;
    };
    // "2026-11-27" e "10-30-00": tres numeros separados por traco, cada um
    // dentro do que uma data e uma hora podem ser.
    let campos = |t: &str, limites: [(u32, u32); 3]| {
        let partes: Vec<&str> = t.split('-').collect();
        partes.len() == 3
            && partes
                .iter()
                .zip(limites)
                .enumerate()
                .all(|(i, (p, (min, max)))| {
                    p.len() == if i == 0 && max > 99 { 4 } else { 2 }
                        && p.chars().all(|c| c.is_ascii_digit())
                        && p.parse::<u32>().is_ok_and(|n| (min..=max).contains(&n))
                })
    };
    campos(data, [(2000, 9999), (1, 12), (1, 31)])
        && campos(hora, [(0, 23), (0, 59), (0, 59)])
        && matches!(sentido, "recebida" | "feita")
        && !ramal.is_empty()
        && ramal.chars().all(caractere_do_ramal)
}

#[cfg(target_os = "linux")]
mod sistema {
    use std::path::Path;
    use std::process::Command;

    /// Pelo D-Bus (org.freedesktop.FileManager1, o Dolphin e o Nautilus): o
    /// gerenciador abre no ambiente dele, fora da montagem da AppImage. Sem
    /// ele, o xdg-open, sem as variaveis que a AppImage pos.
    pub fn mostra_pasta(pasta: &Path) -> Result<(), String> {
        let uri = uri_de(pasta);
        let pelo_dbus = zbus::blocking::Connection::session().and_then(|c| {
            c.call_method(
                Some("org.freedesktop.FileManager1"),
                "/org/freedesktop/FileManager1",
                Some("org.freedesktop.FileManager1"),
                "ShowFolders",
                &(vec![uri], ""),
            )
            .map(|_| ())
        });
        if pelo_dbus.is_ok() {
            return Ok(());
        }
        let appdir = std::env::var("APPDIR").unwrap_or_default();
        let mut comando = Command::new("xdg-open");
        comando.arg(pasta);
        for (chave, valor) in crate::atualizador::ambiente_sem_appdir(
            std::env::vars_os()
                .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))),
            &appdir,
        ) {
            match valor {
                Some(v) => comando.env(chave, v),
                None => comando.env_remove(chave),
            };
        }
        let mut filho = comando.spawn().map_err(|e| format!("xdg-open: {e}"))?;
        std::thread::spawn(move || filho.wait());
        Ok(())
    }

    /// `file://` com o caminho em percent-encoding (espaco, acento).
    pub fn uri_de(caminho: &Path) -> String {
        use std::os::unix::ffi::OsStrExt;
        let mut uri = String::from("file://");
        for b in caminho.as_os_str().as_bytes() {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(b) {
                uri.push(char::from(*b));
            } else {
                uri.push_str(&format!("%{b:02X}"));
            }
        }
        uri
    }
}

#[cfg(windows)]
mod sistema {
    use std::path::Path;
    use std::process::Command;

    pub fn mostra_pasta(pasta: &Path) -> Result<(), String> {
        let mut filho = Command::new("explorer.exe")
            .arg(pasta)
            .spawn()
            .map_err(|e| format!("explorer: {e}"))?;
        std::thread::spawn(move || filho.wait());
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod sistema {
    pub fn mostra_pasta(_pasta: &std::path::Path) -> Result<(), String> {
        Err("abrir a pasta não funciona neste sistema".into())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use motor::midia::Gravador;
    use std::path::Path;

    fn pasta(nome: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "hexagon-call-gravacoes-{nome}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// O "agora" dos testes da retencao: bem depois da data minima, para
    /// valerem em qualquer dia em que rodarem.
    fn agora() -> SystemTime {
        std::time::UNIX_EPOCH + Duration::from_secs(ANTES_DE_GRAVAR + 1000 * DIA)
    }

    fn envelhece(caminho: &Path, dias: u64) {
        let quando = agora() - Duration::from_secs(dias * DIA + 60);
        fs::File::options()
            .write(true)
            .open(caminho)
            .unwrap()
            .set_modified(quando)
            .unwrap();
    }

    #[test]
    fn reconhece_so_o_nome_que_ela_mesma_da() {
        assert!(e_gravacao("2026-11-27_10-30-00_recebida_1002.opus"));
        assert!(e_gravacao(
            "2026-11-27_10-30-00_feita_+5547999990000-2.opus"
        ));
        assert!(!e_gravacao("2026-11-27_10-30-00_feita_*43#.opus"));
        assert!(!e_gravacao("2026-11-27_10-30-00_recebida_1002.opus.bak"));
        assert!(!e_gravacao("2026-11-27_10-30-00_recebida_.opus"));
        assert!(!e_gravacao("2026-11-27_10-30-00_outra_1002.opus"));
        assert!(!e_gravacao("2026-11-27_10-30_recebida_1002.opus"));
        assert!(!e_gravacao("musica.opus"));
        assert!(!e_gravacao("2026-99-99_99-99-99_feita_1001.opus"));
        assert!(!e_gravacao("2026-11-27_24-00-00_feita_1001.opus"));
        assert!(!e_gravacao("26-11-27_10-30-00_feita_1001.opus"));
        assert!(!e_gravacao("2026-11-27_10-30-00_recebida_../x.opus"));
        assert!(!e_gravacao("2026-11-27_10-30-00_recebida_a/b.opus"));
        assert!(!e_gravacao("2026-11-27_10-30-00_recebida_a\\b.opus"));
        assert_eq!(limpa_ramal("*43"), "-43");
        assert_eq!(limpa_ramal("a/b\\c"), "a-b-c");
        assert_eq!(limpa_ramal(""), "sem-numero");
    }

    #[test]
    fn nova_da_o_nome_e_nao_sobrescreve() {
        let p = pasta("nova");
        let g = Gravacoes::em(&p);
        let (a, mut ga) = g.nova(1_790_000_000, true, "1002", Some("Fulano")).unwrap();
        let (b, mut gb) = g.nova(1_790_000_000, true, "1002", None).unwrap();
        assert!(e_gravacao(&a), "{a}");
        assert!(a.ends_with("_recebida_1002.opus"), "{a}");
        // O nome leva a hora local do comeco da ligacao (`quando` do historico).
        let local = jiff::Timestamp::from_second(1_790_000_000)
            .unwrap()
            .to_zoned_datetime(jiff::tz::TimeZone::system())
            .strftime("%Y-%m-%d_%H-%M-%S")
            .to_string();
        assert!(a.starts_with(&local), "{a} x {local}");
        assert!(b.ends_with("_recebida_1002-2.opus"), "{b}");
        let q = [0i16; motor::midia::AMOSTRAS];
        ga.quadro(&q, &q);
        ga.aguarda();
        gb.aguarda();
        assert!(g.existe(&a) && g.existe(&b));
        assert!(!g.existe("../contas.toml"));
        assert!(g.le(&a).unwrap().starts_with(b"OggS"));
        assert!(g.le("../contas.toml").is_err());
        #[cfg(unix)]
        {
            // Atalho com nome de gravacao: nem le, nem mostra o botao.
            let link = "2026-11-27_10-30-00_feita_1009.opus";
            std::os::unix::fs::symlink(p.join(&a), p.join(link)).unwrap();
            assert!(!g.existe(link));
            assert!(g.le(link).is_err());
        }
        assert_eq!(g.resumo().quantidade, 2);
        fs::remove_dir_all(&p).unwrap();
    }

    #[test]
    fn a_retencao_apaga_so_gravacoes_vencidas() {
        let p = pasta("retencao");
        let g = Gravacoes::em(&p);
        let velha = p.join("2026-01-01_08-00-00_feita_1001.opus");
        // Gravada com o relogio atrasado (data de antes do app gravar).
        let do_relogio = p.join("2000-01-01_00-00-00_feita_1005.opus");
        let nova = p.join("2026-11-26_08-00-00_recebida_1002.opus");
        let alheia = p.join("anotacoes.opus");
        // Uma pasta com nome de gravacao nao e gravacao.
        let sub = p.join("2026-01-01_08-00-00_feita_1003.opus");
        for f in [&velha, &nova, &alheia, &do_relogio] {
            fs::write(f, b"x").unwrap();
        }
        let em_2000 = std::time::UNIX_EPOCH + Duration::from_secs(946_684_800);
        fs::File::options()
            .write(true)
            .open(&do_relogio)
            .unwrap()
            .set_modified(em_2000)
            .unwrap();
        fs::create_dir_all(&sub).unwrap();
        envelhece(&velha, 40);
        envelhece(&alheia, 400);
        envelhece(&nova, 5);
        let fora = pasta("retencao-fora");
        #[cfg(unix)]
        {
            let quando = agora() - Duration::from_secs(400 * DIA);
            fs::File::open(&sub).unwrap().set_modified(quando).unwrap();
            // Um atalho com nome de gravacao para um arquivo de fora: nao segue.
            let importante = fora.join("importante.txt");
            fs::write(&importante, b"y").unwrap();
            envelhece(&importante, 400);
            std::os::unix::fs::symlink(&importante, p.join("2026-01-02_08-00-00_feita_1004.opus"))
                .unwrap();
        }

        assert_eq!(g.mais_velhas_em(0, agora()), 0, "0 guarda sempre");
        assert_eq!(g.aplica_retencao_em(0, agora()), 0);
        assert_eq!(g.mais_velhas_em(90, agora()), 0);
        assert_eq!(g.mais_velhas_em(30, agora()), 1);
        assert_eq!(g.mais_velhas_em(1, agora()), 2);
        assert_eq!(g.aplica_retencao_em(30, agora()), 1);
        assert!(!velha.exists());
        assert!(nova.exists() && alheia.exists() && sub.exists() && do_relogio.exists());
        #[cfg(unix)]
        assert!(fora.join("importante.txt").exists());
        assert_eq!(g.aplica_retencao_em(1, agora()), 1, "so a de 5 dias sobrou");
        assert!(alheia.exists() && sub.exists());
        fs::remove_dir_all(&p).unwrap();
        fs::remove_dir_all(&fora).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn o_endereco_da_pasta_para_o_dbus() {
        assert_eq!(
            sistema::uri_de(Path::new("/home/joão/Minhas gravações")),
            "file:///home/jo%C3%A3o/Minhas%20grava%C3%A7%C3%B5es"
        );
    }
}
