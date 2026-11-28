//! O atalho no menu de aplicativos quando o app roda de uma AppImage (Linux).
//! A AppImage e um arquivo solto: sem isto, so abre clicando nela. O atalho
//! aponta para o arquivo da AppImage, que a atualizacao troca no mesmo lugar
//! (atualizador.rs), entao ele vale de uma versao para a outra.
//!
//! Fica no padrao do freedesktop, na pasta do usuario:
//! `~/.local/share/applications/org.jj.HexagonCall.desktop` e o icone em
//! `~/.local/share/icons/hicolor/512x512/apps/org.jj.HexagonCall.png`. O nome e
//! o icone seguem o flavor. So grava o que mudou.

use crate::guarda::Flavor;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// O identificador do app (tauri.conf.json): o nome do atalho e do icone.
pub const ID: &str = "org.jj.HexagonCall";

/// Cria ou acerta o atalho. Fora de uma AppImage, nada (o app de
/// desenvolvimento nao entra no menu).
pub fn garante(flavor: Flavor) {
    let Some(appimage) = crate::atualizador::appimage() else {
        return;
    };
    // Uma quebra de linha no caminho quebraria o arquivo .desktop.
    if appimage.to_string_lossy().contains(['\n', '\r']) {
        warn!("atalho no menu: o caminho da AppImage tem quebra de linha");
        return;
    }
    let Some(dados) = dirs::data_dir() else {
        return;
    };
    // O app_id da janela no Wayland (e a classe no X11) e o nome do binario:
    // o StartupWMClass liga a janela aberta a este atalho na barra de tarefas.
    let classe = std::env::current_exe()
        .ok()
        .and_then(|c| c.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default();
    let icone = match flavor {
        Flavor::Hexagon => &include_bytes!("../icons/icon.png")[..],
        Flavor::Mcalls => &include_bytes!("../icons/m-calls.png")[..],
    };
    let png = dados.join(format!("icons/hicolor/512x512/apps/{ID}.png"));
    let aplicativos = dados.join("applications");
    let desktop = aplicativos.join(format!("{ID}.desktop"));
    // Quem gerencia AppImages (Gear Lever, AppImageLauncher) cria o atalho
    // dele para o mesmo arquivo: com o nosso, seriam dois no menu.
    if let Some(outro) = outro_atalho(&aplicativos, &desktop, &appimage) {
        info!(atalho = %outro.display(), "atalho no menu ja existe, de outro gerenciador");
        let _ = fs::remove_file(&desktop);
        return;
    }
    let texto = entrada(crate::titulo(flavor), &appimage, &classe);
    for (caminho, conteudo) in [(&png, icone), (&desktop, texto.as_bytes())] {
        if let Err(e) = grava_se_mudou(caminho, conteudo) {
            warn!("atalho no menu ({}): {e}", caminho.display());
            return;
        }
    }
}

/// O `.desktop`. O caminho vai entre aspas no Exec (pode ter espaco), com as
/// barras, aspas, crase e cifrao escapados e o `%` dobrado, como a
/// especificacao pede.
fn entrada(nome: &str, appimage: &Path, classe: &str) -> String {
    let mut exec = String::new();
    for c in appimage.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            exec.push('\\');
        }
        if c == '%' {
            exec.push('%');
        }
        exec.push(c);
    }
    // No arquivo .desktop a barra invertida tambem e escape: dobra de novo.
    let exec = exec.replace('\\', "\\\\");
    let mut t = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={nome}\n\
         Comment=Telefone SIP: ligações e mensagens pelo ramal\n\
         Exec=\"{exec}\"\n\
         Icon={ID}\n\
         Terminal=false\n\
         Categories=Network;Telephony;\n\
         Keywords=telefone;ramal;sip;voip;ligação;\n"
    );
    if !classe.is_empty() {
        t.push_str(&format!("StartupWMClass={classe}\n"));
    }
    t
}

/// Um `.desktop` que nao e o nosso e abre esta AppImage: o comando do Exec e
/// o caminho dela, com ou sem aspas (outro arquivo que so comeca igual, como
/// um .bak, nao conta).
fn outro_atalho(pasta: &Path, nosso: &Path, appimage: &Path) -> Option<std::path::PathBuf> {
    let caminho = appimage.to_string_lossy();
    let abre_esta = |linha: &str| {
        let Some(exec) = linha.strip_prefix("Exec=") else {
            return false;
        };
        let (resto, fecho) = match exec.trim_start().strip_prefix('"') {
            Some(r) => (r, Some('"')),
            None => (exec.trim_start(), None),
        };
        resto.strip_prefix(caminho.as_ref()).is_some_and(|depois| {
            let prox = depois.chars().next();
            match fecho {
                Some(aspas) => prox == Some(aspas),
                None => prox.is_none_or(char::is_whitespace),
            }
        })
    };
    fs::read_dir(pasta)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p != nosso && p.extension().is_some_and(|x| x == "desktop"))
        .find(|p| fs::read_to_string(p).is_ok_and(|t| t.lines().any(abre_esta)))
}

fn grava_se_mudou(caminho: &Path, conteudo: &[u8]) -> std::io::Result<()> {
    if fs::read(caminho).is_ok_and(|atual| atual == conteudo) {
        return Ok(());
    }
    if let Some(pasta) = caminho.parent() {
        fs::create_dir_all(pasta)?;
    }
    // Grava ao lado e troca: o menu nunca le um arquivo pela metade.
    let novo = caminho.with_extension("novo");
    fs::write(&novo, conteudo)?;
    fs::rename(&novo, caminho)?;
    info!(arquivo = %caminho.display(), "atalho no menu gravado");
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn exec_entre_aspas_e_escapado() {
        let t = entrada(
            "M Calls",
            Path::new("/home/x/Meus Apps/Hexagon$Call.AppImage"),
            "hexagon-call",
        );
        assert!(t.contains("Name=M Calls\n"));
        assert!(t.contains("Exec=\"/home/x/Meus Apps/Hexagon\\\\$Call.AppImage\"\n"));
        assert!(t.contains("Icon=org.jj.HexagonCall\n"));
        assert!(t.contains("StartupWMClass=hexagon-call\n"));
        let pct = entrada("x", Path::new("/home/x/100%/app.AppImage"), "");
        assert!(pct.contains("Exec=\"/home/x/100%%/app.AppImage\"\n"));
        assert!(!entrada("x", Path::new("/a"), "").contains("StartupWMClass"));
    }

    #[test]
    fn acha_o_atalho_de_outro_gerenciador() {
        let p = std::env::temp_dir().join(format!("hexagon-call-outro-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        let nosso = p.join("org.jj.HexagonCall.desktop");
        let app = Path::new("/home/x/AppImages/hexagoncall.appimage");
        fs::write(&nosso, "Exec=\"/home/x/AppImages/hexagoncall.appimage\"\n").unwrap();
        fs::write(p.join("outro.desktop"), "Exec=/usr/bin/outra-coisa\n").unwrap();
        assert_eq!(outro_atalho(&p, &nosso, app), None);
        let gear = p.join("gearlever_hexagoncall_1a2b3c.desktop");
        fs::write(
            &gear,
            "[Desktop Entry]\nExec=/home/x/AppImages/hexagoncall.appimage %U\n",
        )
        .unwrap();
        assert_eq!(outro_atalho(&p, &nosso, app), Some(gear.clone()));
        // Com aspas tambem; um arquivo que so comeca igual nao conta.
        fs::remove_file(&gear).unwrap();
        fs::write(
            p.join("bak.desktop"),
            "Exec=/home/x/AppImages/hexagoncall.appimage.bak\n",
        )
        .unwrap();
        assert_eq!(outro_atalho(&p, &nosso, app), None);
        let aspas = p.join("appimagelauncher.desktop");
        fs::write(
            &aspas,
            "Exec=\"/home/x/AppImages/hexagoncall.appimage\" %U\n",
        )
        .unwrap();
        assert_eq!(outro_atalho(&p, &nosso, app), Some(aspas));
        assert_eq!(outro_atalho(&p.join("nao-existe"), &nosso, app), None);
    }

    #[test]
    fn grava_so_o_que_mudou() {
        let p = std::env::temp_dir().join(format!("hexagon-call-atalho-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        let c = p.join("applications/x.desktop");
        grava_se_mudou(&c, b"um").unwrap();
        assert_eq!(fs::read(&c).unwrap(), b"um");
        let antes = fs::metadata(&c).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        grava_se_mudou(&c, b"um").unwrap();
        assert_eq!(fs::metadata(&c).unwrap().modified().unwrap(), antes);
        grava_se_mudou(&c, b"dois").unwrap();
        assert_eq!(fs::read(&c).unwrap(), b"dois");
        assert!(!p.join("applications/x.novo").exists());
    }
}
