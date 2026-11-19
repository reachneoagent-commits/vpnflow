//! Connection lifecycle and profile management on top of a [`VpnBackend`].
//!
//! Concurrency model: every mutating operation takes `op_lock` (one at a time,
//! so system state cannot be corrupted by interleaving). `cancel()` and
//! snapshots do not take it. A background monitor polls the backend while a
//! tunnel is active and drives reconnection.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aegis_config::{MAX_CONFIG_BYTES, WgConfig};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::backend::VpnBackend;
use crate::draft::{ProfileDraft, clean_name};
use crate::error::{CoreError, Result};
use crate::registry::{self, Reachability, ServerHealth, ServerMetadata, ServerUpdate};
use crate::retry::RetryPolicy;
use crate::store::{Store, write_private_atomic};
use crate::types::*;

const MAX_EVENTS: usize = 200;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Sent traffic with no reply for this long ⇒ `PeerState::NoResponse`.
const NO_RESPONSE_AFTER: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_secs(2);
/// Data sent, nothing received, for this long ⇒ the server stopped answering.
const SILENT_AFTER: Duration = Duration::from_secs(30);
/// Wait this long after an OS event before reacting, so bursts collapse into one.
const SETTLE_AFTER_EVENT: Duration = Duration::from_secs(3);
/// Network events this soon after our own activation are the tunnel's own noise.
const OWN_ACTIVATION_NOISE: Duration = Duration::from_secs(15);
/// One small DNS lookup through the tunnel per minute while connected.
const REACHABILITY_EVERY: Duration = Duration::from_secs(10);

struct Live {
    state: ConnectionState,
    stats: Option<Stats>,
    /// Result of the last active protection check for the connected tunnel.
    verified: Option<Protection>,
    /// Last reachability check for the connected tunnel: (at_ms, result).
    reachability: Option<(u64, Feature)>,
    /// Last public-IP observation: (at_ms, result). Cleared on disconnect.
    public_ip: Option<(u64, Feature)>,
    /// Kernel block state, or None when inspection failed and cleanup is required.
    blocking: Option<bool>,
    /// The profile the user is meant to be connected to: set once a tunnel
    /// came up, cleared by Disconnect/Cancel. Lets a resume from sleep retry a
    /// dropped session without reconnecting something the user never had up.
    session: Option<ProfileId>,
    /// Bumped when the path to the server may have changed (resume, network
    /// switch): proof of a response from before it no longer counts.
    path_epoch: u64,
    path_changed_ms: u64,
    /// When the tunnel last came up, to ignore the network noise it makes itself.
    activated_at: Option<tokio::time::Instant>,
    events: VecDeque<LogEntry>,
    profiles: Vec<ProfileMeta>,
    settings: Settings,
    /// Cancels the in-flight connect / reconnect loop.
    op_cancel: CancellationToken,
    /// Stops the monitor for the current session.
    monitor_cancel: CancellationToken,
}

pub struct Engine<B: VpnBackend> {
    backend: Arc<B>,
    store: Store,
    live: Mutex<Live>,
    op_lock: tokio::sync::Mutex<()>,
    tx: broadcast::Sender<EngineEvent>,
    caps: Capabilities,
}

/// Failed reconnect attempts on one server before trying backups.
const FALLBACK_AFTER: u32 = 3;

impl<B: VpnBackend> Engine<B> {
    /// Open the store, query the backend, and reconcile state left by a
    /// previous run (crash recovery).
    pub async fn start(backend: B, data_dir: &Path) -> Result<Arc<Self>> {
        let store = Store::open(data_dir)?;
        let profiles = store.profiles()?;
        let mut settings = store.settings()?;
        let caps = backend.capabilities().await?;
        // Default-on where supported, until the user makes their own choice.
        if !settings.kill_switch_chosen {
            let want = caps.kill_switch.is_supported();
            if settings.kill_switch != want {
                settings.kill_switch = want;
                store.save_settings(&settings)?;
            }
        }
        let (tx, _) = broadcast::channel(64);
        let engine = Arc::new(Self {
            backend: Arc::new(backend),
            store,
            live: Mutex::new(Live {
                state: ConnectionState::Disconnected,
                stats: None,
                verified: None,
                reachability: None,
                public_ip: None,
                blocking: Some(false),
                session: None,
                path_epoch: 0,
                path_changed_ms: 0,
                activated_at: None,
                events: VecDeque::new(),
                profiles,
                settings,
                op_cancel: CancellationToken::new(),
                monitor_cancel: CancellationToken::new(),
            }),
            op_lock: tokio::sync::Mutex::new(()),
            tx,
            caps,
        });
        engine.recover().await;
        if let Some(rx) = engine.backend.system_events() {
            let weak = Arc::downgrade(&engine);
            tokio::spawn(Self::system_event_loop(weak, rx));
        }
        Ok(engine)
    }

    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.tx.subscribe()
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    fn log(&self, level: Level, message: impl Into<String>) {
        let entry = LogEntry {
            at_ms: now_ms(),
            level,
            message: message.into(),
        };
        match level {
            Level::Info => tracing::info!(target: "aegis", "{}", entry.message),
            Level::Warn => tracing::warn!(target: "aegis", "{}", entry.message),
            Level::Error => tracing::error!(target: "aegis", "{}", entry.message),
        }
        {
            let mut l = self.live();
            if l.events.len() == MAX_EVENTS {
                l.events.pop_front();
            }
            l.events.push_back(entry.clone());
        }
        let _ = self.tx.send(EngineEvent::Log(entry));
    }

    fn set_state(&self, state: ConnectionState) {
        {
            let mut l = self.live();
            if l.state == state {
                return;
            }
            if state.is_idle() || matches!(state, ConnectionState::Uncertain { .. }) {
                l.stats = None;
            }
            if !matches!(state, ConnectionState::Connected { .. }) {
                l.verified = None;
                l.reachability = None;
                l.public_ip = None;
            }
            l.state = state;
        }
        self.publish();
    }

    fn publish(&self) {
        let _ = self
            .tx
            .send(EngineEvent::Snapshot(Box::new(self.snapshot())));
    }

    pub fn snapshot(&self) -> Snapshot {
        let l = self.live();
        let active = l.state.profile();
        let profile = active
            .and_then(|id| l.profiles.iter().find(|p| p.id == id))
            .map(|m| ProfileView {
                meta: m.clone(),
                preferred: l.settings.preferred_profile == Some(m.id),
                active: true,
            });
        let mut protection = match (&l.state, &profile) {
            (ConnectionState::Connected { .. }, Some(p)) => l
                .verified
                .clone()
                .unwrap_or_else(|| self.backend.protection(&p.meta.summary, &l.settings)),
            (ConnectionState::Uncertain { reason, .. }, _) => Protection {
                kill_switch: Feature::Failed(reason.clone()),
                dns: Feature::Failed(reason.clone()),
                ipv6: Feature::Failed(reason.clone()),
            },
            _ => Protection::off(),
        };
        protection.kill_switch = self.kill_switch_feature(&l, protection.kill_switch);
        let reachability = match (&l.state, &l.reachability) {
            (ConnectionState::Connected { .. }, Some((_, f))) => f.clone(),
            _ => Feature::Off,
        };
        let public_ip = match (&l.state, &l.public_ip) {
            (ConnectionState::Connected { .. }, Some((_, f))) => f.clone(),
            (ConnectionState::Connected { .. }, None) if l.settings.connectivity_check => {
                Feature::Configured("looking up egress address via the tunnel DNS".into())
            }
            _ => Feature::Off,
        };
        Snapshot {
            reachability,
            public_ip,
            health: Self::health(&l),
            state: l.state.clone(),
            protection,
            stats: l.stats.clone(),
            profile,
            simulated: self.caps.simulated,
        }
    }

    /// The kill switch is the engine's to report: only installed rules count.
    fn kill_switch_feature(&self, l: &Live, from_backend: Feature) -> Feature {
        if l.blocking.is_none() {
            return Feature::Failed(
                "firewall state is unknown; retry Disconnect to remove any block".into(),
            );
        }
        if l.blocking == Some(true) {
            return Feature::Verified(
                "the firewall block was confirmed; endpoint and network-control exceptions apply"
                    .into(),
            );
        }
        if !l.settings.kill_switch {
            return from_backend;
        }
        match (&self.caps.kill_switch, &l.state) {
            (Support::Unsupported(m), _) => Feature::Unsupported(m.clone()),
            (_, ConnectionState::Uncertain { reason, .. }) => Feature::Failed(reason.clone()),
            (
                _,
                ConnectionState::Connected { .. }
                | ConnectionState::Reconnecting { .. }
                | ConnectionState::Failed { .. },
            ) => Feature::Failed(
                "on in Settings but not blocking for this session; disconnect and connect again"
                    .into(),
            ),
            _ => Feature::Off,
        }
    }

    pub fn events(&self) -> Vec<LogEntry> {
        self.live().events.iter().cloned().collect()
    }

    // ---------- profiles ----------

    pub fn profiles(&self) -> Vec<ProfileView> {
        let l = self.live();
        let active = l.state.profile();
        l.profiles
            .iter()
            .map(|m| ProfileView {
                meta: m.clone(),
                preferred: l.settings.preferred_profile == Some(m.id),
                active: active == Some(m.id),
            })
            .collect()
    }

    fn meta(&self, id: ProfileId) -> Result<ProfileMeta> {
        self.live()
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or(CoreError::NotFound)
    }

    fn save_profiles(&self, f: impl FnOnce(&mut Vec<ProfileMeta>)) -> Result<()> {
        let mut next = self.live().profiles.clone();
        f(&mut next);
        self.store.save_profiles(&next)?;
        self.live().profiles = next;
        let _ = self.tx.send(EngineEvent::ProfilesChanged);
        Ok(())
    }

    fn unique_name(&self, wanted: &str) -> String {
        let l = self.live();
        let taken = |n: &str| l.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(n));
        if !taken(wanted) {
            return wanted.to_owned();
        }
        (2..)
            .map(|i| format!("{wanted} ({i})"))
            .find(|n| !taken(n))
            .expect("unbounded")
    }

    async fn add(&self, name: &str, cfg: &WgConfig, server: ServerMetadata) -> Result<ProfileView> {
        let name = self.unique_name(&clean_name(name).map_err(CoreError::invalid)?);
        let id = ProfileId::new();
        self.backend.install(id, &name, cfg).await?;
        let meta = ProfileMeta {
            id,
            name: name.clone(),
            created_ms: now_ms(),
            updated_ms: now_ms(),
            summary: ProfileSummary::from_config(cfg),
            server,
        };
        if let Err(e) = self.save_profiles(|v| v.push(meta.clone())) {
            // Roll back so the secret store holds nothing we do not track.
            let _ = self.backend.uninstall(id).await;
            return Err(e);
        }
        self.log(Level::Info, format!("Profile \"{name}\" added"));
        Ok(ProfileView {
            meta,
            preferred: false,
            active: false,
        })
    }

    pub async fn import_text(&self, name: &str, text: &str) -> Result<ProfileView> {
        let _g = self.op_lock.lock().await;
        let (cfg, _ipv4_only) = aegis_config::parse_import(text)?;
        let (name, server) = match registry::provider_label(text) {
            Some((label, location)) => (
                label,
                ServerMetadata {
                    location: Some(location),
                    ..Default::default()
                },
            ),
            None => (name.to_owned(), ServerMetadata::default()),
        };
        self.add(&name, &cfg, server).await
    }

    /// Import a user-selected file. The path must come from a native file
    /// dialog or drag-and-drop, never from web content.
    pub async fn import_file(&self, path: &Path) -> Result<ProfileView> {
        use std::io::Read as _;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            // Never block on a swapped FIFO or follow a replaced symlink.
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = options.open(path).map_err(CoreError::storage)?;
        let meta = file.metadata().map_err(CoreError::storage)?;
        if !meta.is_file() {
            return Err(CoreError::invalid("not a regular file"));
        }
        if meta.len() > MAX_CONFIG_BYTES as u64 {
            return Err(CoreError::invalid(
                "file is too large to be a WireGuard profile",
            ));
        }
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        file.take(MAX_CONFIG_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CoreError::invalid("file is not readable UTF-8 text"))?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(CoreError::invalid(
                "file is too large to be a WireGuard profile",
            ));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| CoreError::invalid("file is not readable UTF-8 text"))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported".into());
        self.import_text(&name, text).await
    }

    pub async fn create(&self, draft: &ProfileDraft) -> Result<ProfileView> {
        let _g = self.op_lock.lock().await;
        let cfg = draft.apply(None)?;
        self.add(
            &draft.name,
            &cfg,
            ServerMetadata {
                registration_pending: true,
                ..Default::default()
            },
        )
        .await
    }

    pub async fn draft(&self, id: ProfileId) -> Result<ProfileDraft> {
        let meta = self.meta(id)?;
        let cfg = self.backend.load(id).await?;
        Ok(ProfileDraft::from_config(&meta.name, &cfg))
    }

    pub async fn update(&self, id: ProfileId, draft: &ProfileDraft) -> Result<ProfileView> {
        let _g = self.op_lock.lock().await;
        let state = self.live().state.clone();
        if !state.is_idle() && state.profile().is_none_or(|active| active == id) {
            return Err(CoreError::invalid(
                "disconnect before editing the active profile",
            ));
        }
        let mut meta = self.meta(id)?;
        let existing = self.backend.load(id).await?;
        let cfg = draft.apply(Some(&existing))?;
        let name = clean_name(&draft.name).map_err(CoreError::invalid)?;
        let name = if name.eq_ignore_ascii_case(&meta.name) {
            name
        } else {
            self.unique_name(&name)
        };
        self.backend.install(id, &name, &cfg).await?;
        let previous_name = std::mem::replace(&mut meta.name, name);
        meta.summary = ProfileSummary::from_config(&cfg);
        meta.updated_ms = now_ms();
        meta.server.last_response_ms = None;
        meta.server.last_failure_ms = None;
        if draft.private_key == crate::draft::KeyAction::Generate
            || draft
                .peers
                .iter()
                .any(|p| p.preshared_key == crate::draft::KeyAction::Generate)
        {
            meta.server.registration_pending = true;
        }
        let m = meta.clone();
        if let Err(error) = self.save_profiles(|v| {
            if let Some(slot) = v.iter_mut().find(|p| p.id == id) {
                *slot = m;
            }
        }) {
            if let Err(rollback) = self.backend.install(id, &previous_name, &existing).await {
                return Err(CoreError::storage(format!(
                    "profile metadata write failed and the old configuration could not be restored ({rollback}); check Diagnostics before connecting"
                )));
            }
            return Err(error);
        }
        self.log(Level::Info, format!("Profile \"{}\" updated", meta.name));
        Ok(ProfileView {
            meta,
            preferred: false,
            active: false,
        })
    }

    pub async fn rename(&self, id: ProfileId, name: &str) -> Result<()> {
        let mut d = self.draft(id).await?;
        d.name = name.to_owned();
        self.update(id, &d).await.map(|_| ())
    }

    pub async fn duplicate(&self, id: ProfileId) -> Result<ProfileView> {
        let _g = self.op_lock.lock().await;
        let meta = self.meta(id)?;
        let cfg = self.backend.load(id).await?;
        self.add(
            &format!("{} copy", meta.name),
            &cfg,
            ServerMetadata {
                location: meta.server.location,
                enabled: meta.server.enabled,
                registration_pending: meta.server.registration_pending,
                ..Default::default()
            },
        )
        .await
    }

    pub async fn delete(&self, id: ProfileId) -> Result<()> {
        let _g = self.op_lock.lock().await;
        let meta = self.meta(id)?;
        let state = self.live().state.clone();
        if !state.is_idle() && state.profile().is_none_or(|active| active == id) {
            return Err(CoreError::invalid(
                "disconnect before deleting the active profile",
            ));
        }
        // Keep the registry entry until the backend confirms deletion.
        self.backend.uninstall(id).await?;
        self.save_profiles(|v| v.retain(|p| p.id != id))?;
        if self.live().settings.preferred_profile == Some(id) {
            let mut s = self.settings();
            s.preferred_profile = None;
            self.store.save_settings(&s)?;
            self.live().settings = s.clone();
            let _ = self.tx.send(EngineEvent::SettingsChanged(s));
        }
        self.log(Level::Info, format!("Profile \"{}\" deleted", meta.name));
        Ok(())
    }

    /// Write the full configuration, including private keys, to `path`.
    /// The UI must show an explicit warning first; the path must come from a
    /// native save dialog.
    pub async fn export(&self, id: ProfileId, path: &Path) -> Result<()> {
        let meta = self.meta(id)?;
        let cfg = self.backend.load(id).await?;
        write_private_atomic(path, cfg.to_wg_quick().as_bytes())?;
        self.log(
            Level::Warn,
            format!("Profile \"{}\" exported with its private key", meta.name),
        );
        Ok(())
    }

    // ---------- server registry (profile IDs, no credential copies) ----------

    pub async fn update_server(&self, id: ProfileId, update: ServerUpdate) -> Result<ProfileView> {
        let _g = self.op_lock.try_lock().map_err(|_| CoreError::Busy)?;
        self.update_server_locked(id, update)
    }

    fn update_server_locked(&self, id: ProfileId, mut update: ServerUpdate) -> Result<ProfileView> {
        self.meta(id)?;
        if let Some(l) = &mut update.location {
            l.validate()?;
        }
        let state = self.live().state.clone();
        if !state.is_idle()
            && state.profile().is_none_or(|active| active == id)
            && (!update.enabled || update.registration_pending)
        {
            return Err(CoreError::invalid(
                "disconnect before disabling the active server or marking its registration pending",
            ));
        }
        self.save_profiles(|profiles| {
            if let Some(p) = profiles.iter_mut().find(|p| p.id == id) {
                p.server.location = update.location;
                p.server.favorite = update.favorite;
                p.server.enabled = update.enabled;
                p.server.registration_pending = update.registration_pending;
                p.updated_ms = now_ms();
            }
        })?;
        self.publish();
        self.profiles()
            .into_iter()
            .find(|p| p.meta.id == id)
            .ok_or(CoreError::NotFound)
    }

    fn record_server(&self, id: ProfileId, f: impl FnOnce(&mut ServerMetadata)) {
        if let Err(e) = self.save_profiles(|profiles| {
            if let Some(p) = profiles.iter_mut().find(|p| p.id == id) {
                f(&mut p.server);
            }
        }) {
            self.log(Level::Warn, format!("Could not save server history: {e}"));
        }
    }

    pub fn server_health(&self) -> Vec<ServerHealth> {
        Self::health(&self.live())
    }

    fn health(l: &Live) -> Vec<ServerHealth> {
        let now = now_ms();
        l.profiles.iter().map(|p| {
            let responded = registry::recent(p.server.last_response_ms, now)
                && p.server.last_failure_ms.is_none();
            let no_response = matches!(l.state, ConnectionState::Connected { profile, peer: PeerState::NoResponse, .. } if profile == p.id);
            ServerHealth {
                id: p.id,
                reachability: if responded { Reachability::Responded } else if no_response { Reachability::NoResponse } else { Reachability::Unknown },
                observed_ms: p.server.last_response_ms,
                measured_rtt_ms: None,
                last_handshake_ms: if l.state.profile() == Some(p.id) { l.stats.as_ref().and_then(|s| s.last_handshake_ms) } else { None },
            }
        }).collect()
    }

    /// Refresh only the active tunnel. Never activate a tunnel to probe a server.
    pub async fn refresh_server_health(&self) -> Result<Vec<ServerHealth>> {
        let _g = self.op_lock.try_lock().map_err(|_| CoreError::Busy)?;
        if self.live().state.profile().is_some() {
            let status = self.read_status().await?;
            if let Some(status) = status {
                self.verify_protection(status.id, &status).await;
            }
        }
        self.publish();
        Ok(self.server_health())
    }

    pub async fn quick_connect(self: &Arc<Self>) -> Result<registry::QuickConnectChoice> {
        let _g = self.op_lock.try_lock().map_err(|_| CoreError::Busy)?;
        let choice = {
            let l = self.live();
            let candidates: Vec<_> = l.profiles.iter().filter(|p| {
                !matches!(l.state, ConnectionState::Connected { profile, peer: PeerState::NoResponse, .. } if profile == p.id)
            }).cloned().collect();
            registry::quick_connect(&candidates, &l.settings, now_ms())?
        };
        self.log(Level::Info, choice.reason.clone());
        self.connect_locked(choice.id).await?;
        Ok(choice)
    }

    // ---------- settings ----------

    pub fn settings(&self) -> Settings {
        self.live().settings.clone()
    }

    pub fn set_settings(&self, mut s: Settings) -> Result<Settings> {
        let _g = self.op_lock.try_lock().map_err(|_| CoreError::Busy)?;
        {
            let l = self.live();
            let changed = s.kill_switch != l.settings.kill_switch
                || s.kill_switch_allow_lan != l.settings.kill_switch_allow_lan;
            if s.kill_switch != l.settings.kill_switch {
                s.kill_switch_chosen = true;
            }
            if changed && (!l.state.is_idle() || l.blocking != Some(false)) {
                return Err(CoreError::invalid(
                    "disconnect before changing the kill switch; it applies from the next connection",
                ));
            }
        }
        if let Some(p) = s.preferred_profile
            && self.meta(p).is_err()
        {
            s.preferred_profile = None;
        }
        if s.favorite_countries.len() > registry::countries().len() {
            return Err(CoreError::invalid("too many favorite countries"));
        }
        for code in s
            .favorite_countries
            .iter()
            .chain(s.quick_connect_country.iter())
        {
            if registry::country(code).is_none() {
                return Err(CoreError::invalid("choose a valid ISO country"));
            }
        }
        s.favorite_countries.sort();
        s.favorite_countries.dedup();
        if s.bypass.len() > crate::types::MAX_BYPASS {
            return Err(CoreError::invalid("too many bypass entries"));
        }
        let mut nets = s
            .bypass
            .iter()
            .map(|e| crate::types::parse_bypass(e))
            .collect::<Result<Vec<_>>>()?;
        nets.sort();
        nets.dedup();
        s.bypass = nets.iter().map(ToString::to_string).collect();
        if s.dns_servers.len() > crate::types::MAX_DNS_SERVERS {
            return Err(CoreError::invalid("too many DNS servers"));
        }
        let mut dns = s
            .dns_servers
            .iter()
            .map(|e| crate::types::parse_dns_server(e))
            .collect::<Result<Vec<_>>>()?;
        dns.dedup();
        s.dns_servers = dns.iter().map(ToString::to_string).collect();
        if s.trusted_networks.len() > crate::types::MAX_TRUSTED {
            return Err(CoreError::invalid("too many trusted networks"));
        }
        for n in &mut s.trusted_networks {
            n.id = n.id.trim().to_ascii_lowercase();
            n.name = n.name.trim().chars().take(40).collect();
            if n.id.is_empty() || n.name.is_empty() {
                return Err(CoreError::invalid("a trusted network needs a name"));
            }
        }
        s.trusted_networks.dedup_by(|a, b| a.id == b.id);
        for t in [&mut s.schedule_connect, &mut s.schedule_disconnect] {
            if crate::types::parse_hhmm(t).is_none() {
                return Err(CoreError::invalid("schedule times look like 09:00"));
            }
            *t = t.trim().to_owned();
        }
        s.schedule_days.retain(|d| (1..=7).contains(d));
        s.schedule_days.sort_unstable();
        s.schedule_days.dedup();
        s.reconnect_max_attempts = s.reconnect_max_attempts.clamp(1, 50);
        let preferred_changed = self.live().settings.preferred_profile != s.preferred_profile;
        self.store.save_settings(&s)?;
        {
            let mut l = self.live();
            if !s.connectivity_check {
                l.reachability = None;
                l.public_ip = None;
            }
            l.settings = s.clone();
        }
        if preferred_changed {
            let _ = self.tx.send(EngineEvent::ProfilesChanged);
        }
        let _ = self.tx.send(EngineEvent::SettingsChanged(s.clone()));
        self.publish();
        Ok(s)
    }

    // ---------- connection ----------

    /// Fail closed: a requested protection that the backend cannot provide
    /// blocks the connection instead of being silently downgraded.
    fn check_requested_protection(&self) -> Result<()> {
        if self.live().settings.kill_switch && !self.caps.kill_switch.is_supported() {
            let why = match &self.caps.kill_switch {
                Support::Unsupported(m) => m.clone(),
                Support::Supported => String::new(),
            };
            return Err(CoreError::unsupported(format!(
                "the kill switch is enabled in Settings but is not available on this system ({why}). \
                 Not connecting, to avoid running without the protection you asked for. \
                 Disable the kill switch in Settings to connect without it."
            )));
        }
        Ok(())
    }

    fn check_session_protection(&self) -> Result<()> {
        self.check_requested_protection()?;
        let l = self.live();
        if l.blocking.is_none() {
            return Err(CoreError::invalid(
                "firewall state could not be confirmed; retry Disconnect",
            ));
        }
        if l.settings.kill_switch && l.blocking != Some(true) {
            return Err(CoreError::invalid(
                "the requested kill switch is not blocking traffic; disconnect and connect again to install it",
            ));
        }
        Ok(())
    }

    pub async fn connect(self: &Arc<Self>, id: ProfileId) -> Result<()> {
        let _g = self.op_lock.try_lock().map_err(|_| CoreError::Busy)?;
        self.connect_locked(id).await
    }

    async fn connect_locked(self: &Arc<Self>, id: ProfileId) -> Result<()> {
        let meta = self.meta(id)?;
        let current = self.live().state.clone();
        if matches!(current, ConnectionState::Uncertain { profile: None, .. }) {
            return Err(CoreError::invalid(
                "tunnel status is unknown; wait for recovery or retry Disconnect before connecting",
            ));
        }
        self.check_requested_protection()?;
        if let ConnectionState::Connected { profile, .. } = current
            && profile == id
        {
            return Ok(());
        }
        if !self.caps.available.is_supported() {
            return Err(CoreError::unsupported(
                "no usable VPN backend; see Diagnostics",
            ));
        }

        if !registry::eligible(&meta) {
            return Err(CoreError::invalid(
                "server is disabled, missing an endpoint, or awaiting remote key registration; edit it in Locations",
            ));
        }
        let cancel = CancellationToken::new();
        self.live().op_cancel = cancel.clone();
        // Preflight must finish before touching the working tunnel, and cancellation
        // during a permission prompt must not be forgotten when preflight finishes.
        let cfg = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            result = tokio::time::timeout(CONNECT_TIMEOUT, self.backend.load(id)) => {
                let loaded = result.map_err(|_| CoreError::Timeout {
                    what: "loading destination profile".into(),
                    seconds: CONNECT_TIMEOUT.as_secs(),
                })?;
                match loaded {
                    Err(CoreError::NotFound) => {
                        return Err(CoreError::invalid(if self.caps.simulated {
                            "this profile's WireGuard config is not in the simulated backend. \
                             Re-import the .conf, or relaunch without AEGIS_BACKEND=mock to use \
                             your NetworkManager profiles."
                        } else {
                            "this profile's WireGuard config is missing from the system store; \
                             re-import the .conf"
                        }));
                    }
                    other => other?,
                }
            }
        };
        let issues = aegis_config::validate(&cfg);
        if !issues.is_empty() {
            return Err(aegis_config::ConfigError { issues }.into());
        }
        let settings = self.settings();
        let block = if settings.kill_switch {
            Some(kill_switch_endpoints(&cfg)?)
        } else {
            None
        };
        if !current.is_idle() && current.profile().is_some() && settings.kill_switch {
            return Err(CoreError::unsupported(
                "protected switching is unavailable; keeping the current tunnel",
            ));
        }
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        if let Some(prev) = current.profile() {
            self.teardown(prev).await?;
        }
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        self.record_server(id, |s| s.last_attempt_ms = Some(now_ms()));
        self.set_state(ConnectionState::Preparing { profile: id });
        self.log(Level::Info, format!("Connecting to \"{}\"", meta.name));
        // The block goes in before the tunnel exists, so nothing can slip out
        // while it comes up. Endpoints are IP literals, so NM needs no DNS.
        let bypass = settings
            .bypass
            .iter()
            .filter_map(|e| crate::types::parse_bypass(e).ok())
            .collect::<Vec<_>>();
        if let Err(e) = self.backend.apply_bypass(&bypass).await {
            self.log(Level::Error, format!("Bypass list not applied: {e}"));
            self.set_state(ConnectionState::Failed {
                profile: Some(id),
                reason: format!("could not prepare the connection: {e}"),
            });
            return Err(e);
        }
        let dns = settings
            .dns_servers
            .iter()
            .filter_map(|e| crate::types::parse_dns_server(e).ok())
            .map(std::net::IpAddr::V4)
            .collect::<Vec<_>>();
        if let Err(e) = self.backend.apply_dns(&dns).await {
            self.log(Level::Error, format!("Custom DNS not applied: {e}"));
            self.set_state(ConnectionState::Failed {
                profile: Some(id),
                reason: format!("could not prepare the connection: {e}"),
            });
            return Err(e);
        }
        match &block {
            Some(endpoints) => {
                if let Err(e) = self
                    .backend
                    .apply_kill_switch(id, endpoints, settings.kill_switch_allow_lan)
                    .await
                {
                    let blocking = self.backend.kill_switch_active().await.ok();
                    self.live().blocking = blocking;
                    self.log(
                        Level::Error,
                        format!("Kill switch could not be turned on: {e}"),
                    );
                    let reason = format!(
                        "the kill switch could not be turned on, so Aegis did not connect: {e}"
                    );
                    self.set_state(if blocking.is_none() {
                        ConnectionState::Uncertain {
                            profile: Some(id),
                            reason,
                        }
                    } else {
                        ConnectionState::Failed {
                            profile: Some(id),
                            reason,
                        }
                    });
                    return Err(e);
                }
                self.live().blocking = Some(true);
                self.log(
                    Level::Info,
                    "Kill switch on: traffic outside the tunnel is blocked",
                );
            }
            // A block left from an earlier session ends when connecting without one.
            None => {
                if let Err(e) = self.end_block().await {
                    self.set_state(ConnectionState::Failed {
                        profile: Some(id),
                        reason: format!("could not prepare the connection: {e}"),
                    });
                    return Err(e);
                }
            }
        }
        self.set_state(ConnectionState::Connecting { profile: id });

        match self.activate_once(id, &cancel).await {
            Ok(()) => {
                self.on_connected(id);
                Ok(())
            }
            Err(e) => {
                if !matches!(e, CoreError::Cancelled) {
                    self.record_server(id, |s| s.last_failure_ms = Some(now_ms()));
                }
                if let Err(cleanup) = self.backend.deactivate(id).await {
                    self.set_state(ConnectionState::Uncertain {
                        profile: Some(id),
                        reason: format!("activation failed ({e}); cleanup failed ({cleanup}); tunnel may still be active"),
                    });
                    return Err(cleanup);
                }
                if matches!(e, CoreError::Cancelled) {
                    self.log(Level::Info, "Connection cancelled");
                    self.set_state(ConnectionState::Disconnected);
                    // Cancel ends the session; a failed attempt does not, so the
                    // block stays until the user disconnects.
                    let _ = self.end_block().await;
                    self.live().session = None;
                } else {
                    match self.try_fallback(id, &cancel).await {
                        Ok(Some(_)) => return Ok(()),
                        Err(CoreError::Cancelled) => {
                            self.log(Level::Info, "Connection cancelled");
                            self.set_state(ConnectionState::Disconnected);
                            let _ = self.end_block().await;
                            self.live().session = None;
                            return Err(CoreError::Cancelled);
                        }
                        _ => {}
                    }
                    self.log(
                        Level::Error,
                        format!("Connection to \"{}\" failed: {e}", meta.name),
                    );
                    self.set_state(ConnectionState::Failed {
                        profile: Some(id),
                        reason: e.to_string(),
                    });
                }
                Err(e)
            }
        }
    }

    /// The server stopped responding: try other servers in the same country,
    /// one at a time. The kill switch block is replaced atomically for each
    /// new endpoint, so nothing leaks while switching. Returns the server now
    /// connected, or `None` when no backup worked (the block then points back
    /// at the original server).
    async fn try_fallback(
        self: &Arc<Self>,
        failed: ProfileId,
        cancel: &CancellationToken,
    ) -> Result<Option<ProfileId>> {
        let settings = self.settings();
        if !settings.auto_switch_server {
            return Ok(None);
        }
        let candidates = {
            let l = self.live();
            registry::fallback_candidates(&l.profiles, failed, now_ms())
        };
        if candidates.is_empty() {
            return Ok(None);
        }
        let from = self.meta(failed)?.name;
        let mut moved = false;
        let mut found = None;
        for cand in candidates {
            if cancel.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let Ok(meta) = self.meta(cand) else { continue };
            let _ = self.backend.deactivate(failed).await;
            let cfg = match tokio::time::timeout(CONNECT_TIMEOUT, self.backend.load(cand)).await {
                Ok(Ok(cfg)) if aegis_config::validate(&cfg).is_empty() => cfg,
                _ => continue,
            };
            if settings.kill_switch {
                let Ok(endpoints) = kill_switch_endpoints(&cfg) else {
                    continue;
                };
                if let Err(e) = self
                    .backend
                    .apply_kill_switch(cand, &endpoints, settings.kill_switch_allow_lan)
                    .await
                {
                    self.log(
                        Level::Warn,
                        format!("Could not prepare a backup server: {e}"),
                    );
                    self.live().blocking = self.backend.kill_switch_active().await.ok();
                    break;
                }
                self.live().blocking = Some(true);
                moved = true;
            }
            self.log(
                Level::Info,
                format!(
                    "\"{from}\" is not responding. Trying \"{}\" in the same country",
                    meta.name
                ),
            );
            self.record_server(cand, |s| s.last_attempt_ms = Some(now_ms()));
            self.set_state(ConnectionState::Connecting { profile: cand });
            match self.activate_once(cand, cancel).await {
                Ok(()) => {
                    self.log(
                        Level::Warn,
                        format!("Switched from \"{from}\" to \"{}\" because the first server stopped responding", meta.name),
                    );
                    self.on_connected(cand);
                    found = Some(cand);
                    break;
                }
                Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                Err(e) => {
                    self.record_server(cand, |s| s.last_failure_ms = Some(now_ms()));
                    let _ = self.backend.deactivate(cand).await;
                    self.log(
                        Level::Warn,
                        format!("\"{}\" did not respond either: {e}", meta.name),
                    );
                }
            }
        }
        if found.is_none() && moved && settings.kill_switch {
            // Point the block back at the original server so retries can reach it.
            let cfg = self.backend.load(failed).await?;
            let endpoints = kill_switch_endpoints(&cfg)?;
            self.backend
                .apply_kill_switch(failed, &endpoints, settings.kill_switch_allow_lan)
                .await?;
        }
        Ok(found)
    }

    async fn activate_once(&self, id: ProfileId, cancel: &CancellationToken) -> Result<()> {
        self.check_session_protection()?;
        tokio::select! {
            _ = cancel.cancelled() => Err(CoreError::Cancelled),
            r = tokio::time::timeout(CONNECT_TIMEOUT, async {
                self.refresh_firewall().await?;
                self.backend.activate(id).await?;
                self.refresh_firewall().await?;
                match self.backend.status().await? {
                    Some(s) if s.id == id && s.link == LinkState::Up => Ok(()),
                    _ => Err(CoreError::backend("backend did not confirm the selected tunnel is up")),
                }
            }) => match r {
                Ok(r) => r,
                Err(_) => Err(CoreError::Timeout { what: "the tunnel did not come up".into(), seconds: CONNECT_TIMEOUT.as_secs() }),
            },
        }
    }

    async fn refresh_firewall(&self) -> Result<()> {
        let expected = {
            let l = self.live();
            self.caps.kill_switch.is_supported()
                && (l.settings.kill_switch || l.blocking != Some(false))
        };
        if expected {
            let active = tokio::time::timeout(POLL, self.backend.kill_switch_active())
                .await
                .map_err(|_| CoreError::Timeout {
                    what: "checking firewall state".into(),
                    seconds: POLL.as_secs(),
                })
                .and_then(|result| result);
            self.live().blocking = active.as_ref().ok().copied();
            active?;
        }
        self.check_session_protection()
    }

    fn on_connected(self: &Arc<Self>, id: ProfileId) {
        {
            let mut l = self.live();
            l.session = Some(id);
            l.activated_at = Some(tokio::time::Instant::now());
        }
        match self.check_session_protection() {
            Ok(()) => {
                self.set_state(ConnectionState::Connected {
                    profile: id,
                    since_ms: now_ms(),
                    peer: PeerState::Pending,
                });
                self.log(
                    Level::Info,
                    "Tunnel is up; waiting for the server to respond",
                );
            }
            Err(e) => {
                self.set_state(ConnectionState::Uncertain {
                    profile: Some(id),
                    reason: format!(
                        "existing tunnel protection is not confirmed: {e}; retry Disconnect"
                    ),
                });
                self.log(
                    Level::Warn,
                    "Existing tunnel kept active, but requested protection is not active",
                );
            }
        }
        self.start_monitor(Some(id));
    }

    fn start_monitor(self: &Arc<Self>, id: Option<ProfileId>) {
        let token = CancellationToken::new();
        self.live().monitor_cancel = token.clone();
        let me = Arc::clone(self);
        tokio::spawn(async move { me.monitor(id, token).await });
    }

    /// Cancel an in-flight connect or reconnect loop.
    pub fn cancel(&self) {
        self.live().op_cancel.cancel();
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.cancel(); // stop a pending connect/reconnect first
        self.live().session = None; // the user ended it; a resume must not bring it back
        let _g = self.op_lock.lock().await;
        let state = self.live().state.clone();
        let id = match state.profile() {
            Some(id) => Some(id),
            None if matches!(state, ConnectionState::Uncertain { .. }) => {
                match self.read_status().await? {
                    Some(s) => Some(s.id),
                    None => {
                        self.live().monitor_cancel.cancel();
                        self.set_state(ConnectionState::Disconnected);
                        None
                    }
                }
            }
            None => None,
        };
        if let Some(id) = id {
            self.teardown(id).await?;
        } else if matches!(state, ConnectionState::Failed { .. }) {
            self.set_state(ConnectionState::Disconnected);
        }
        // Disconnect is what ends a kill-switch session, whatever state it was in.
        self.end_block().await
    }

    /// Remove the kill-switch block if one is installed.
    async fn end_block(&self) -> Result<()> {
        if self.live().blocking == Some(false) {
            return Ok(());
        }
        match self.backend.remove_kill_switch().await {
            Ok(()) => {
                self.live().blocking = Some(false);
                self.log(
                    Level::Info,
                    "Kill switch off: traffic outside the tunnel is allowed again",
                );
                self.publish();
                Ok(())
            }
            Err(e) => {
                let blocking = self.backend.kill_switch_active().await.ok();
                self.live().blocking = blocking;
                if blocking.is_none() {
                    let profile = self.live().state.profile();
                    self.set_state(ConnectionState::Uncertain {
                        profile,
                        reason: "firewall cleanup was not confirmed; retry Disconnect".into(),
                    });
                }
                self.log(
                    Level::Error,
                    format!("Kill-switch cleanup reported an error: {e}. Check its current status and retry Disconnect if needed."),
                );
                self.publish();
                Err(e)
            }
        }
    }

    async fn teardown(&self, id: ProfileId) -> Result<()> {
        self.live().monitor_cancel.cancel();
        self.set_state(ConnectionState::Disconnecting { profile: id });
        match self.backend.deactivate(id).await {
            Ok(()) => {
                self.log(Level::Info, "Disconnected");
                self.set_state(ConnectionState::Disconnected);
                Ok(())
            }
            Err(e) => {
                self.log(Level::Error, format!("Disconnect failed: {e}"));
                self.set_state(ConnectionState::Uncertain {
                    profile: Some(id),
                    reason: format!(
                        "disconnect failed: {e}; tunnel may still be active; retry Disconnect"
                    ),
                });
                Err(e)
            }
        }
    }

    async fn read_status(&self) -> Result<Option<TunnelStatus>> {
        tokio::time::timeout(CONNECT_TIMEOUT, self.backend.status())
            .await
            .unwrap_or_else(|_| {
                Err(CoreError::Timeout {
                    what: "reading tunnel status".into(),
                    seconds: CONNECT_TIMEOUT.as_secs(),
                })
            })
    }

    async fn monitor(self: Arc<Self>, id: Option<ProfileId>, stop: CancellationToken) {
        let mut prev: Option<(u64, u64, u64)> = None; // rx, tx, at_ms
        let mut first_tx_ms: Option<u64> = None;
        let mut recorded_success = false;
        // Evidence from before the last path change (resume, network switch)
        // does not count: a response must arrive after it.
        let (mut seen_epoch, mut base_rx, mut base_tx, mut event_ms) = (0u64, 0u64, 0u64, 0u64);
        // A live server answers data within seconds (WireGuard sends a
        // keepalive after 10 s). Sending with nothing coming back means the
        // path or the server is gone, even after an earlier response.
        let (mut rx_seen, mut tx_at_rx, mut rx_moved_at) =
            (0u64, 0u64, tokio::time::Instant::now());
        let since_ms = match self.live().state {
            ConnectionState::Connected { since_ms, .. } => since_ms,
            _ => now_ms(),
        };
        loop {
            tokio::select! {
                _ = stop.cancelled() => return,
                _ = tokio::time::sleep(POLL) => {}
            }
            let status = self.read_status().await;
            let observation = self.op_lock.lock().await;
            if stop.is_cancelled() {
                return;
            }
            if let Some(id) = id
                && self.caps.kill_switch.is_supported()
                && let Err(e) = self.refresh_firewall().await
            {
                self.live().session = None;
                self.log(
                    Level::Error,
                    format!(
                        "Stopping tunnel because firewall protection could not be confirmed: {e}"
                    ),
                );
                if self.teardown(id).await.is_ok() {
                    self.set_state(ConnectionState::Failed {
                        profile: Some(id),
                        reason: "Firewall protection could not be confirmed. Check the helper and connect again, or retry Disconnect to clear a remaining block.".into(),
                    });
                }
                return;
            }
            let Some(id) = id else {
                match status {
                    Ok(Some(s)) if self.meta(s.id).is_ok() && s.link == LinkState::Up => {
                        self.on_connected(s.id);
                        return;
                    }
                    Ok(None) if self.live().blocking.is_none() => {}
                    Ok(None) => {
                        self.set_state(ConnectionState::Disconnected);
                        return;
                    }
                    Err(e) => self.log(
                        Level::Warn,
                        format!("Could not reconcile tunnel status: {e}"),
                    ),
                    _ => {}
                }
                continue;
            };
            match status {
                Ok(Some(s)) if s.id == id && s.link == LinkState::Up => {
                    let now = now_ms();
                    let (rx_rate, tx_rate) = match prev {
                        Some((rx, tx, at)) if now > at => {
                            let dt = (now - at) as f64 / 1000.0;
                            (
                                s.rx_bytes.saturating_sub(rx) as f64 / dt,
                                s.tx_bytes.saturating_sub(tx) as f64 / dt,
                            )
                        }
                        _ => (0.0, 0.0),
                    };
                    let fresh_response = prev.is_some_and(|(rx, _, _)| s.rx_bytes > rx)
                        || s.last_handshake_ms
                            .is_some_and(|at| at <= now && now - at < 180_000);
                    prev = Some((s.rx_bytes, s.tx_bytes, now));
                    if fresh_response && !recorded_success {
                        self.record_server(id, |server| {
                            server.last_connected_ms = Some(now);
                            server.last_response_ms = Some(now);
                            server.last_failure_ms = None;
                        });
                        recorded_success = true;
                    }
                    let (epoch, changed_at) = {
                        let l = self.live();
                        (l.path_epoch, l.path_changed_ms)
                    };
                    if epoch != seen_epoch {
                        (seen_epoch, base_rx, base_tx, event_ms) =
                            (epoch, s.rx_bytes, s.tx_bytes, changed_at);
                        first_tx_ms = None;
                    }
                    if s.tx_bytes > base_tx && first_tx_ms.is_none() {
                        first_tx_ms = Some(now);
                    }
                    let confirmed = if seen_epoch == 0 {
                        s.last_handshake_ms.is_some() || s.rx_bytes > 0
                    } else {
                        s.last_handshake_ms.is_some_and(|h| h > event_ms) || s.rx_bytes > base_rx
                    };
                    if s.rx_bytes != rx_seen {
                        (rx_seen, tx_at_rx, rx_moved_at) =
                            (s.rx_bytes, s.tx_bytes, tokio::time::Instant::now());
                    }
                    let silent = s.tx_bytes > tx_at_rx && rx_moved_at.elapsed() > SILENT_AFTER;
                    let peer = if silent {
                        PeerState::NoResponse
                    } else if confirmed {
                        PeerState::Confirmed
                    } else if first_tx_ms
                        .is_some_and(|t| now - t > NO_RESPONSE_AFTER.as_millis() as u64)
                    {
                        PeerState::NoResponse
                    } else {
                        PeerState::Pending
                    };
                    // A received-data observation has no exact handshake timestamp.
                    let last_response_ms = s.last_handshake_ms.or_else(|| {
                        if confirmed {
                            Some(
                                self.live()
                                    .profiles
                                    .iter()
                                    .find(|p| p.id == id)
                                    .and_then(|p| p.server.last_response_ms)
                                    .unwrap_or(now),
                            )
                        } else {
                            None
                        }
                    });
                    let mut changed = None;
                    let mut was = PeerState::Pending;
                    {
                        let mut l = self.live();
                        l.stats = Some(Stats {
                            rx_bytes: s.rx_bytes,
                            tx_bytes: s.tx_bytes,
                            rx_rate_bps: rx_rate,
                            tx_rate_bps: tx_rate,
                            last_handshake_ms: s.last_handshake_ms,
                            last_response_ms,
                            interface: s.interface.clone(),
                            sampled_ms: now,
                        });
                        if let ConnectionState::Connected { peer: old, .. } = &mut l.state
                            && *old != peer
                        {
                            was = *old;
                            *old = peer;
                            changed = Some(peer);
                        }
                    }
                    if fresh_response {
                        let mut l = self.live();
                        if let Some(p) = l.profiles.iter_mut().find(|p| p.id == id) {
                            p.server.last_response_ms = Some(now);
                        }
                    }
                    match changed {
                        Some(PeerState::Confirmed) => {
                            self.log(Level::Info, "Server responded; tunnel confirmed")
                        }
                        Some(PeerState::NoResponse) if was == PeerState::Confirmed => {
                            self.record_server(id, |server| server.last_failure_ms = Some(now));
                            self.log(
                                Level::Warn,
                                "The server stopped answering. The network may be down or you may have switched networks; Aegis keeps the tunnel and will confirm it again when the server replies.",
                            );
                        }
                        Some(PeerState::NoResponse) => {
                            self.record_server(id, |server| server.last_failure_ms = Some(now));
                            self.log(Level::Warn, "No response from the server yet. Check the endpoint, keys, and that UDP is not blocked.");
                        }
                        _ => {}
                    }
                    self.verify_protection(id, &s).await;
                    let uncertain = matches!(self.live().state, ConnectionState::Uncertain { .. });
                    if uncertain && self.check_session_protection().is_ok() {
                        self.set_state(ConnectionState::Connected {
                            profile: id,
                            since_ms,
                            peer,
                        });
                    }
                    self.publish();
                }
                Ok(_) => {
                    self.log(Level::Warn, "Tunnel went down unexpectedly");
                    drop(observation);
                    self.reconnect_loop(id, stop).await;
                    return;
                }
                Err(e) => {
                    self.log(Level::Warn, format!("Could not read tunnel status: {e}"));
                    self.set_state(ConnectionState::Uncertain { profile: Some(id), reason: "cannot read tunnel status; protection is unknown; check NetworkManager or retry Disconnect".into() });
                }
            }
        }
    }

    /// Run the backend's active protection check and log changes worth telling the user.
    async fn verify_protection(&self, id: ProfileId, status: &TunnelStatus) {
        let Some((summary, settings)) = ({
            let l = self.live();
            l.profiles
                .iter()
                .find(|p| p.id == id)
                .map(|p| (p.summary.clone(), l.settings.clone()))
        }) else {
            return;
        };
        // Probing through the tunnel before the server has answered races the
        // WireGuard handshake (its retry is 5 s), so a healthy server would be
        // reported unreachable. Wait for peer confirmation; the next poll runs it.
        let confirmed = matches!(
            self.live().state,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        );
        let rechecking = self.live().reachability.is_some();
        if settings.connectivity_check && (confirmed || rechecking) {
            self.check_reachability(&summary, status).await;
            self.check_public_ip(&summary, status).await;
        }
        let new = self
            .backend
            .verify_protection(&summary, &settings, status)
            .await;
        let old = {
            let mut l = self.live();
            if !matches!(l.state, ConnectionState::Connected { .. }) {
                return;
            }
            l.verified.replace(new.clone())
        };
        for (name, before, after) in [
            (
                "DNS leak protection",
                old.as_ref().map(|p| &p.dns),
                &new.dns,
            ),
            ("IPv6 protection", old.as_ref().map(|p| &p.ipv6), &new.ipv6),
        ] {
            if before.map(std::mem::discriminant) == Some(std::mem::discriminant(after)) {
                continue;
            }
            match after {
                Feature::Verified(d) => self.log(Level::Info, format!("{name} verified: {d}")),
                Feature::Failed(d) => self.log(Level::Warn, format!("{name} check failed: {d}")),
                _ => {}
            }
        }
    }

    /// Reachability on connect, then at most once per [`REACHABILITY_EVERY`].
    async fn check_reachability(&self, summary: &ProfileSummary, status: &TunnelStatus) {
        let due = self.live().reachability.as_ref().is_none_or(|(at, _)| {
            now_ms().saturating_sub(*at) >= REACHABILITY_EVERY.as_millis() as u64
        });
        if !due {
            return;
        }
        let new = self.backend.check_reachability(summary, status).await;
        let old = {
            let mut l = self.live();
            if !matches!(l.state, ConnectionState::Connected { .. }) {
                return;
            }
            l.reachability
                .replace((now_ms(), new.clone()))
                .map(|(_, f)| f)
        };
        if old.as_ref().map(std::mem::discriminant) != Some(std::mem::discriminant(&new)) {
            match &new {
                Feature::Verified(d) => {
                    self.log(Level::Info, format!("Reachability check passed: {d}"))
                }
                Feature::Failed(d) => {
                    self.log(Level::Warn, format!("Reachability check failed: {d}"))
                }
                _ => {}
            }
        }
    }

    /// Public egress IP once per connect (retry on the reachability interval after failure).
    async fn check_public_ip(&self, summary: &ProfileSummary, status: &TunnelStatus) {
        let due = match self.live().public_ip.as_ref() {
            None => true,
            Some((_, Feature::Verified(_))) => false,
            Some((at, _)) => now_ms().saturating_sub(*at) >= REACHABILITY_EVERY.as_millis() as u64,
        };
        if !due {
            return;
        }
        let new = self.backend.check_public_ip(summary, status).await;
        let old = {
            let mut l = self.live();
            if !matches!(l.state, ConnectionState::Connected { .. }) {
                return;
            }
            l.public_ip.replace((now_ms(), new.clone())).map(|(_, f)| f)
        };
        if old.as_ref().map(std::mem::discriminant) != Some(std::mem::discriminant(&new)) {
            match &new {
                Feature::Verified(ip) => {
                    self.log(Level::Info, format!("Public IP via tunnel DNS: {ip}"))
                }
                Feature::Failed(d) => {
                    self.log(Level::Warn, format!("Public IP lookup failed: {d}"))
                }
                Feature::Unsupported(d) => {
                    self.log(Level::Info, format!("Public IP not available: {d}"))
                }
                _ => {}
            }
        }
    }

    /// Reacts to OS events (resume from sleep, network switch) from the backend.
    /// A burst of events (Wi-Fi roaming emits several) is handled once, after
    /// the network has had a moment to settle.
    async fn system_event_loop(
        weak: std::sync::Weak<Self>,
        mut rx: tokio::sync::mpsc::Receiver<SystemEvent>,
    ) {
        while let Some(first) = rx.recv().await {
            let (mut resumed, mut changed) = (
                first == SystemEvent::Resumed,
                first == SystemEvent::NetworkChanged,
            );
            let settle = tokio::time::sleep(SETTLE_AFTER_EVENT);
            tokio::pin!(settle);
            loop {
                tokio::select! {
                    _ = &mut settle => break,
                    e = rx.recv() => match e {
                        Some(SystemEvent::Resumed) => resumed = true,
                        Some(SystemEvent::NetworkChanged) => changed = true,
                        None => return,
                    },
                }
            }
            let Some(me) = weak.upgrade() else { return };
            if resumed {
                me.on_resume().await;
            } else if changed {
                me.on_network_changed();
            }
        }
    }

    /// The path to the server may have changed: forget proof from before it.
    /// Returns whether a session was affected.
    fn mark_path_changed(&self) -> bool {
        let mut l = self.live();
        let affected = matches!(
            l.state,
            ConnectionState::Connected { .. } | ConnectionState::Reconnecting { .. }
        );
        if !affected {
            return false;
        }
        l.path_epoch += 1;
        l.path_changed_ms = now_ms();
        if let ConnectionState::Connected { peer, .. } = &mut l.state {
            *peer = PeerState::Pending;
        }
        l.verified = None;
        l.reachability = None;
        l.public_ip = None;
        true
    }

    fn on_network_changed(&self) {
        // Bringing the tunnel up changes NetworkManager's state itself.
        if self
            .live()
            .activated_at
            .is_some_and(|t| t.elapsed() < OWN_ACTIVATION_NOISE)
        {
            return;
        }
        if self.mark_path_changed() {
            self.log(
                Level::Info,
                "The network changed; checking that the tunnel still works",
            );
            self.publish();
        }
    }

    async fn on_resume(self: &Arc<Self>) {
        self.log(Level::Info, "The computer woke from sleep");
        if self.mark_path_changed() {
            self.publish();
        }
        // A session that ran out of retries while asleep gets one fresh start.
        let (state, session, auto) = {
            let l = self.live();
            (l.state.clone(), l.session, l.settings.auto_reconnect)
        };
        if let (ConnectionState::Failed { .. }, Some(id), true) = (state, session, auto) {
            self.log(Level::Info, "Reconnecting after sleep");
            if let Err(e) = self.connect(id).await {
                self.log(Level::Warn, format!("Could not reconnect after sleep: {e}"));
            }
        }
    }

    async fn reconnect_loop(self: Arc<Self>, id: ProfileId, stop: CancellationToken) {
        let guard = self.op_lock.lock().await;
        if stop.is_cancelled() {
            return;
        }
        self.record_server(id, |s| s.last_failure_ms = Some(now_ms()));
        let settings = self.settings();
        if !settings.auto_reconnect {
            self.set_state(ConnectionState::Failed {
                profile: Some(id),
                reason: "tunnel went down and automatic reconnection is off".into(),
            });
            return;
        }
        let cancel = CancellationToken::new();
        self.live().op_cancel = cancel.clone();
        drop(guard);
        let policy = RetryPolicy::new(settings.reconnect_max_attempts);
        let mut attempt = 1;
        let mut fallback_tried = false;
        while let Some(delay) = policy.delay(attempt) {
            let scheduling = self.op_lock.lock().await;
            if stop.is_cancelled() {
                return;
            }
            if cancel.is_cancelled() {
                if self.teardown(id).await.is_ok() {
                    let _ = self.end_block().await;
                    self.live().session = None;
                }
                return;
            }
            self.set_state(ConnectionState::Reconnecting {
                profile: id,
                attempt,
                next_retry_ms: now_ms() + delay.as_millis() as u64,
            });
            self.log(
                Level::Info,
                format!("Reconnecting (attempt {attempt}) in {}s", delay.as_secs()),
            );
            drop(scheduling);
            let cancelled = tokio::select! {
                _ = cancel.cancelled() => true,
                _ = stop.cancelled() => return,
                _ = tokio::time::sleep(delay) => false,
            };
            let _g = self.op_lock.lock().await;
            if stop.is_cancelled() {
                return;
            }
            if cancelled || cancel.is_cancelled() {
                if self.teardown(id).await.is_ok() {
                    let _ = self.end_block().await;
                    self.live().session = None;
                }
                return;
            }
            self.record_server(id, |s| s.last_attempt_ms = Some(now_ms()));
            match self.activate_once(id, &cancel).await {
                Ok(()) => {
                    self.on_connected(id);
                    return;
                }
                Err(CoreError::Cancelled) => {
                    if self.teardown(id).await.is_ok() {
                        let _ = self.end_block().await;
                        self.live().session = None;
                    }
                    return;
                }
                Err(e) => {
                    self.record_server(id, |s| s.last_failure_ms = Some(now_ms()));
                    self.log(
                        Level::Warn,
                        format!("Reconnect attempt {attempt} failed: {e}"),
                    );
                    if attempt == FALLBACK_AFTER && !fallback_tried {
                        fallback_tried = true;
                        match self.try_fallback(id, &cancel).await {
                            Ok(Some(_)) => return,
                            Err(CoreError::Cancelled) => {
                                if self.teardown(id).await.is_ok() {
                                    let _ = self.end_block().await;
                                    self.live().session = None;
                                }
                                return;
                            }
                            Err(e) => {
                                self.log(Level::Warn, format!("Backup servers unavailable: {e}"))
                            }
                            Ok(None) => {}
                        }
                    }
                }
            }
            attempt += 1;
        }
        let _g = self.op_lock.lock().await;
        if cancel.is_cancelled() || stop.is_cancelled() {
            return;
        }
        if let Err(e) = self.teardown(id).await {
            self.log(Level::Error, format!("Reconnect cleanup failed: {e}"));
            return;
        }
        self.log(Level::Error, "Giving up after repeated reconnect failures");
        let reason = if fallback_tried && settings.auto_switch_server {
            "this server and its backups did not respond; check your internet connection or choose a server in another country in Locations"
        } else {
            "reconnect attempts exhausted"
        };
        self.set_state(ConnectionState::Failed {
            profile: Some(id),
            reason: reason.into(),
        });
    }

    /// Reconcile with the backend after start-up: adopt a tunnel that is
    /// still up (the UI may have restarted while the tunnel persisted).
    async fn recover(self: &Arc<Self>) {
        if !self.caps.available.is_supported() {
            return;
        }
        let known: Vec<ProfileId> = self.live().profiles.iter().map(|p| p.id).collect();
        let blocking = match self.backend.kill_switch_active().await {
            Ok(active) => Some(active),
            Err(e) => {
                self.log(Level::Warn, format!("Could not inspect the firewall: {e}"));
                None
            }
        };
        self.live().blocking = blocking;
        if blocking == Some(true) {
            self.log(
                Level::Warn,
                "The kill switch from the previous session is still blocking traffic outside the tunnel",
            );
        }
        match self.backend.installed().await {
            Ok(installed) => {
                if installed.iter().any(|id| !known.contains(id)) {
                    self.log(Level::Warn, "The backend has profiles missing from this registry. They were left intact; restore the registry or manage them in the system VPN settings.");
                }
                let missing = known
                    .iter()
                    .copied()
                    .filter(|id| !installed.contains(id))
                    .count();
                if missing > 0 {
                    self.log(
                        Level::Warn,
                        if self.caps.simulated {
                            format!(
                                "{missing} listed profile(s) have no simulated config. Re-import \
                                 each .conf, or relaunch without AEGIS_BACKEND=mock to use NetworkManager."
                            )
                        } else {
                            format!(
                                "{missing} listed profile(s) are missing from the VPN backend; \
                                 re-import their .conf files before connecting."
                            )
                        },
                    );
                }
            }
            Err(e) => self.log(Level::Warn, format!("Could not list backend profiles: {e}")),
        }
        match self.read_status().await {
            Ok(Some(s)) if known.contains(&s.id) && s.link == LinkState::Up => {
                self.log(
                    Level::Info,
                    "Found an active tunnel from a previous session",
                );
                self.on_connected(s.id);
            }
            Ok(None) if blocking == Some(true) => self.set_state(ConnectionState::Failed {
                profile: None,
                reason: "the kill switch from the previous session is still blocking traffic; connect again, \
                         or choose Stop blocking"
                    .into(),
            }),
            Ok(None) if blocking.is_none() => {
                self.set_state(ConnectionState::Uncertain {
                    profile: None,
                    reason: "cannot inspect the firewall; retry Disconnect to remove any block".into(),
                });
                self.start_monitor(None);
            }
            Ok(None) => {}
            result => {
                let (profile, reason) = match result {
                    Ok(Some(s)) => (
                        Some(s.id),
                        "existing tunnel is not yet reconciled".to_owned(),
                    ),
                    Err(e) => (None, format!("cannot read tunnel status at startup: {e}")),
                    Ok(None) => unreachable!(),
                };
                self.set_state(ConnectionState::Uncertain { profile, reason });
                self.start_monitor(None);
            }
        }
    }

    // ---------- diagnostics ----------

    pub async fn diagnostics(&self) -> Vec<crate::diagnostics::Check> {
        crate::diagnostics::run(self).await
    }

    /// The underlying backend (read access for diagnostics and integration tests).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub(crate) fn profiles_meta(&self) -> Vec<ProfileMeta> {
        self.live().profiles.clone()
    }
}

/// WireGuard endpoints the kill switch must allow. Only IP literals: a name
/// would need DNS outside the tunnel to resolve, which the block forbids.
fn kill_switch_endpoints(cfg: &WgConfig) -> Result<Vec<std::net::SocketAddr>> {
    let mut out = Vec::new();
    for e in cfg.peers.iter().filter_map(|p| p.endpoint.as_ref()) {
        match &e.host {
            aegis_config::Host::Ip(ip) => out.push(std::net::SocketAddr::new(*ip, e.port)),
            aegis_config::Host::Name(_) => {
                return Err(CoreError::invalid(
                    "the kill switch needs each server endpoint as an IP address, because names cannot be \
                     looked up while traffic outside the tunnel is blocked. Edit the profile's endpoint to the \
                     server's IP address, or turn off the kill switch.",
                ));
            }
        }
    }
    if out.is_empty() {
        return Err(CoreError::invalid(
            "the kill switch needs the server's endpoint, and this profile has none",
        ));
    }
    Ok(out)
}
