//! O AppUserModelID do app no Windows: e por ele que o toast sai com o nome e
//! o icone do app. Sem ele, quem mostra e o PowerShell ("Windows PowerShell"
//! no cabecalho, com o icone dele).
//!
//! Dois caminhos levam ao mesmo lugar: o instalador (pacote/instalador.iss)
//! poe o AUMID no atalho do Menu Iniciar e grava a mesma chave abaixo; o app,
//! ao abrir, grava a chave de novo. Assim o toast sai certo tambem no exe
//! solto (sem instalador) e no flavor M Calls, que tem nome e icone proprios.
//! A chave fica no HKCU: nao pede administrador.

use crate::guarda::Flavor;
use std::fs;
use std::path::PathBuf;

/// O mesmo do atalho do instalador: mudar aqui e mudar la.
pub const AUMID_HEXAGON: &str = "org.jj.HexagonCall";
pub const AUMID_MCALLS: &str = "org.jj.HexagonCall.MCalls";

pub struct Registro {
    pub aumid: String,
    pub icone: PathBuf,
}

pub fn de(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Hexagon => AUMID_HEXAGON,
        Flavor::Mcalls => AUMID_MCALLS,
    }
}

/// Grava o icone em PNG em %LOCALAPPDATA%\hexagon-call e o nome e o icone em
/// HKCU\Software\Classes\AppUserModelId\<aumid>.
pub fn registra(flavor: Flavor) -> Result<Registro, String> {
    let aumid = de(flavor);
    let png: &[u8] = match flavor {
        Flavor::Hexagon => include_bytes!("../icons/icon.png"),
        Flavor::Mcalls => include_bytes!("../icons/m-calls.png"),
    };
    let pasta = dirs::data_local_dir()
        .ok_or("sem a pasta local do usuário")?
        .join("hexagon-call");
    fs::create_dir_all(&pasta).map_err(|e| format!("{}: {e}", pasta.display()))?;
    let icone = pasta.join(format!("{aumid}.png"));
    if fs::read(&icone).ok().as_deref() != Some(png) {
        fs::write(&icone, png).map_err(|e| format!("{}: {e}", icone.display()))?;
    }
    let chave = windows_registry::CURRENT_USER
        .create(format!(r"Software\Classes\AppUserModelId\{aumid}"))
        .map_err(|e| format!("registro do AUMID: {e}"))?;
    chave
        .set_string("DisplayName", crate::titulo(flavor))
        .and_then(|()| chave.set_string("IconUri", icone.to_string_lossy()))
        .map_err(|e| format!("registro do AUMID: {e}"))?;
    Ok(Registro {
        aumid: aumid.to_string(),
        icone,
    })
}
