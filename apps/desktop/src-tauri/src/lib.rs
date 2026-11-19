mod backend;
mod commands;
mod onboarding;
mod speed;
mod tray;
mod trusted;

use std::sync::Arc;

use aegis_core::{ConnectionState, Engine, EngineEvent, PeerState};
use commands::AppState;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

/// Page zoom applied at startup; 1.2 turns the 13-14 px body text into ~16-17 px.
const UI_ZOOM: f64 = 1.2;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("AEGIS_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let backend = tauri::async_runtime::block_on(backend::select());
            let dir = if matches!(backend, backend::AnyBackend::Mock(_)) {
                let simulation = tempfile::Builder::new()
                    .prefix("aegis-simulation-")
                    .tempdir()?;
                let path = simulation.path().to_path_buf();
                app.manage(simulation);
                path
            } else {
                app.path().app_data_dir()?
            };
            let engine = tauri::async_runtime::block_on(Engine::start(backend, &dir))
                .map_err(|e| format!("failed to start: {e}"))?;

            let tray_available = match tray::create(app.handle(), &engine) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!("system tray unavailable: {e}");
                    false
                }
            };
            app.manage(AppState {
                engine: Arc::clone(&engine),
                tray_available,
            });
            forward_events(app.handle().clone(), Arc::clone(&engine));
            trusted::spawn(Arc::clone(&engine));

            // Larger text for readability: the UI uses fixed px sizes, so scale the
            // whole page (text and spacing together) like browser zoom.
            // Ctrl+= / Ctrl+- / Ctrl+0 adjust it further (zoomHotkeysEnabled).
            if let Some(w) = app.get_webview_window("main")
                && let Err(e) = w.set_zoom(UI_ZOOM)
            {
                tracing::warn!("could not set UI zoom: {e}");
            }

            let s = engine.settings();
            if s.connect_on_launch
                && let Some(id) = s.preferred_profile
                && engine.snapshot().state.is_idle()
            {
                let e = Arc::clone(&engine);
                tauri::async_runtime::spawn(async move {
                    let _ = e.connect(id).await; // failures surface as state + events
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let st = window.state::<AppState>();
                // Closing the UI never touches the tunnel; with a tray we just hide.
                if st.tray_available && st.engine.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                let engine = Arc::clone(&window.state::<AppState>().engine);
                let app = window.app_handle().clone();
                let paths = paths.clone();
                tauri::async_runtime::spawn(commands::import_dropped(app, engine, paths));
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::capabilities,
            commands::snapshot,
            commands::events,
            commands::profiles,
            commands::country_catalog,
            commands::update_server,
            commands::server_health,
            commands::refresh_server_health,
            commands::quick_connect,
            commands::settings,
            commands::set_settings,
            commands::connect,
            commands::disconnect,
            commands::cancel,
            commands::import_dialog,
            commands::new_draft,
            commands::profile_draft,
            commands::create_profile,
            commands::update_profile,
            commands::duplicate_profile,
            commands::delete_profile,
            commands::export_profile,
            commands::diagnostics,
            commands::export_diagnostics,
            commands::speed_test,
            commands::open_link,
            commands::scan_downloads,
            commands::current_network,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!("Aegis VPN exited with an error: {e}");
        std::process::exit(1);
    }
}

pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn forward_events(app: tauri::AppHandle, engine: Arc<Engine<backend::AnyBackend>>) {
    let mut rx = engine.subscribe();
    tauri::async_runtime::spawn(async move {
        let mut last: Option<ConnectionState> = None;
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            };
            if let EngineEvent::Snapshot(s) = &ev
                && last.as_ref() != Some(&s.state)
            {
                notify_transition(&app, &engine, last.as_ref(), &s.state);
                last = Some(s.state.clone());
                tray::refresh(&app, &engine);
            }
            if matches!(
                ev,
                EngineEvent::ProfilesChanged | EngineEvent::SettingsChanged(_)
            ) {
                tray::refresh(&app, &engine);
            }
            let _ = app.emit("engine", &ev);
        }
    });
}

fn notify_transition(
    app: &tauri::AppHandle,
    engine: &Engine<backend::AnyBackend>,
    prev: Option<&ConnectionState>,
    next: &ConnectionState,
) {
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible || !engine.settings().notifications {
        return;
    }
    let body = match next {
        ConnectionState::Connected {
            peer: PeerState::Confirmed,
            ..
        } if !matches!(
            prev,
            Some(ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            })
        ) =>
        {
            "VPN connected"
        }
        ConnectionState::Reconnecting { attempt: 1, .. } => "VPN connection lost. Reconnecting.",
        ConnectionState::Failed { .. } => "VPN connection failed. Open Aegis VPN for details.",
        _ => return,
    };
    show_notification(body);
}

/// Desktop notification via notify-rust: freedesktop D-Bus on Linux (zbus 6 on
/// our tokio runtime), WinRT toasts on Windows. Failures are logged, never fatal.
fn show_notification(body: &'static str) {
    let mut n = notify_rust::Notification::new();
    n.appname("Aegis VPN").summary("Aegis VPN").body(body);
    #[cfg(all(unix, not(target_os = "macos")))]
    tauri::async_runtime::spawn(async move {
        if let Err(e) = n.show_async().await {
            tracing::warn!("could not show notification: {e}");
        }
    });
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = n.show() {
            tracing::warn!("could not show notification: {e}");
        }
    });
}
