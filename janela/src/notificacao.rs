//! A ligacao chegando aparece na notificacao do sistema, no canto da tela,
//! com Atender e Recusar (pedido de 26/11): no KDE e no GNOME pelo D-Bus, no
//! Windows como toast. Clicar no corpo abre a janela. Fica ate alguem agir, e
//! some quando a ligacao e atendida pela janela ou acaba. O som e o toque do
//! app, nao o da notificacao.
//!
//! A perdida tambem vai para a notificacao (pedido de 28/11), com "Ligar de
//! volta": uma por numero, que conta as repetidas no lugar de empilhar.
//!
//! No Windows o toast sai com o nome e o icone do app (nao "Windows
//! PowerShell"): o app registra o proprio AppUserModelID no HKCU, como o
//! instalador faz no atalho do Menu Iniciar (ver `windows::registra`).

use crate::guarda::Flavor;
use crate::telefone::Telefone;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Manager};
use tracing::warn;

/// A notificacao de cada ligacao chegando, pela plataforma.
pub struct Notificacoes {
    sistema: sistema::Sistema,
    /// Ramal -> quantas perdidas dele ainda nao vistas.
    perdidas: Mutex<HashMap<String, u32>>,
}

impl Notificacoes {
    pub fn nova(flavor: Flavor) -> Self {
        Self {
            sistema: sistema::Sistema::novo(flavor),
            perdidas: Mutex::default(),
        }
    }

    pub fn chegando(&self, app: &AppHandle, id: &str, quem: &str, ramal: &str) {
        self.sistema.chegando(app, id, quem, ramal);
    }

    /// Uma ligacao perdida: o aviso daquele numero, com "Ligar de volta",
    /// toma o lugar do anterior e diz quantas foram.
    pub fn perdida(&self, app: &AppHandle, ramal: &str, nome: Option<&str>) {
        let n = {
            let mut p = self.perdidas.lock().unwrap_or_else(PoisonError::into_inner);
            let n = p.entry(ramal.to_string()).or_insert(0);
            *n = n.saturating_add(1);
            *n
        };
        let titulo = if n == 1 {
            "Ligação perdida".to_string()
        } else {
            format!("{n} ligações perdidas")
        };
        self.sistema
            .perdida(app, ramal, &titulo, &quem(ramal, nome));
    }

    /// As perdidas foram vistas na tela: os avisos delas saem.
    pub fn fecha_perdidas(&self) {
        self.perdidas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.sistema.fecha_perdidas();
    }

    /// Uma mensagem chegou com a janela escondida ou atras de outra: quem
    /// mandou e o texto, com "Responder" (abre a conversa).
    pub fn mensagem(&self, app: &AppHandle, ramal: &str, nome: Option<&str>, texto: &str) {
        let corpo: String = texto.chars().take(200).collect();
        self.sistema
            .mensagem(app, ramal, &quem(ramal, nome), &corpo);
    }

    /// A ligacao foi atendida pela janela ou acabou: a notificacao sai.
    pub fn fecha(&self, id: &str) {
        self.sistema.fecha(id);
    }

    /// O app vai sair: nenhuma notificacao de ligacao fica para tras (nem a
    /// de perdida, cujo "Ligar de volta" nao teria mais quem atender).
    pub fn fecha_todas(&self) {
        self.sistema.fecha_todas();
        self.fecha_perdidas();
    }
}

/// "Financeiro · 1003", ou "Ramal 1003" sem nome.
fn quem(ramal: &str, nome: Option<&str>) -> String {
    match nome {
        Some(n) => format!("{n} · {ramal}"),
        None => format!("Ramal {ramal}"),
    }
}

/// O clique na notificacao. "default" e "abrir" sao o corpo (o Linux manda
/// um, o Windows o outro); os botoes mandam "atender" e "recusar".
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
                    warn!("atender pela notificacao: {e}");
                    tel.avisa_erro(format!("Não atendi: {e}"));
                }
            });
        }
        "recusar" => {
            tauri::async_runtime::spawn(async move {
                let _ = tel.desliga(&id).await;
            });
        }
        "default" | "abrir" => crate::mostra(app),
        _ => {}
    }
}

/// O clique no aviso da perdida: "Ligar de volta" liga pela tela (como o
/// botao dela); o corpo abre a janela em Chamadas.
fn responde_perdida(app: &AppHandle, ramal: &str, acao: &str) {
    let Some(t) = app.try_state::<Arc<Telefone>>() else {
        return;
    };
    match acao {
        "ligar" => {
            crate::mostra(app);
            t.liga_de_volta(ramal);
        }
        "default" | "abrir" => {
            crate::mostra(app);
            t.abre_chamadas();
        }
        _ => {}
    }
}

/// O clique no aviso da mensagem: abre a janela na conversa.
#[cfg_attr(windows, allow(dead_code))]
fn responde_mensagem(app: &AppHandle, ramal: &str, acao: &str) {
    if !matches!(acao, "default" | "abrir" | "responder") {
        return;
    }
    crate::mostra(app);
    if let Some(t) = app.try_state::<Arc<Telefone>>() {
        t.abre_conversa(ramal);
    }
}

#[cfg(not(windows))]
mod sistema {
    use super::{responde, responde_mensagem, responde_perdida};
    use crate::guarda::Flavor;
    #[cfg(all(unix, not(target_os = "macos")))]
    use notify_rust::Hint;
    use notify_rust::{Notification, Timeout, Urgency};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, PoisonError};
    use tauri::AppHandle;
    use tracing::debug;

    /// A notificacao aberta de cada ligacao chegando (id da ligacao -> id no
    /// sistema).
    pub struct Sistema {
        titulo: &'static str,
        abertas: Arc<Mutex<HashMap<String, u32>>>,
        /// O aviso de perdida de cada numero (ramal -> id no sistema).
        perdidas: Arc<Mutex<HashMap<String, u32>>>,
    }

    impl Sistema {
        pub fn novo(flavor: Flavor) -> Self {
            Self {
                titulo: crate::titulo(flavor),
                abertas: Arc::default(),
                perdidas: Arc::default(),
            }
        }

        /// O aviso anterior do mesmo numero fecha antes (a thread dele, que
        /// espera o clique, termina junto) e o novo entra com a conta. Fechar o
        /// anterior, mostrar e guardar o id vao sob a mesma trava: duas perdidas
        /// seguidas nao deixam dois avisos.
        pub fn perdida(&self, app: &AppHandle, ramal: &str, titulo: &str, quem: &str) {
            let mut n = Notification::new();
            n.appname(self.titulo)
                .summary(titulo)
                .body(quem)
                .icon("call-missed")
                .action("default", "Abrir")
                .action("ligar", "Ligar de volta")
                .urgency(Urgency::Normal);
            #[cfg(all(unix, not(target_os = "macos")))]
            n.hint(Hint::Category("call.unanswered".into()))
                .hint(Hint::SuppressSound(true));
            let app = app.clone();
            let ramal = ramal.to_string();
            let perdidas = Arc::clone(&self.perdidas);
            std::thread::spawn(move || {
                let handle = {
                    let mut abertas = perdidas.lock().unwrap_or_else(PoisonError::into_inner);
                    if let Some(anterior) = abertas.remove(&ramal) {
                        fecha_no_sistema(anterior);
                    }
                    let handle = match n.show() {
                        Ok(h) => h,
                        Err(e) => {
                            debug!("notificacao da perdida: {e}");
                            return;
                        }
                    };
                    #[cfg(target_os = "linux")]
                    abertas.insert(ramal.clone(), handle.id());
                    #[cfg(not(target_os = "linux"))]
                    let _ = &mut abertas;
                    handle
                };
                handle.wait_for_action(|acao| responde_perdida(&app, &ramal, acao));
            });
        }

        pub fn fecha_perdidas(&self) {
            let abertas: Vec<u32> = self
                .perdidas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .drain()
                .map(|(_, n)| n)
                .collect();
            for n in abertas {
                fecha_no_sistema(n);
            }
        }

        pub fn chegando(&self, app: &AppHandle, id: &str, quem: &str, ramal: &str) {
            let mut n = Notification::new();
            n.appname(self.titulo)
                .summary(&format!("Ligação de {quem}"))
                .body(&format!("{ramal} · chamando você"))
                .icon("call-start")
                .action("default", "Abrir")
                .action("recusar", "Recusar")
                .action("atender", "Atender")
                .urgency(Urgency::Critical)
                .timeout(Timeout::Never);
            // Categoria e "sem o som da notificacao" sao do padrao do Linux
            // (freedesktop).
            #[cfg(all(unix, not(target_os = "macos")))]
            n.hint(Hint::Category("call.incoming".into()))
                .hint(Hint::SuppressSound(true));
            let app = app.clone();
            let id = id.to_string();
            let abertas = Arc::clone(&self.abertas);
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

        pub fn mensagem(&self, app: &AppHandle, ramal: &str, quem: &str, corpo: &str) {
            let mut n = Notification::new();
            n.appname(self.titulo)
                .summary(quem)
                .body(corpo)
                .icon("mail-message-new")
                .action("default", "Abrir")
                .action("responder", "Responder")
                .urgency(Urgency::Normal);
            // O som e o do app (o de mensagem), nao o da notificacao.
            #[cfg(all(unix, not(target_os = "macos")))]
            n.hint(Hint::Category("im.received".into()))
                .hint(Hint::SuppressSound(true));
            let app = app.clone();
            let ramal = ramal.to_string();
            std::thread::spawn(move || match n.show() {
                Ok(h) => h.wait_for_action(|acao| responde_mensagem(&app, &ramal, acao)),
                Err(e) => debug!("notificacao da mensagem: {e}"),
            });
        }

        pub fn fecha(&self, id: &str) {
            let aberta = self
                .abertas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(id);
            if let Some(n) = aberta {
                fecha_no_sistema(n);
            }
        }

        pub fn fecha_todas(&self) {
            let abertas: Vec<u32> = self
                .abertas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .drain()
                .map(|(_, n)| n)
                .collect();
            for n in abertas {
                fecha_no_sistema(n);
            }
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

    #[cfg(not(target_os = "linux"))]
    fn fecha_no_sistema(_n: u32) {}
}

#[cfg(windows)]
mod sistema {
    use super::{responde, responde_perdida};
    use crate::guarda::Flavor;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, PoisonError};
    use tauri::AppHandle;
    use tracing::{debug, warn};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{
        ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
    };
    use windows::core::{HSTRING, IInspectable, Interface};

    /// O AUMID do PowerShell: o ultimo recurso, se o registro do nosso falhar
    /// (o toast sai com o nome errado, mas sai).
    const AUMID_POWERSHELL: &str =
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";
    /// Todas as notificacoes de ligacao ficam num grupo: o tag sozinho nao
    /// identifica o toast para tirar da tela.
    const GRUPO: &str = "ligacao";
    /// As perdidas, noutro grupo: o tag e o numero, e o toast novo do mesmo
    /// numero toma o lugar do anterior na central.
    const GRUPO_PERDIDAS: &str = "perdida";

    pub struct Sistema {
        aumid: String,
        /// O icone do app em PNG, para o toast mostrar grande (o do
        /// cabecalho vem do registro do AUMID).
        icone: Option<PathBuf>,
        /// Id da ligacao -> (tag, toast). O objeto fica guardado para o
        /// clique continuar chegando enquanto a ligacao toca.
        abertas: Mutex<HashMap<String, (String, ToastNotification)>>,
        /// Ramal -> (tag, toast) das perdidas, guardados pelo mesmo motivo.
        perdidas: Mutex<HashMap<String, (String, ToastNotification)>>,
        proximo: AtomicU64,
    }

    impl Sistema {
        pub fn novo(flavor: Flavor) -> Self {
            let (aumid, icone) = match crate::aumid::registra(flavor) {
                Ok(r) => (r.aumid, Some(r.icone)),
                Err(e) => {
                    warn!("registrar o app para as notificacoes: {e}");
                    (AUMID_POWERSHELL.to_string(), None)
                }
            };
            // Toast de ligacao de uma execucao anterior (o app caiu com uma
            // tocando) nao vale mais nada: sai da central.
            if let Err(e) = ToastNotificationManager::History()
                .and_then(|h| h.ClearWithId(&HSTRING::from(&aumid)))
            {
                debug!("limpar notificacoes antigas: {e}");
            }
            Self {
                aumid,
                icone,
                abertas: Mutex::new(HashMap::new()),
                perdidas: Mutex::new(HashMap::new()),
                proximo: AtomicU64::new(1),
            }
        }

        pub fn perdida(&self, app: &AppHandle, ramal: &str, titulo: &str, quem: &str) {
            // O tag tem limite de 16 no Windows 10 antigo: o fim do numero.
            let fim: String = ramal
                .chars()
                .rev()
                .take(15)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let tag = format!("p{fim}");
            match self.mostra_perdida(app, ramal, &tag, titulo, quem) {
                Ok(toast) => {
                    self.perdidas
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(ramal.to_string(), (tag, toast));
                }
                Err(e) => warn!("notificacao da perdida: {e}"),
            }
        }

        fn mostra_perdida(
            &self,
            app: &AppHandle,
            ramal: &str,
            tag: &str,
            titulo: &str,
            quem: &str,
        ) -> windows::core::Result<ToastNotification> {
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml_da_perdida(
                titulo,
                quem,
                self.icone.as_deref(),
            )))?;
            let toast = ToastNotification::CreateToastNotification(&doc)?;
            toast.SetTag(&HSTRING::from(tag))?;
            toast.SetGroup(&HSTRING::from(GRUPO_PERDIDAS))?;
            let app = app.clone();
            let ramal = ramal.to_string();
            toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                move |_, args| {
                    let acao = args
                        .as_ref()
                        .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                        .and_then(|a| a.Arguments().ok())
                        .map(|h| h.to_string())
                        .unwrap_or_default();
                    responde_perdida(&app, &ramal, if acao.is_empty() { "abrir" } else { &acao });
                    Ok(())
                },
            ))?;
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(&self.aumid))?
                .Show(&toast)?;
            Ok(toast)
        }

        pub fn fecha_perdidas(&self) {
            let abertas: Vec<(String, ToastNotification)> = self
                .perdidas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .drain()
                .map(|(_, t)| t)
                .collect();
            for (tag, toast) in &abertas {
                self.tira(tag, GRUPO_PERDIDAS, toast);
            }
        }

        /// O aviso da mensagem no Windows fica para o backlog do Windows
        /// (docs/estado.md): por ora, so o som e o numero na aba Conversas.
        pub fn mensagem(&self, _app: &AppHandle, _ramal: &str, _quem: &str, _corpo: &str) {}

        pub fn chegando(&self, app: &AppHandle, id: &str, quem: &str, ramal: &str) {
            // O tag tem limite de tamanho (16 no Windows 10 antigo) e o id da
            // ligacao e longo: um numero da vez basta.
            let tag = format!("lig{}", self.proximo.fetch_add(1, Ordering::Relaxed));
            match self.mostra(app, id, &tag, quem, ramal) {
                Ok(toast) => {
                    self.abertas
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(id.to_string(), (tag, toast));
                }
                Err(e) => warn!("notificacao da ligacao: {e}"),
            }
        }

        fn mostra(
            &self,
            app: &AppHandle,
            id: &str,
            tag: &str,
            quem: &str,
            ramal: &str,
        ) -> windows::core::Result<ToastNotification> {
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml_da_ligacao(
                quem,
                ramal,
                self.icone.as_deref(),
            )))?;
            let toast = ToastNotification::CreateToastNotification(&doc)?;
            toast.SetTag(&HSTRING::from(tag))?;
            toast.SetGroup(&HSTRING::from(GRUPO))?;
            toast.SetExpiresOnReboot(true)?;
            let app = app.clone();
            let id = id.to_string();
            toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                move |_, args| {
                    let acao = args
                        .as_ref()
                        .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                        .and_then(|a| a.Arguments().ok())
                        .map(|h| h.to_string())
                        .unwrap_or_default();
                    responde(&app, &id, if acao.is_empty() { "abrir" } else { &acao });
                    Ok(())
                },
            ))?;
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(&self.aumid))?
                .Show(&toast)?;
            Ok(toast)
        }

        pub fn fecha(&self, id: &str) {
            let aberta = self
                .abertas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(id);
            if let Some((tag, toast)) = aberta {
                self.tira(&tag, GRUPO, &toast);
            }
        }

        pub fn fecha_todas(&self) {
            let abertas: Vec<(String, ToastNotification)> = self
                .abertas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .drain()
                .map(|(_, t)| t)
                .collect();
            for (tag, toast) in &abertas {
                self.tira(tag, GRUPO, toast);
            }
        }

        /// Tira da tela (Hide) e da central de notificacoes (History): o
        /// cenario de ligacao fica na tela ate alguem agir, entao nao some
        /// sozinho quando a ligacao acaba.
        fn tira(&self, tag: &str, grupo: &str, toast: &ToastNotification) {
            let aumid = HSTRING::from(&self.aumid);
            if let Err(e) = ToastNotificationManager::CreateToastNotifierWithId(&aumid)
                .and_then(|n| n.Hide(toast))
            {
                debug!("esconder a notificacao: {e}");
            }
            if let Err(e) = ToastNotificationManager::History().and_then(|h| {
                h.RemoveGroupedTagWithId(&HSTRING::from(tag), &HSTRING::from(grupo), &aumid)
            }) {
                debug!("tirar a notificacao da central: {e}");
            }
        }
    }

    /// O toast da ligacao: cenario de ligacao (fica na tela, botoes grandes),
    /// sem som (o toque e o do app) e o corpo abrindo a janela.
    pub(super) fn xml_da_ligacao(quem: &str, ramal: &str, icone: Option<&Path>) -> String {
        let imagem = icone.map_or(String::new(), |p| {
            format!(
                r#"<image placement="appLogoOverride" hint-crop="circle" src="{}"/>"#,
                escapa(&uri_de_arquivo(p))
            )
        });
        format!(
            r#"<toast scenario="incomingCall" launch="abrir"><visual><binding template="ToastGeneric"><text>Ligação de {}</text><text>{} · chamando você</text>{imagem}</binding></visual><actions><action content="Recusar" arguments="recusar"/><action content="Atender" arguments="atender"/></actions><audio silent="true"/></toast>"#,
            escapa(quem),
            escapa(ramal)
        )
    }

    /// O toast da perdida: o comum (some sozinho e fica na central), sem som (a
    /// ligacao ja tocou), com "Ligar de volta" e o corpo abrindo a janela.
    pub(super) fn xml_da_perdida(titulo: &str, quem: &str, icone: Option<&Path>) -> String {
        let imagem = icone.map_or(String::new(), |p| {
            format!(
                r#"<image placement="appLogoOverride" hint-crop="circle" src="{}"/>"#,
                escapa(&uri_de_arquivo(p))
            )
        });
        format!(
            r#"<toast launch="abrir"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text>{imagem}</binding></visual><actions><action content="Ligar de volta" arguments="ligar"/></actions><audio silent="true"/></toast>"#,
            escapa(titulo),
            escapa(quem)
        )
    }

    /// file:///C:/Users/... com espaco e % escapados (nome de usuario com
    /// espaco e comum).
    fn uri_de_arquivo(p: &Path) -> String {
        let mut s = String::from("file:///");
        for c in p.to_string_lossy().chars() {
            match c {
                '\\' => s.push('/'),
                ' ' => s.push_str("%20"),
                '%' => s.push_str("%25"),
                '#' => s.push_str("%23"),
                c => s.push(c),
            }
        }
        s
    }

    fn escapa(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                c => out.push(c),
            }
        }
        out
    }

    #[cfg(test)]
    mod testes {
        use super::*;

        #[test]
        fn nome_com_xml_nao_quebra_o_toast() {
            let x = xml_da_ligacao("<Loja & Cia>", "1001", None);
            assert!(x.contains("Ligação de &lt;Loja &amp; Cia&gt;"));
            let doc = XmlDocument::new().unwrap();
            doc.LoadXml(&HSTRING::from(x)).unwrap();
        }

        #[test]
        fn a_perdida_vira_toast_com_ligar_de_volta() {
            let x = xml_da_perdida("2 ligações perdidas", "<Loja & Cia> · 1003", None);
            assert!(x.contains("&lt;Loja &amp; Cia&gt; · 1003"));
            assert!(x.contains(r#"arguments="ligar""#));
            let doc = XmlDocument::new().unwrap();
            doc.LoadXml(&HSTRING::from(x)).unwrap();
        }

        #[test]
        fn icone_vira_uri_de_arquivo() {
            let x = xml_da_ligacao(
                "Ana",
                "1002",
                Some(Path::new(
                    r"C:\Users\Joao Silva\AppData\Local\hexagon-call\icone.png",
                )),
            );
            assert!(
                x.contains(
                    r#"src="file:///C:/Users/Joao%20Silva/AppData/Local/hexagon-call/icone.png""#
                ),
                "{x}"
            );
            assert!(x.contains(r#"scenario="incomingCall""#));
            assert!(x.contains(r#"<audio silent="true"/>"#));
        }
    }
}
