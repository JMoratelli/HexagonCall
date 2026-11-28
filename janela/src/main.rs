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

#[cfg(target_os = "linux")]
mod atalho;
mod atualizador;
#[cfg(windows)]
mod aumid;
mod gravacao;
mod guarda;
mod musica;
mod notificacao;
mod perdidas;
mod telefone;

use guarda::{Flavor, Guarda, Preferencias};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};
use telefone::{Estado, FormConta, Telefone};
use tracing::{info, warn};

type Tel<'a> = State<'a, Arc<Telefone>>;

/// Mostra a janela (bandeja, segunda abertura, ligacao chegando).
pub fn mostra(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("principal") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    // O ponto das perdidas de novo, fora da thread principal (perdidas.rs).
    if let Some(t) = app.try_state::<Arc<Telefone>>() {
        let t = Arc::clone(&t);
        tauri::async_runtime::spawn(async move { t.reaplica_perdidas() });
    }
}

pub fn titulo(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Mcalls => "M Calls",
        Flavor::Hexagon => "Hexagon Call",
    }
}

/// O id do icone da bandeja (o ponto das perdidas troca o icone dele).
pub const BANDEJA: &str = "hexagon-call";

/// O icone do flavor: o do Hexagon Call vai no executavel (icons/icon.png);
/// o do M Calls entra na hora, pelo arquivo de preferencias, sem compilar de
/// novo.
pub fn icone_do_flavor(app: &AppHandle, flavor: Flavor) -> Option<tauri::image::Image<'static>> {
    match flavor {
        Flavor::Hexagon => app
            .default_window_icon()
            .cloned()
            .map(tauri::image::Image::to_owned),
        Flavor::Mcalls => tauri::image::Image::from_bytes(include_bytes!("../icons/m-calls.png"))
            .map_err(|e| warn!("icone do M Calls: {e}"))
            .ok(),
    }
}

// ---- Comandos da tela ----

#[tauri::command]
fn estado(t: Tel<'_>) -> Estado {
    t.estado()
}

/// Alguem mexeu na tela de Chamadas: as perdidas foram vistas. Assincrono de
/// proposito: fora da thread principal, que a bandeja usa.
#[tauri::command]
async fn viu_perdidas(t: Tel<'_>) -> Result<(), String> {
    t.viu_perdidas();
    Ok(())
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

/// Abrir com o sistema, so na bandeja (`--bandeja`): liga (`Some(true)`),
/// desliga (`Some(false)`) ou so diz como esta (`None`). Quem guarda e o
/// sistema (a pasta de inicio do KDE, o Run do Windows).
#[tauri::command]
fn abrir_com_o_sistema(app: AppHandle, t: Tel<'_>, sim: Option<bool>) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    if sim.is_some() {
        so_sem_administrador(&t)?;
    }
    let auto = app.autolaunch();
    match sim {
        Some(true) => auto.enable().map_err(|e| e.to_string())?,
        Some(false) => auto.disable().map_err(|e| e.to_string())?,
        None => {}
    }
    auto.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
async fn ouve_toque(t: Tel<'_>, toque: String) -> Result<(), String> {
    t.ouve_toque(toque).await
}

#[tauri::command]
fn tons(t: Tel<'_>, id: String, teclas: String) -> Result<(), String> {
    t.tons(&id, &teclas)
}

#[tauri::command]
async fn transfere(t: Tel<'_>, id: String, destino: String) -> Result<(), String> {
    t.transfere(&id, destino.trim()).await
}

#[tauri::command]
fn tira_do_mudo(t: Tel<'_>) -> Result<(), String> {
    t.tira_do_mudo()
}

#[tauri::command]
fn testa_microfone(t: Tel<'_>, microfone: Option<String>) -> Result<(), String> {
    let tel = Arc::clone(&t);
    tel.testa_microfone(microfone.filter(|m| !m.is_empty()))
}

#[tauri::command]
fn para_teste_microfone(t: Tel<'_>) {
    t.para_teste_microfone();
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

/// Com `ocultaConfig` no arquivo, as contas e o que a tela esconde sao de quem
/// administra: a tela nao muda, nem chamando o comando direto.
fn so_sem_administrador(t: &Telefone) -> Result<(), String> {
    if t.guarda().preferencias().oculta_config {
        return Err("Neste computador, isso é definido pelo administrador.".into());
    }
    Ok(())
}

#[tauri::command]
async fn salva_conta(t: Tel<'_>, conta: FormConta) -> Result<Estado, String> {
    // A primeira conta se cadastra mesmo com ocultaConfig (27/11): e o primeiro
    // passo da instalacao, e so depois dela as contas somem da tela.
    let primeira = conta.id_antigo.is_none() && t.guarda().contas().is_ok_and(|c| c.is_empty());
    if !primeira {
        so_sem_administrador(&t)?;
    }
    let tel = Arc::clone(&t);
    tel.salva_conta(conta).await?;
    Ok(tel.estado())
}

#[tauri::command]
async fn reconecta(t: Tel<'_>, id: String) -> Result<(), String> {
    let tel = Arc::clone(&t);
    tel.reconecta(&id).await
}

#[tauri::command]
async fn remove_conta(t: Tel<'_>, id: String) -> Result<Estado, String> {
    so_sem_administrador(&t)?;
    t.remove_conta(&id).await?;
    Ok(t.estado())
}

#[tauri::command]
fn salva_preferencias(app: AppHandle, t: Tel<'_>, p: Preferencias) -> Result<(), String> {
    // O flavor e o que quem administra decide so mudam pelo arquivo.
    // Um arquivo que nao abre (de quem administra, com um erro) nao se grava por cima.
    let p = t
        .guarda()
        .preferencias_lidas()
        .map_err(|e| format!("o arquivo de preferências tem um erro; corrija ou apague: {e}"))?
        .com_o_que_a_tela_muda(p);
    t.guarda().grava_preferencias(&p)?;
    if let Some(w) = app.get_webview_window("principal") {
        let _ = w.set_title(titulo(p.flavor));
    }
    Ok(())
}

/// Exclui do historico as ligacoes escolhidas (a tela ja confirmou) e, com
/// `gravacoes`, as gravacoes delas.
#[tauri::command]
fn exclui_historico(
    t: Tel<'_>,
    itens: Vec<guarda::ChaveHistorico>,
    gravacoes: bool,
) -> Result<telefone::Exclusao, String> {
    t.exclui_historico(&itens, gravacoes)
}

/// O nao perturbe, pelo botao do status (vale tambem com `ocultaConfig`: e de
/// uso, nao de configuracao).
#[tauri::command]
fn nao_perturbe(t: Tel<'_>, sim: bool) -> Result<Estado, String> {
    t.nao_perturbe(sim)?;
    Ok(t.estado())
}

/// O nome de um ramal, dado em Ramais (vazio tira). Devolve todos.
#[tauri::command]
fn salva_nome(
    t: Tel<'_>,
    ramal: String,
    nome: String,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    t.grava_nome(&ramal, &nome)
}

#[tauri::command]
fn gravacoes(t: Tel<'_>) -> Result<gravacao::Resumo, String> {
    Ok(t.gravacoes()?.resumo())
}

/// Quantas gravacoes uma retencao de `dias` apagaria: a tela confirma antes.
#[tauri::command]
fn gravacoes_mais_velhas(t: Tel<'_>, dias: u32) -> Result<usize, String> {
    Ok(t.gravacoes()?.mais_velhas(dias))
}

/// Aplica agora a retencao que esta salva (depois da confirmacao na tela).
#[tauri::command]
fn aplica_retencao(t: Tel<'_>) -> Result<usize, String> {
    let dias = t
        .guarda()
        .retencao_salva()
        .ok_or("as preferências salvas não abriram: nada foi apagado")?;
    Ok(t.gravacoes()?.aplica_retencao(dias))
}

/// O arquivo da gravacao, em bytes, para a tela tocar.
#[tauri::command]
async fn ouve_gravacao(t: Tel<'_>, arquivo: String) -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(t.gravacoes()?.le(&arquivo)?))
}

#[tauri::command]
fn abre_pasta_gravacoes(t: Tel<'_>) -> Result<(), String> {
    t.gravacoes()?.mostra_pasta()
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

/// "Padrao do sistema" primeiro (no PipeWire, segue o que se escolhe no KDE) e
/// depois cada dispositivo. No ALSA cru (sem servidor de som), so as placas:
/// ele lista tambem plugins (null, upmix, lavrate) e cada placa em varios
/// caminhos.
fn de_verdade(lista: Vec<som::Dispositivo>) -> Vec<Dispositivo> {
    let padrao = Dispositivo {
        id: String::new(),
        nome: "Padrão do sistema".into(),
        padrao: false,
    };
    std::iter::once(padrao)
        .chain(
            lista
                .into_iter()
                .filter(|d| !d.id.starts_with("alsa:") || d.id.starts_with("alsa:sysdefault:CARD="))
                .map(|d| Dispositivo {
                    id: d.id,
                    nome: d.nome,
                    padrao: d.padrao,
                }),
        )
        .collect()
}

#[tauri::command]
fn dispositivos() -> Result<Dispositivos, String> {
    Ok(Dispositivos {
        microfones: de_verdade(som::microfones().map_err(|e| e.to_string())?),
        alto_falantes: de_verdade(som::alto_falantes().map_err(|e| e.to_string())?),
    })
}

// ---- Atualizacao ----

/// A versao nova encontrada (a checagem roda sozinha, ou pelo Sobre) e se a
/// instalacao ja comecou.
#[derive(Default)]
struct Atualizacao {
    pendente: Mutex<Option<atualizador::Release>>,
    instalando: AtomicBool,
}

/// O que chega na tela pelo evento "atualizacao".
#[derive(Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "kebab-case")]
enum AvisoAtualizacao {
    Disponivel {
        release: atualizador::Release,
        atual: String,
    },
    Baixando {
        fracao: f64,
    },
    Instalando,
    Erro {
        texto: String,
    },
}

fn avisa_atualizacao(app: &AppHandle, aviso: AvisoAtualizacao) {
    use tauri::Emitter;
    if let Err(e) = app.emit("atualizacao", aviso) {
        warn!("aviso de atualizacao para a tela: {e}");
    }
}

/// Guarda o que a checagem achou e avisa a tela quando ha versao nova.
fn guarda_checagem(app: &AppHandle, achada: Option<atualizador::Release>) {
    if let Some(a) = app.try_state::<Atualizacao>() {
        *a.pendente.lock().unwrap_or_else(PoisonError::into_inner) = achada.clone();
    }
    if let Some(release) = achada {
        info!(tag = %release.tag, "atualizacao disponivel");
        avisa_atualizacao(
            app,
            AvisoAtualizacao::Disponivel {
                release,
                atual: atualizador::versao_atual().to_string(),
            },
        );
    }
}

async fn checa_atualizacao() -> Result<Option<atualizador::Release>, String> {
    tauri::async_runtime::spawn_blocking(atualizador::checa)
        .await
        .map_err(|e| e.to_string())?
}

/// Checa sozinho: 30 s depois de abrir (a rede do boot ja subiu) e a cada 6
/// horas, porque o app fica dias aberto na bandeja. Silencioso: sem rede ou
/// sem versao nova, ninguem fica sabendo.
fn vigia_atualizacao(app: AppHandle) {
    if !atualizador::suportado() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            match checa_atualizacao().await {
                Ok(achada) => guarda_checagem(&app, achada),
                Err(e) => info!("checar atualizacao: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoAtualizacao {
    versao: &'static str,
    suportado: bool,
    pendente: Option<atualizador::Release>,
}

#[tauri::command]
fn atualizacao(a: State<'_, Atualizacao>) -> InfoAtualizacao {
    InfoAtualizacao {
        versao: atualizador::versao_atual(),
        suportado: atualizador::suportado(),
        pendente: a
            .pendente
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
    }
}

/// "Procurar atualização", no Sobre.
#[tauri::command]
async fn procura_atualizacao(app: AppHandle) -> Result<Option<atualizador::Release>, String> {
    let achada = checa_atualizacao().await?;
    guarda_checagem(&app, achada.clone());
    Ok(achada)
}

/// Baixa, confere e dispara o instalador; depois desregistra as contas e
/// sai. O instalador reabre o app na versao nova. `false`: ja nao ha versao
/// nova para instalar.
#[tauri::command]
async fn atualiza(app: AppHandle, t: Tel<'_>, a: State<'_, Atualizacao>) -> Result<bool, String> {
    if t.tem_chamada() {
        return Err("Termine a ligação antes de atualizar.".into());
    }
    if a.instalando.swap(true, Ordering::SeqCst) {
        return Err("A atualização já está em andamento.".into());
    }
    // A lista de novo, e nao a guardada: ela pode ter ficado velha (a release
    // foi corrigida, ou saiu outra mais nova). Com a guardada, o "Tentar de
    // novo" depois de um sha256 que nao conferiu repetia o mesmo erro.
    let rel = match checa_atualizacao().await {
        Ok(Some(rel)) => rel,
        Ok(None) => {
            a.instalando.store(false, Ordering::SeqCst);
            guarda_checagem(&app, None);
            return Ok(false);
        }
        Err(e) => {
            a.instalando.store(false, Ordering::SeqCst);
            return Err(format!("não consegui procurar a atualização: {e}"));
        }
    };
    *a.pendente.lock().unwrap_or_else(PoisonError::into_inner) = Some(rel.clone());
    let quem_avisa = app.clone();
    let quem_liga = Arc::clone(&t);
    let feito = tauri::async_runtime::spawn_blocking(move || {
        // Um aviso por ponto percentual, nao um por pedaco baixado.
        let ultimo = AtomicU32::new(u32::MAX);
        atualizador::instala(
            &rel,
            &|fracao| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pct = (fracao.clamp(0.0, 1.0) * 100.0) as u32;
                if ultimo.swap(pct, Ordering::Relaxed) != pct {
                    avisa_atualizacao(&quem_avisa, AvisoAtualizacao::Baixando { fracao });
                }
            },
            // Uma ligacao que chegou durante o download: a saida do app a
            // cortaria.
            &|| !quem_liga.tem_chamada(),
        )
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    if let Err(e) = feito {
        a.instalando.store(false, Ordering::SeqCst);
        avisa_atualizacao(&app, AvisoAtualizacao::Erro { texto: e.clone() });
        return Err(e);
    }
    avisa_atualizacao(&app, AvisoAtualizacao::Instalando);
    // O instalador ja esta rodando: sai do PBX direito e das notificacoes, e
    // deixa o lugar para ele copiar. Cinco segundos bastam ao PBX; sem
    // resposta, sai assim mesmo.
    let tel = Arc::clone(&t);
    let _ = tokio::time::timeout(Duration::from_secs(5), tel.encerra()).await;
    app.exit(0);
    Ok(true)
}

// ---- Bandeja ----

fn bandeja(app: &tauri::App, flavor: Flavor) -> tauri::Result<()> {
    let abrir = MenuItem::with_id(
        app,
        "abrir",
        format!("Abrir o {}", titulo(flavor)),
        true,
        None::<&str>,
    )?;
    let sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&abrir, &PredefinedMenuItem::separator(app)?, &sair])?;
    let icone = icone_do_flavor(app.handle(), flavor)
        .ok_or_else(|| tauri::Error::InvalidIcon(std::io::Error::other("sem icone")))?;
    TrayIconBuilder::with_id(BANDEJA)
        .icon(icone)
        .tooltip(titulo(flavor))
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
                .unwrap_or_else(|_| "warn,janela=info,motor=info,som=info,pulseaudio=off".into()),
        )
        .init();
    // A versao no log: depois de uma atualizacao, diz qual abriu.
    info!(versao = atualizador::versao_atual(), "abrindo");
    let guarda = match Guarda::abre() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("sem pasta de configuracao: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let na_bandeja = std::env::args().any(|a| a == "--bandeja");
    let preferencias = guarda.preferencias();
    let flavor = preferencias.flavor;
    let abrir_pelo_arquivo = preferencias
        .abrir_com_o_sistema
        .filter(|_| preferencias.oculta_config);
    let mut guarda = Some(guarda);

    let resultado = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            mostra(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--bandeja"]),
        ))
        .setup(move |app| {
            let Some(guarda) = guarda.take() else {
                return Ok(());
            };
            // O arquivo de preferencias com todas as chaves (o `ocultaConfig = 0`):
            // quem administra ve o que pode mudar. Aqui, e nao no main: a segunda
            // instancia ja saiu, e nao grava junto com a primeira.
            if let Err(e) = guarda.completa_preferencias() {
                warn!("completar as preferencias: {e}");
            }
            let telefone = Telefone::novo(app.handle().clone(), guarda);
            app.manage(Arc::clone(&telefone));
            app.manage(Atualizacao::default());
            // Abrir com o sistema guarda o caminho do exe: depois de mudar de
            // lugar (do exe solto para o instalado, numa atualizacao), o
            // caminho velho abriria a versao antiga no proximo login. Com
            // `ocultaConfig`, o arquivo pode mandar ligar ou desligar.
            {
                use tauri_plugin_autostart::ManagerExt;
                let auto = app.autolaunch();
                let ligado = auto.is_enabled().unwrap_or(false);
                let r = if abrir_pelo_arquivo.unwrap_or(ligado) {
                    auto.enable()
                } else if ligado {
                    auto.disable()
                } else {
                    Ok(())
                };
                if let Err(e) = r {
                    warn!("atualizar o abrir com o sistema: {e}");
                }
            }
            vigia_atualizacao(app.handle().clone());
            #[cfg(target_os = "linux")]
            atalho::garante(flavor);
            bandeja(app, flavor)?;
            if let Some(w) = app.get_webview_window("principal") {
                let _ = w.set_title(titulo(flavor));
                if flavor == Flavor::Mcalls
                    && let Some(i) = icone_do_flavor(app.handle(), flavor)
                {
                    let _ = w.set_icon(i);
                }
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
            viu_perdidas,
            liga,
            atende,
            desliga,
            mudo,
            espera,
            troca_microfone,
            tira_do_mudo,
            testa_microfone,
            para_teste_microfone,
            tons,
            transfere,
            ouve_toque,
            abrir_com_o_sistema,
            mensagem,
            salva_conta,
            remove_conta,
            reconecta,
            salva_preferencias,
            exclui_historico,
            salva_nome,
            nao_perturbe,
            gravacoes,
            gravacoes_mais_velhas,
            aplica_retencao,
            ouve_gravacao,
            abre_pasta_gravacoes,
            dispositivos,
            atualizacao,
            procura_atualizacao,
            atualiza
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
