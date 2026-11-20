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

/// "2 h 05 min", "12 min", "under a minute".
fn elapsed_text(ms: u64) -> String {
    let mins = ms / 60_000;
    match (mins / 60, mins % 60) {
        (0, 0) => "under a minute".into(),
        (0, m) => format!("{m} min"),
        (h, m) => format!("{h} h {m:02} min"),
    }
}

/// Read-only lines shown while the tunnel is up. No speeds or totals.
fn detail_lines(snap: &aegis_core::Snapshot) -> Vec<String> {
    let ConnectionState::Connected { since_ms, .. } = &snap.state else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(p) = &snap.profile {
        let country = p
            .meta
            .server
            .location
            .as_ref()
            .and_then(|l| aegis_core::registry::country(&l.country_code))
            .map(|c| c.name.as_str());
        out.push(match country {
            Some(c) => format!("Server: {} ({c})", p.meta.name),
            None => format!("Server: {}", p.meta.name),
        });
    }
    if let aegis_core::Feature::Verified(ip) = &snap.public_ip {
        out.push(format!("Public address: {ip}"));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    out.push(format!(
        "Connected: {}",
        elapsed_text(now.saturating_sub(*since_ms))
    ));
    out.push(
        if matches!(
            snap.protection.kill_switch,
            aegis_core::Feature::Verified(_)
        ) {
            "Kill switch: on"
        } else {
            "Kill switch: off"
        }
        .into(),
    );
    out
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

    let paused = app.try_state::<AppState>().and_then(|st| st.pause.info());
    let connected = matches!(snap.state, ConnectionState::Connected { .. });
    let pause_menu = if paused.is_some() {
        Submenu::with_id_and_items(
            app,
            "pause",
            "Paused",
            true,
            &[&MenuItem::with_id(
                app,
                "resume",
                "Resume now",
                true,
                None::<&str>,
            )?],
        )?
    } else {
        Submenu::with_id_and_items(
            app,
            "pause",
            "Pause VPN",
            connected,
            &[
                &MenuItem::with_id(app, "pause:5", "For 5 minutes", true, None::<&str>)?,
                &MenuItem::with_id(app, "pause:30", "For 30 minutes", true, None::<&str>)?,
                &MenuItem::with_id(app, "pause:120", "For 2 hours", true, None::<&str>)?,
            ],
        )?
    };

    let open = MenuItem::with_id(app, "open", "Open VPNFlow", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit (VPN stays as it is)", true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "Check for updates…", true, None::<&str>)?;
    let details: Vec<MenuItem<Wry>> = detail_lines(&snap)
        .into_iter()
        .enumerate()
        .map(|(i, t)| MenuItem::with_id(app, format!("detail:{i}"), t, false, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let sep = || PredefinedMenuItem::separator(app);
    let (s1, s2, s3, s4) = (sep()?, sep()?, sep()?, sep()?);
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = vec![
        &status,
        &s1,
        &quick,
        &connect,
        &disconnect,
        &pause_menu,
        &pref,
        &s2,
        &open,
        &settings_item,
        &updates,
        &s3,
    ];
    if !details.is_empty() {
        items.extend(
            details
                .iter()
                .map(|d| d as &dyn tauri::menu::IsMenuItem<Wry>),
        );
        items.push(&s4);
    }
    items.push(&quit);
    Menu::with_items(app, &items)
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
                app.state::<AppState>().pause.clear(app);
                if let Some(pid) = engine.settings().preferred_profile {
                    tauri::async_runtime::spawn(async move {
                        let _ = engine.connect(pid).await;
                    });
                }
            }
            "quick" => {
                app.state::<AppState>().pause.clear(app);
                tauri::async_runtime::spawn(async move {
                    let _ = engine.quick_connect().await;
                });
            }
            "disconnect" => {
                app.state::<AppState>().pause.clear(app);
                tauri::async_runtime::spawn(async move {
                    let _ = engine.disconnect().await;
                });
            }
            "resume" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let st = app.state::<AppState>();
                    let _ = crate::pause::resume_now(&st.pause, &app, &st.engine).await;
                });
            }
            "open" => crate::show_main(app),
            "settings" => {
                crate::show_main(app);
                let _ = app.emit("navigate", "settings");
            }
            "updates" => {
                crate::show_main(app);
                let _ = app.emit("navigate", "about");
                let _ = app.emit("check-updates", ());
            }
            "quit" => app.exit(0),
            other => {
                if let Some(mins) = other.strip_prefix("pause:").and_then(|m| m.parse().ok()) {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let st = app.state::<AppState>();
                        let _ = crate::pause::start(&st.pause, &app, &st.engine, mins).await;
                    });
                } else if let Some(pid) = other.strip_prefix("pref:").and_then(uuid_from) {
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
