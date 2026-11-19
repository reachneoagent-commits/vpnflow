//! System tray. Quitting from the tray closes the UI only; a connected
//! tunnel stays up (it belongs to NetworkManager / the tunnel service).

use std::sync::Arc;

use aegis_core::{ConnectionState, Engine, PeerState, ProfileId};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::backend::AnyBackend;
use crate::commands::AppState;

const TRAY_ID: &str = "main";

fn status_text(s: &ConnectionState) -> &'static str {
    match s {
        ConnectionState::Disconnected => "Not connected",
        ConnectionState::Preparing { .. } | ConnectionState::Connecting { .. } => "Connecting…",
        ConnectionState::Connected {
            peer: PeerState::Confirmed,
            ..
        } => "Connected",
        ConnectionState::Connected {
            peer: PeerState::NoResponse,
            ..
        } => "No response from server",
        ConnectionState::Connected { .. } => "Tunnel up, waiting for server",
        ConnectionState::Reconnecting { .. } => "Reconnecting…",
        ConnectionState::Disconnecting { .. } => "Disconnecting…",
        ConnectionState::Failed { .. } => "Connection failed",
        ConnectionState::Uncertain { .. } => "Tunnel status unknown; may still be active",
    }
}

fn build_menu(app: &AppHandle, engine: &Engine<AnyBackend>) -> tauri::Result<Menu<Wry>> {
    let snap = engine.snapshot();
    let profiles = engine.profiles();
    let settings = engine.settings();
    let preferred = profiles
        .iter()
        .find(|p| Some(p.meta.id) == settings.preferred_profile);
    let idle = snap.state.is_idle();

    let status = MenuItem::with_id(app, "status", status_text(&snap.state), false, None::<&str>)?;
    let connect_label = match preferred {
        Some(p) => format!("Connect {}", p.meta.name),
        None => "Connect (choose a preferred profile)".into(),
    };
    let connect = MenuItem::with_id(
        app,
        "connect",
        connect_label,
        idle && engine.capabilities().available.is_supported()
            && preferred.is_some_and(|p| aegis_core::registry::eligible(&p.meta)),
        None::<&str>,
    )?;
    let quick = MenuItem::with_id(
        app,
        "quick",
        "Quick connect",
        idle && engine.capabilities().available.is_supported()
            && profiles
                .iter()
                .any(|p| aegis_core::registry::eligible(&p.meta)),
        None::<&str>,
    )?;
    let blocking = matches!(
        snap.protection.kill_switch,
        aegis_core::Feature::Verified(_)
    );
    let disconnect = MenuItem::with_id(
        app,
        "disconnect",
        if idle && blocking {
            "Stop blocking"
        } else {
            "Disconnect"
        },
        !idle || blocking,
        None::<&str>,
    )?;

    // Prefer-profile changes take the engine op lock; disable while a
    // connect/disconnect/uncertain session is in progress so Busy is not a
    // silent no-op on the tray.
    let items: Vec<CheckMenuItem<Wry>> = profiles
        .iter()
        .map(|p| {
            CheckMenuItem::with_id(
                app,
                format!("pref:{}", p.meta.id),
                &p.meta.name,
                idle,
                p.preferred,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items.iter().map(|i| i as _).collect();
    let pref = Submenu::with_id_and_items(
        app,
        "preferred",
        "Preferred profile",
        idle && !profiles.is_empty(),
        &refs,
    )?;

    let open = MenuItem::with_id(app, "open", "Open VPNFlow", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit (VPN stays as it is)", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &status,
            &sep()?,
            &quick,
            &connect,
            &disconnect,
            &pref,
            &sep()?,
            &open,
            &settings_item,
            &sep()?,
            &quit,
        ],
    )
}

pub fn create(app: &AppHandle, engine: &Arc<Engine<AnyBackend>>) -> tauri::Result<()> {
    let menu = build_menu(app, engine)?;
    let mut b = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("VPNFlow")
        .show_menu_on_left_click(true);
    // The plain white mark, as a template image so macOS tints it to match the
    // menu bar (light or dark). Fall back to the app icon if it cannot load.
    match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        Ok(icon) => b = b.icon(icon).icon_as_template(true),
        Err(_) => {
            if let Some(icon) = app.default_window_icon() {
                b = b.icon(icon.clone());
            }
        }
    }
    b.on_menu_event(|app, ev| {
        let engine = Arc::clone(&app.state::<AppState>().engine);
        let id = ev.id().as_ref().to_owned();
        match id.as_str() {
            "connect" => {
                if let Some(pid) = engine.settings().preferred_profile {
                    tauri::async_runtime::spawn(async move {
                        let _ = engine.connect(pid).await;
                    });
                }
            }
            "quick" => {
                tauri::async_runtime::spawn(async move {
                    let _ = engine.quick_connect().await;
                });
            }
            "disconnect" => {
                tauri::async_runtime::spawn(async move {
                    let _ = engine.disconnect().await;
                });
            }
            "open" => crate::show_main(app),
            "settings" => {
                crate::show_main(app);
                let _ = app.emit("navigate", "settings");
            }
            "quit" => app.exit(0),
            other => {
                if let Some(pid) = other.strip_prefix("pref:").and_then(uuid_from) {
                    let mut s = engine.settings();
                    s.preferred_profile = if s.preferred_profile == Some(pid) {
                        None
                    } else {
                        Some(pid)
                    };
                    if let Err(e) = engine.set_settings(s) {
                        crate::show_main(app);
                        let _ = app.emit("settings-error", e.to_string());
                    }
                    refresh(app, &engine);
                }
            }
        }
    })
    .build(app)?;
    Ok(())
}

fn uuid_from(s: &str) -> Option<ProfileId> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

pub fn refresh(app: &AppHandle, engine: &Engine<AnyBackend>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Ok(menu) = build_menu(app, engine)
    {
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_tooltip(Some(format!(
            "VPNFlow: {}",
            status_text(&engine.snapshot().state)
        )));
    }
}
