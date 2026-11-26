//! Pausa o que estiver tocando (musica, video) quando a ligacao e atendida e
//! volta a tocar quando ela acaba (pedido de 26/11). So retoma o que ela mesma
//! pausou e que continua pausado: se nada tocava, ou se a pessoa parou a
//! musica durante a ligacao, nada volta a tocar sozinho.
//!
//! No Linux, pelo MPRIS no D-Bus (Spotify, navegadores, VLC, Elisa...). No
//! Windows, por enquanto, nada.

use std::sync::{Arc, Mutex, PoisonError};
use tracing::debug;

/// Os tocadores que a ligacao pausou (nomes no D-Bus).
#[derive(Default)]
pub struct Musica(Arc<Mutex<Vec<String>>>);

impl Musica {
    /// Pausa os que estao tocando e guarda quais foram (fora da tarefa do
    /// telefone: o D-Bus bloqueia).
    pub fn pausa(&self) {
        let pausados = Arc::clone(&self.0);
        std::thread::spawn(move || {
            let nomes = mpris::pausa_os_que_tocam();
            if !nomes.is_empty() {
                debug!(?nomes, "musica pausada para a ligacao");
            }
            pausados
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(nomes);
        });
    }

    /// Volta a tocar so o que a ligacao pausou.
    pub fn retoma(&self) {
        let nomes = std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner));
        if nomes.is_empty() {
            return;
        }
        std::thread::spawn(move || mpris::retoma(&nomes));
    }
}

#[cfg(target_os = "linux")]
mod mpris {
    use zbus::blocking::fdo::DBusProxy;
    use zbus::blocking::{Connection, Proxy};

    const CAMINHO: &str = "/org/mpris/MediaPlayer2";
    const TOCADOR: &str = "org.mpris.MediaPlayer2.Player";

    fn tocadores(c: &Connection) -> Vec<String> {
        DBusProxy::new(c)
            .and_then(|d| d.list_names().map_err(Into::into))
            .map(|nomes| {
                nomes
                    .into_iter()
                    .map(|n| n.to_string())
                    .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn tocador<'a>(c: &'a Connection, nome: &'a str) -> Option<Proxy<'a>> {
        Proxy::new(c, nome, CAMINHO, TOCADOR).ok()
    }

    fn estado(p: &Proxy<'_>) -> Option<String> {
        p.get_property::<String>("PlaybackStatus").ok()
    }

    pub fn pausa_os_que_tocam() -> Vec<String> {
        let Ok(c) = Connection::session() else {
            return Vec::new();
        };
        tocadores(&c)
            .into_iter()
            .filter(|nome| {
                tocador(&c, nome).is_some_and(|p| {
                    estado(&p).as_deref() == Some("Playing") && p.call_method("Pause", &()).is_ok()
                })
            })
            .collect()
    }

    pub fn retoma(nomes: &[String]) {
        let Ok(c) = Connection::session() else {
            return;
        };
        for nome in nomes {
            if let Some(p) = tocador(&c, nome)
                && estado(&p).as_deref() == Some("Paused")
            {
                let _ = p.call_method("Play", &());
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod mpris {
    pub fn pausa_os_que_tocam() -> Vec<String> {
        Vec::new()
    }

    pub fn retoma(_nomes: &[String]) {}
}
