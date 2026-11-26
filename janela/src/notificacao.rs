//! A ligacao chegando aparece na notificacao do sistema, no canto da tela,
//! com Atender e Recusar (pedido de 26/11): no KDE e no GNOME pelo D-Bus, no
//! Windows como toast. Clicar no corpo abre a janela. Fica ate alguem agir, e
//! some quando a ligacao e atendida pela janela ou acaba. O som e o toque do
//! app, nao o da notificacao.

use crate::telefone::Telefone;
use notify_rust::{Hint, Notification, Timeout, Urgency};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Manager};
use tracing::debug;

/// A notificacao aberta de cada ligacao chegando (id da ligacao -> id no sistema).
#[derive(Default)]
pub struct Notificacoes(Arc<Mutex<HashMap<String, u32>>>);

impl Notificacoes {
    pub fn chegando(&self, app: &AppHandle, titulo: &str, id: &str, quem: &str, ramal: &str) {
        let mut n = Notification::new();
        n.appname(titulo)
            .summary(&format!("Ligação de {quem}"))
            .body(&format!("{ramal} · chamando você"))
            .icon("call-start")
            .action("default", "Abrir")
            .action("recusar", "Recusar")
            .action("atender", "Atender")
            .hint(Hint::Category("call.incoming".into()))
            .hint(Hint::SuppressSound(true))
            .urgency(Urgency::Critical)
            .timeout(Timeout::Never);
        let app = app.clone();
        let id = id.to_string();
        let abertas = Arc::clone(&self.0);
        // `show` e `wait_for_action` bloqueiam: numa thread so dela.
        std::thread::spawn(move || {
            let handle = match n.show() {
                Ok(h) => h,
                Err(e) => {
                    debug!("notificacao: {e}");
                    return;
                }
            };
            #[cfg(target_os = "linux")]
            abertas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id.clone(), handle.id());
            #[cfg(not(target_os = "linux"))]
            let _ = &abertas;
            handle.wait_for_action(|acao| responde(&app, &id, acao));
        });
    }

    /// A ligacao foi atendida pela janela ou acabou: a notificacao sai.
    pub fn fecha(&self, id: &str) {
        let aberta = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        if let Some(n) = aberta {
            fecha_no_sistema(n);
        }
    }
}

fn responde(app: &AppHandle, id: &str, acao: &str) {
    let Some(t) = app.try_state::<Arc<Telefone>>() else {
        return;
    };
    let tel = Arc::clone(&t);
    let id = id.to_string();
    match acao {
        "atender" => {
            crate::mostra(app);
            tauri::async_runtime::spawn(async move {
                if let Err(e) = tel.atende(&id).await {
                    debug!("atender pela notificacao: {e}");
                }
            });
        }
        "recusar" => {
            tauri::async_runtime::spawn(async move {
                let _ = tel.desliga(&id).await;
            });
        }
        "default" => crate::mostra(app),
        _ => {}
    }
}

#[cfg(target_os = "linux")]
fn fecha_no_sistema(n: u32) {
    std::thread::spawn(move || {
        let r = zbus::blocking::Connection::session().and_then(|c| {
            c.call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "CloseNotification",
                &(n,),
            )
            .map(|_| ())
        });
        if let Err(e) = r {
            debug!("fechar a notificacao: {e}");
        }
    });
}

/// No Windows o toast some sozinho; nao ha como tirar pelo id.
#[cfg(not(target_os = "linux"))]
fn fecha_no_sistema(_n: u32) {}
