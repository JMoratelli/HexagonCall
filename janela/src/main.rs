//! Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
//! Licenca GNU GPL 3 ou posterior.
//!
//! A janela do softphone (Tauri 2, no molde da Usina do LinuxMachines: a tela
//! em HTML puro em `ui/`, sem bundler). Fechar a janela esconde: o app fica na
//! bandeja recebendo ligacao (decisao de 26/11); "Sair", no menu da bandeja,
//! desregistra as contas e encerra. Abrir de novo traz a janela que ja existe.
//!
//!   hexagon-call            abre a janela
//!   hexagon-call --bandeja  so na bandeja (para abrir com o sistema)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod guarda;
mod notificacao;
mod telefone;

use guarda::{Guarda, Marca, Preferencias};
use serde::Serialize;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};
use telefone::{Estado, FormConta, Telefone};
use tracing::warn;

type Tel<'a> = State<'a, Arc<Telefone>>;

/// Mostra a janela (bandeja, segunda abertura, ligacao chegando).
pub fn mostra(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("principal") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn titulo(marca: Marca) -> &'static str {
    match marca {
        Marca::Mcalls => "M Calls",
        Marca::Hexagon => "Hexagon Call",
    }
}

// ---- Comandos da tela ----

#[tauri::command]
fn estado(t: Tel<'_>) -> Estado {
    t.estado()
}

#[tauri::command]
async fn liga(t: Tel<'_>, destino: String, conta: Option<String>) -> Result<String, String> {
    t.liga(destino.trim(), conta.as_deref()).await
}

#[tauri::command]
async fn atende(t: Tel<'_>, id: String) -> Result<(), String> {
    t.atende(&id).await
}

#[tauri::command]
async fn desliga(t: Tel<'_>, id: String) -> Result<(), String> {
    t.desliga(&id).await
}

#[tauri::command]
fn mudo(t: Tel<'_>, id: String, sim: bool) -> Result<(), String> {
    t.mudo(&id, sim)
}

#[tauri::command]
async fn espera(t: Tel<'_>, id: String, sim: bool) -> Result<(), String> {
    t.espera(&id, sim).await
}

#[tauri::command]
fn troca_microfone(t: Tel<'_>, microfone: Option<String>) -> Result<(), String> {
    t.troca_microfone(microfone)
}

#[tauri::command]
async fn mensagem(
    t: Tel<'_>,
    destino: String,
    texto: String,
    conta: Option<String>,
) -> Result<guarda::ItemConversa, String> {
    t.mensagem(destino.trim(), &texto, conta.as_deref()).await
}

#[tauri::command]
async fn salva_conta(t: Tel<'_>, conta: FormConta) -> Result<Estado, String> {
    let tel = Arc::clone(&t);
    tel.salva_conta(conta).await?;
    Ok(tel.estado())
}

#[tauri::command]
async fn remove_conta(t: Tel<'_>, id: String) -> Result<Estado, String> {
    t.remove_conta(&id).await?;
    Ok(t.estado())
}

#[tauri::command]
fn salva_preferencias(app: AppHandle, t: Tel<'_>, p: Preferencias) -> Result<(), String> {
    t.guarda().grava_preferencias(&p)?;
    if let Some(w) = app.get_webview_window("principal") {
        let _ = w.set_title(titulo(p.marca));
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Dispositivos {
    microfones: Vec<Dispositivo>,
    alto_falantes: Vec<Dispositivo>,
}

#[derive(Serialize)]
struct Dispositivo {
    id: String,
    nome: String,
    padrao: bool,
}

/// So as placas de verdade: o ALSA lista tambem plugins (null, upmix,
/// lavrate...) e cada placa em varios caminhos. Fica o padrao do sistema (que
/// no PipeWire segue o que se escolhe no KDE) e um item por placa.
fn de_verdade(lista: Vec<som::Dispositivo>) -> Vec<Dispositivo> {
    lista
        .into_iter()
        .filter(|d| {
            !d.id.starts_with("alsa:")
                || d.id == "alsa:default"
                || d.id.starts_with("alsa:sysdefault:CARD=")
        })
        .map(|d| Dispositivo {
            nome: if d.id == "alsa:default" {
                "Padrão do sistema".into()
            } else {
                d.nome
            },
            id: d.id,
            padrao: d.padrao,
        })
        .collect()
}

#[tauri::command]
fn dispositivos() -> Result<Dispositivos, String> {
    Ok(Dispositivos {
        microfones: de_verdade(som::microfones().map_err(|e| e.to_string())?),
        alto_falantes: de_verdade(som::alto_falantes().map_err(|e| e.to_string())?),
    })
}

// ---- Bandeja ----

fn bandeja(app: &tauri::App, marca: Marca) -> tauri::Result<()> {
    let abrir = MenuItem::with_id(
        app,
        "abrir",
        format!("Abrir o {}", titulo(marca)),
        true,
        None::<&str>,
    )?;
    let sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&abrir, &PredefinedMenuItem::separator(app)?, &sair])?;
    let icone = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::InvalidIcon(std::io::Error::other("sem icone")))?;
    TrayIconBuilder::with_id("hexagon-call")
        .icon(icone)
        .tooltip(titulo(marca))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "abrir" => mostra(app),
            "sair" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(t) = app.try_state::<Arc<Telefone>>() {
                        t.encerra().await;
                    }
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                mostra(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,janela=info,motor=info,som=info".into()),
        )
        .init();
    let guarda = match Guarda::abre() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("sem pasta de configuracao: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let na_bandeja = std::env::args().any(|a| a == "--bandeja");
    let marca = guarda.preferencias().marca;
    let mut guarda = Some(guarda);

    let resultado = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            mostra(app)
        }))
        .setup(move |app| {
            let Some(guarda) = guarda.take() else {
                return Ok(());
            };
            let telefone = Telefone::novo(app.handle().clone(), guarda);
            app.manage(Arc::clone(&telefone));
            bandeja(app, marca)?;
            if let Some(w) = app.get_webview_window("principal") {
                let _ = w.set_title(titulo(marca));
            }
            if !na_bandeja {
                mostra(app.handle());
            }
            tauri::async_runtime::spawn(async move { telefone.comeca().await });
            Ok(())
        })
        .on_window_event(|w, ev| {
            // Fechar esconde: o app segue na bandeja, recebendo ligacao.
            if let WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            estado,
            liga,
            atende,
            desliga,
            mudo,
            espera,
            troca_microfone,
            mensagem,
            salva_conta,
            remove_conta,
            salva_preferencias,
            dispositivos
        ])
        .build(tauri::generate_context!());
    match resultado {
        Ok(app) => {
            app.run(|_, ev| {
                // Sem janela visivel o app nao sai: so pelo "Sair" da bandeja.
                if let tauri::RunEvent::ExitRequested {
                    api, code: None, ..
                } = ev
                {
                    api.prevent_exit();
                }
            });
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            warn!("a janela nao abriu: {e}");
            eprintln!("a janela nao abriu: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
