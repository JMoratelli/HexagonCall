//! As ligacoes perdidas que ninguem viu ainda (pedido de 28/11): o ponto
//! vermelho no icone da bandeja, a quantidade na dica dele e o selo na barra de
//! tarefas (no Windows, o icone sobreposto; no Linux, o contador do Unity, que
//! o KDE mostra no gerenciador de tarefas). Zera quando a tela de Chamadas
//! aparece com a janela em foco (`Telefone::vendo_chamadas`).

use crate::guarda::Flavor;
use tauri::AppHandle;
use tauri::image::Image;
use tracing::debug;

/// O vermelho do ponto e a borda clara em volta, para ele se destacar do
/// icone vermelho do M Calls.
const VERMELHO: [u8; 3] = [0xe5, 0x39, 0x35];
const BORDA: [u8; 3] = [0xff, 0xff, 0xff];

/// O icone com o ponto no canto de cima, a direita.
pub fn com_ponto(base: &Image<'_>) -> Image<'static> {
    let (w, h) = (base.width(), base.height());
    let lado = f64::from(w.min(h));
    let raio = lado * 0.22;
    let centro = (f64::from(w) - raio - lado * 0.02, raio + lado * 0.02);
    let mut px = base.rgba().to_vec();
    desenha_ponto(&mut px, w, centro, raio, (lado * 0.045).max(1.0));
    Image::new_owned(px, w, h)
}

/// O ponto sozinho, ocupando um quadrado transparente (o icone sobreposto do
/// Windows, que ja fica no canto do icone do app).
#[cfg(any(windows, test))]
fn ponto_sozinho(lado: u32) -> Image<'static> {
    let mut px = vec![0; usize::try_from(lado * lado * 4).unwrap_or(0)];
    let meio = f64::from(lado) / 2.0;
    desenha_ponto(&mut px, lado, (meio, meio), meio - 0.5, 1.0);
    Image::new_owned(px, lado, lado)
}

/// Pinta um ponto vermelho com borda clara em `px` (RGBA, `w` de largura),
/// com a beirada antisserrilhada.
fn desenha_ponto(px: &mut [u8], w: u32, (cx, cy): (f64, f64), raio: f64, borda: f64) {
    let w = w.max(1);
    let (pixels, _) = px.as_chunks_mut::<4>();
    for (i, p) in pixels.iter_mut().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let (x, y) = (f64::from(i % w) + 0.5, f64::from(i / w) + 0.5);
        let d = (x - cx).hypot(y - cy);
        // Quanto do pixel cai dentro do ponto inteiro, e quanto so no miolo.
        let dentro = (raio + 0.5 - d).clamp(0.0, 1.0);
        if dentro <= 0.0 {
            continue;
        }
        let miolo = (raio - borda + 0.5 - d).clamp(0.0, 1.0);
        for (c, (v, b)) in p.iter_mut().zip(VERMELHO.iter().zip(BORDA)) {
            let cor = f64::from(*v) * miolo + f64::from(b) * (1.0 - miolo);
            *c = mistura(*c, cor, dentro);
        }
        p[3] = mistura(p[3], 255.0, dentro);
    }
}

/// De `de` ate `para`, na proporcao `t` (0 a 1), arredondado para um byte.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn mistura(de: u8, para: f64, t: f64) -> u8 {
    (f64::from(de) * (1.0 - t) + para * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// A dica da bandeja: o nome e, com perdidas, quantas.
pub fn dica(flavor: Flavor, n: u32) -> String {
    let nome = crate::titulo(flavor);
    match n {
        0 => nome.to_string(),
        1 => format!("{nome} · 1 ligação perdida"),
        n => format!("{nome} · {n} ligações perdidas"),
    }
}

/// Mostra `n` perdidas na bandeja e na barra de tarefas (0 tira).
pub fn mostra(app: &AppHandle, flavor: Flavor, n: u32) {
    if let Some(bandeja) = app.tray_by_id(crate::BANDEJA) {
        if let Some(base) = crate::icone_do_flavor(app, flavor) {
            let icone = if n > 0 { com_ponto(&base) } else { base };
            if let Err(e) = bandeja.set_icon(Some(icone)) {
                debug!("icone da bandeja: {e}");
            }
        }
        if let Err(e) = bandeja.set_tooltip(Some(dica(flavor, n))) {
            debug!("dica da bandeja: {e}");
        }
    }
    #[cfg(windows)]
    {
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("principal")
            && let Err(e) = w.set_overlay_icon((n > 0).then(|| ponto_sozinho(16)))
        {
            debug!("icone sobreposto na barra de tarefas: {e}");
        }
    }
    #[cfg(target_os = "linux")]
    contador_do_unity(n);
}

/// O contador do Unity (`com.canonical.Unity.LauncherEntry`), que o KDE e o
/// GNOME com Dash to Dock mostram no icone do app na barra de tarefas. Vale
/// para o atalho que o app poe no menu (`atalho.rs`). A conexao fica aberta
/// enquanto o app roda: o KDE tira o numero quando quem mandou sai do D-Bus.
#[cfg(target_os = "linux")]
fn contador_do_unity(n: u32) {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    use zbus::zvariant::Value;
    static CONEXAO: OnceLock<Option<zbus::blocking::Connection>> = OnceLock::new();
    let Some(conexao) = CONEXAO
        .get_or_init(|| {
            zbus::blocking::Connection::session()
                .map_err(|e| debug!("contador na barra de tarefas: {e}"))
                .ok()
        })
        .as_ref()
    else {
        return;
    };
    let props: HashMap<&str, Value<'_>> = HashMap::from([
        ("count", Value::I64(i64::from(n))),
        ("count-visible", Value::Bool(n > 0)),
    ]);
    let r = conexao.emit_signal(
        None::<&str>,
        "/org/jj/HexagonCall/launcher",
        "com.canonical.Unity.LauncherEntry",
        "Update",
        &(
            format!("application://{}.desktop", crate::atalho::ID),
            props,
        ),
    );
    if let Err(e) = r {
        debug!("contador na barra de tarefas: {e}");
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pixel(img: &Image<'_>, x: u32, y: u32) -> Vec<u8> {
        let i = usize::try_from((y * img.width() + x) * 4).unwrap_or(0);
        img.rgba()
            .get(i..i + 4)
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    }

    #[test]
    fn o_ponto_fica_no_canto_de_cima_a_direita() {
        let com = com_ponto(&Image::new_owned(vec![0; 64 * 64 * 4], 64, 64));
        // O miolo e vermelho e opaco; o canto oposto fica como estava.
        assert_eq!(pixel(&com, 64 - 15, 15), [0xe5, 0x39, 0x35, 255]);
        assert_eq!(pixel(&com, 5, 60), [0, 0, 0, 0]);
        // A borda clara separa o ponto do icone.
        let borda = pixel(&com, 64 - 15, 2);
        assert!(borda.get(1).is_some_and(|g| *g > 0x80), "{borda:?}");
    }

    #[test]
    fn o_ponto_sozinho_preenche_o_quadrado() {
        let p = ponto_sozinho(16);
        assert_eq!((p.width(), p.height()), (16, 16));
        assert_eq!(pixel(&p, 8, 8), [0xe5, 0x39, 0x35, 255]);
        assert_eq!(pixel(&p, 0, 0).get(3), Some(&0));
    }

    #[test]
    fn a_dica_diz_quantas() {
        assert_eq!(dica(Flavor::Mcalls, 0), "M Calls");
        assert_eq!(dica(Flavor::Mcalls, 1), "M Calls · 1 ligação perdida");
        assert_eq!(
            dica(Flavor::Hexagon, 40),
            "Hexagon Call · 40 ligações perdidas"
        );
    }
}
