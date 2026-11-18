//! Tauri commands: thin wrappers over the engine. No command accepts a file
//! path from web content; import/export open native dialogs here instead.

use std::path::PathBuf;
use std::sync::Arc;

use aegis_config::Issue;
use aegis_core::diagnostics::Check;
use aegis_core::draft::ProfileDraft;
use aegis_core::registry::{self, Country, QuickConnectChoice, ServerHealth, ServerUpdate};
use aegis_core::{
    Capabilities, CoreError, Engine, LogEntry, ProfileId, ProfileView, Settings, Snapshot,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

use crate::backend::AnyBackend;

pub struct AppState {
    pub engine: Arc<Engine<AnyBackend>>,
    pub tray_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiError {
    kind: &'static str,
    message: String,
    issues: Vec<Issue>,
}

impl From<CoreError> for UiError {
    fn from(e: CoreError) -> Self {
        let kind = match &e {
            CoreError::Config(_) => "config",
            CoreError::NotFound => "not_found",
            CoreError::Busy => "busy",
            CoreError::Cancelled => "cancelled",
            CoreError::Timeout { .. } => "timeout",
            CoreError::Invalid { .. } => "invalid",
            CoreError::Unsupported { .. } => "unsupported",
            CoreError::PermissionDenied { .. } => "permission_denied",
            CoreError::Backend { .. } => "backend",
            CoreError::Storage { .. } => "storage",
        };
        let (message, issues) = match e {
            CoreError::Config(c) => {
                let n = c.issues.len();
                let first = c
                    .issues
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                let msg = if n > 1 {
                    format!("{first} (and {} more)", n - 1)
                } else {
                    first
                };
                (format!("Profile is not valid: {msg}"), c.issues)
            }
            other => (other.to_string(), vec![]),
        };
        Self {
            kind,
            message,
            issues,
        }
    }
}

impl UiError {
    fn msg(message: impl Into<String>) -> Self {
        Self {
            kind: "invalid",
            message: message.into(),
            issues: vec![],
        }
    }
}

type R<T> = Result<T, UiError>;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    tray_available: bool,
}

#[tauri::command]
pub fn app_info(st: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        tray_available: st.tray_available,
    }
}

#[tauri::command]
pub fn capabilities(st: State<'_, AppState>) -> Capabilities {
    st.engine.capabilities().clone()
}

#[tauri::command]
pub fn snapshot(st: State<'_, AppState>) -> Snapshot {
    st.engine.snapshot()
}

#[tauri::command]
pub fn events(st: State<'_, AppState>) -> Vec<LogEntry> {
    st.engine.events()
}

#[tauri::command]
pub fn profiles(st: State<'_, AppState>) -> Vec<ProfileView> {
    st.engine.profiles()
}

#[tauri::command]
pub fn country_catalog() -> Vec<Country> {
    registry::countries().to_vec()
}

#[tauri::command]
pub async fn update_server(
    st: State<'_, AppState>,
    id: ProfileId,
    update: ServerUpdate,
) -> R<ProfileView> {
    Ok(st.engine.update_server(id, update).await?)
}

#[tauri::command]
pub fn server_health(st: State<'_, AppState>) -> Vec<ServerHealth> {
    st.engine.server_health()
}

#[tauri::command]
pub async fn refresh_server_health(st: State<'_, AppState>) -> R<Vec<ServerHealth>> {
    Ok(st.engine.refresh_server_health().await?)
}

#[tauri::command]
pub async fn quick_connect(st: State<'_, AppState>) -> R<QuickConnectChoice> {
    Ok(st.engine.quick_connect().await?)
}

#[tauri::command]
pub fn settings(st: State<'_, AppState>) -> Settings {
    st.engine.settings()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, st: State<'_, AppState>, settings: Settings) -> R<Settings> {
    let before = st.engine.settings();
    let settings = st.engine.set_settings(settings)?;
    if settings.launch_on_login != before.launch_on_login {
        let al = app.autolaunch();
        let r = if settings.launch_on_login {
            al.enable()
        } else {
            al.disable()
        };
        if let Err(e) = r {
            st.engine.set_settings(before)?;
            return Err(UiError::msg(format!(
                "could not change start-at-login: {e}"
            )));
        }
    }
    Ok(settings)
}

#[tauri::command]
pub async fn connect(st: State<'_, AppState>, id: ProfileId) -> R<()> {
    Ok(st.engine.connect(id).await?)
}

#[tauri::command]
pub async fn disconnect(st: State<'_, AppState>) -> R<()> {
    Ok(st.engine.disconnect().await?)
}

#[tauri::command]
pub fn cancel(st: State<'_, AppState>) {
    st.engine.cancel();
}

async fn pick_conf_files(app: &AppHandle) -> Vec<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Import WireGuard profile")
        .add_filter("WireGuard profile", &["conf"])
        .pick_files(move |files| {
            let _ = tx.send(files);
        });
    rx.await
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .collect()
}

#[tauri::command]
pub async fn import_dialog(app: AppHandle, st: State<'_, AppState>) -> R<Vec<ProfileView>> {
    let mut out = Vec::new();
    for path in pick_conf_files(&app).await {
        out.push(st.engine.import_file(&path).await?);
    }
    Ok(out)
}

/// Import files dropped onto the window (paths come from the OS, not web content).
pub async fn import_dropped(app: AppHandle, engine: Arc<Engine<AnyBackend>>, paths: Vec<PathBuf>) {
    #[derive(Serialize, Clone)]
    struct ImportResult {
        name: String,
        error: Option<UiError>,
    }
    for p in paths {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let error = if p.extension().and_then(|e| e.to_str()) != Some("conf") {
            Some(UiError::msg("only WireGuard .conf files can be imported"))
        } else {
            engine.import_file(&p).await.err().map(UiError::from)
        };
        let _ = app.emit("import-result", ImportResult { name, error });
    }
}

#[tauri::command]
pub fn new_draft() -> ProfileDraft {
    ProfileDraft::blank()
}

#[tauri::command]
pub async fn profile_draft(st: State<'_, AppState>, id: ProfileId) -> R<ProfileDraft> {
    Ok(st.engine.draft(id).await?)
}

#[tauri::command]
pub async fn create_profile(st: State<'_, AppState>, draft: ProfileDraft) -> R<ProfileView> {
    Ok(st.engine.create(&draft).await?)
}

#[tauri::command]
pub async fn update_profile(
    st: State<'_, AppState>,
    id: ProfileId,
    draft: ProfileDraft,
) -> R<ProfileView> {
    Ok(st.engine.update(id, &draft).await?)
}

#[tauri::command]
pub async fn duplicate_profile(st: State<'_, AppState>, id: ProfileId) -> R<ProfileView> {
    Ok(st.engine.duplicate(id).await?)
}

#[tauri::command]
pub async fn delete_profile(st: State<'_, AppState>, id: ProfileId) -> R<()> {
    Ok(st.engine.delete(id).await?)
}

async fn save_path(app: &AppHandle, title: &str, name: &str, ext: &str) -> Option<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .set_file_name(name)
        .add_filter(ext, &[ext])
        .save_file(move |f| {
            let _ = tx.send(f);
        });
    rx.await.ok().flatten().and_then(|f| f.into_path().ok())
}

fn safe_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_ ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    s.trim().chars().take(48).collect::<String>()
}

/// The UI shows the private-key warning before calling this.
#[tauri::command]
pub async fn export_profile(app: AppHandle, st: State<'_, AppState>, id: ProfileId) -> R<bool> {
    let name = st
        .engine
        .profiles()
        .into_iter()
        .find(|p| p.meta.id == id)
        .map(|p| p.meta.name)
        .unwrap_or_default();
    let Some(path) = save_path(
        &app,
        "Export profile",
        &format!("{}.conf", safe_file_name(&name)),
        "conf",
    )
    .await
    else {
        return Ok(false);
    };
    st.engine.export(id, &path).await?;
    Ok(true)
}

#[tauri::command]
pub async fn diagnostics(st: State<'_, AppState>) -> R<Vec<Check>> {
    Ok(st.engine.diagnostics().await)
}

#[tauri::command]
pub async fn export_diagnostics(app: AppHandle, st: State<'_, AppState>) -> R<bool> {
    let Some(path) = save_path(&app, "Export diagnostics", "aegis-diagnostics.json", "json").await
    else {
        return Ok(false);
    };
    let text = aegis_core::diagnostics::bundle(&st.engine, env!("CARGO_PKG_VERSION")).await;
    aegis_core::store::write_private_atomic(&path, text.as_bytes())?;
    Ok(true)
}

/// Runs only when the user presses "Test speed". Contacts Cloudflare's public
/// speed endpoint; nothing is sent automatically.
#[tauri::command]
pub async fn speed_test() -> R<crate::speed::SpeedResult> {
    tokio::task::spawn_blocking(crate::speed::run)
        .await
        .map_err(|e| UiError {
            kind: "backend",
            message: format!("the speed test stopped unexpectedly: {e}"),
            issues: vec![],
        })
}

/// Opens one of the fixed onboarding links in the user's browser.
#[tauri::command]
pub fn open_link(which: String) -> R<()> {
    let url = crate::onboarding::link(&which)
        .ok_or_else(|| UiError::msg("unknown link"))?;
    crate::onboarding::open_in_browser(url).map_err(|_| UiError::msg("could not open your browser"))
}

#[derive(Serialize)]
pub struct ScanResult {
    pub imported: Vec<String>,
    pub failed: u32,
}

/// Imports new WireGuard `.conf` files from the Downloads folder.
#[tauri::command]
pub async fn scan_downloads(st: State<'_, AppState>, since_ms: u64) -> R<ScanResult> {
    let mut res = ScanResult { imported: vec![], failed: 0 };
    let Some(dir) = crate::onboarding::downloads_dir() else {
        return Ok(res);
    };
    for p in crate::onboarding::new_conf_files(&dir, since_ms) {
        match st.engine.import_file(&p).await {
            Ok(v) => res.imported.push(v.meta.name.clone()),
            Err(_) => res.failed += 1,
        }
    }
    Ok(res)
}
