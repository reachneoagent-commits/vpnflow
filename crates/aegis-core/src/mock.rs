//! Simulated backend for UI development and deterministic tests.
//!
//! It never touches the network. Every capability it reports says
//! `simulated: true`, and protection is always `Unsupported`, so the UI can
//! never present a mock connection as real protection.
//!
//! Desktop mock mode can persist configs under a private directory so restarts
//! keep imported profiles. Tests keep using the in-memory constructors.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use aegis_config::WgConfig;

use crate::backend::VpnBackend;
use crate::error::{CoreError, Result};
use crate::store::write_private_atomic;
use crate::types::*;

#[derive(Default)]
struct Inner {
    profiles: HashMap<ProfileId, WgConfig>,
    active: Option<(ProfileId, u64)>,
    fail_next_activate: Option<String>,
    drop_active: bool,
    fail_deactivate: bool,
    fail_status: bool,
    silent_peer: bool,
    /// Extra received bytes, to model the server answering again.
    extra_rx: u64,
    /// Extra sent bytes, to model the client sending while the server is silent.
    extra_tx: u64,
    reachability_checks: usize,
    /// Simulated kill switch: (iface owner, endpoints, allow_lan) while blocking.
    blocking: Option<(ProfileId, Vec<std::net::SocketAddr>, bool)>,
    fail_next_kill_switch: bool,
    fail_kill_switch_status: bool,
    fail_remove_kill_switch: bool,
    fail_after_remove_kill_switch: bool,
    /// Order of kill-switch and tunnel operations, for tests.
    ops: Vec<&'static str>,
}

pub struct MockBackend {
    inner: Mutex<Inner>,
    /// When set, install/uninstall also write/delete `{id}.conf` here.
    store_dir: Option<PathBuf>,
    pub activate_delay: Duration,
    pub load_delay: Duration,
    pub(crate) reachability_delay: Duration,
    /// Test-only: report the kill switch as supported. Never set by the app,
    /// so `AEGIS_BACKEND=mock` keeps reporting it unsupported.
    test_kill_switch: bool,
    /// Test-driven OS events (resume, network switch), handed to the engine once.
    events_tx: tokio::sync::mpsc::Sender<SystemEvent>,
    events_rx: Mutex<Option<tokio::sync::mpsc::Receiver<SystemEvent>>>,
}

impl Default for MockBackend {
    fn default() -> Self {
        let (events_tx, events_rx) = tokio::sync::mpsc::channel(16);
        Self {
            events_tx,
            events_rx: Mutex::new(Some(events_rx)),
            inner: Mutex::default(),
            store_dir: None,
            activate_delay: Duration::from_millis(700),
            load_delay: Duration::ZERO,
            reachability_delay: Duration::ZERO,
            test_kill_switch: false,
        }
    }
}

impl MockBackend {
    pub fn instant() -> Self {
        Self::with_delay(Duration::ZERO)
    }
    pub fn with_delay(activate_delay: Duration) -> Self {
        Self {
            activate_delay,
            ..Default::default()
        }
    }
    /// Persist simulated configs under `dir` (mode 0700) so mock sessions survive restarts.
    pub fn with_store(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(CoreError::storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
        let mut profiles = HashMap::new();
        for entry in fs::read_dir(&dir).map_err(CoreError::storage)? {
            let entry = entry.map_err(CoreError::storage)?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("conf") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(id) = uuid::Uuid::parse_str(stem).map(ProfileId) else {
                continue;
            };
            let text = fs::read_to_string(&path).map_err(CoreError::storage)?;
            match aegis_config::parse(&text) {
                Ok(cfg) => {
                    profiles.insert(id, cfg);
                }
                Err(e) => tracing::warn!(
                    target: "aegis",
                    "ignoring unreadable mock profile {}: {e}",
                    path.display()
                ),
            }
        }
        Ok(Self {
            inner: Mutex::new(Inner {
                profiles,
                ..Inner::default()
            }),
            store_dir: Some(dir),
            ..Default::default()
        })
    }
    fn conf_path(&self, id: ProfileId) -> Option<PathBuf> {
        self.store_dir
            .as_ref()
            .map(|d| d.join(format!("{id}.conf")))
    }
    fn persist(&self, id: ProfileId, cfg: &WgConfig) -> Result<()> {
        let Some(path) = self.conf_path(id) else {
            return Ok(());
        };
        write_private_atomic(&path, cfg.to_wg_quick().as_bytes())
    }
    fn forget_file(&self, id: ProfileId) {
        if let Some(path) = self.conf_path(id) {
            let _ = fs::remove_file(path);
        }
    }
    /// Test hook: make the next `activate` fail with this message.
    pub fn fail_next_activate(&self, msg: &str) {
        self.lock().fail_next_activate = Some(msg.into());
    }
    /// Test hook: simulate the tunnel disappearing underneath us.
    pub fn drop_tunnel(&self) {
        let mut g = self.lock();
        g.active = None;
        g.drop_active = true;
    }
    pub fn fail_deactivate(&self, fail: bool) {
        self.lock().fail_deactivate = fail;
    }
    pub fn fail_status(&self, fail: bool) {
        self.lock().fail_status = fail;
    }
    /// Test hook: the link is up but the server has not answered (no rx, no handshake).
    pub fn silent_peer(&self, silent: bool) {
        self.lock().silent_peer = silent;
    }
    /// Test-only: an instant mock whose kill switch is "supported" and recorded.
    pub fn with_test_kill_switch() -> Self {
        Self {
            test_kill_switch: true,
            ..Self::instant()
        }
    }
    /// Test hook: deliver an OS event (resume, network switch) to the engine.
    pub fn send_event(&self, ev: SystemEvent) {
        let _ = self.events_tx.try_send(ev);
    }
    /// Test hook: make the next kill-switch apply fail.
    pub fn fail_next_kill_switch(&self) {
        self.lock().fail_next_kill_switch = true;
    }
    pub fn fail_kill_switch_status(&self, fail: bool) {
        self.lock().fail_kill_switch_status = fail;
    }
    pub fn fail_remove_kill_switch(&self, fail: bool) {
        self.lock().fail_remove_kill_switch = fail;
    }
    pub fn fail_after_remove_kill_switch(&self) {
        self.lock().fail_after_remove_kill_switch = true;
    }
    /// Simulated block, if any: (profile, endpoints, allow_lan).
    pub fn blocking(&self) -> Option<(ProfileId, Vec<std::net::SocketAddr>, bool)> {
        self.lock().blocking.clone()
    }
    /// Test hook: a block left over from a previous run.
    pub fn leave_block(&self, id: ProfileId) {
        self.lock().blocking = Some((id, vec![], false));
    }
    pub fn ops(&self) -> Vec<&'static str> {
        self.lock().ops.clone()
    }
    /// Test hook: the client sends `bytes` more to the server.
    pub fn client_sends(&self, bytes: u64) {
        self.lock().extra_tx += bytes;
    }
    /// Test hook: the server sends `bytes` more to the client.
    pub fn peer_sends(&self, bytes: u64) {
        self.lock().extra_rx += bytes;
    }
    /// How many times the engine ran the reachability check.
    pub fn reachability_checks(&self) -> usize {
        self.lock().reachability_checks
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

const SIM: &str = "simulated backend: no real tunnel exists";

impl VpnBackend for MockBackend {
    async fn capabilities(&self) -> Result<Capabilities> {
        let u = || Support::Unsupported(SIM.into());
        Ok(Capabilities {
            backend: BackendKind::Mock,
            simulated: true,
            backend_version: None,
            available: Support::Supported,
            kill_switch: if self.test_kill_switch { Support::Supported } else { u() },
            dns_protection: u(),
            ipv6_protection: u(),
            exact_handshake_time: u(),
            notes: vec!["Running with the simulated backend (AEGIS_BACKEND=mock). Traffic is NOT protected.".into()],
        })
    }

    async fn install(&self, id: ProfileId, _name: &str, cfg: &WgConfig) -> Result<()> {
        self.persist(id, cfg)?;
        self.lock().profiles.insert(id, cfg.clone());
        Ok(())
    }

    async fn uninstall(&self, id: ProfileId) -> Result<()> {
        self.forget_file(id);
        let mut g = self.lock();
        g.profiles.remove(&id);
        if g.active.map(|a| a.0) == Some(id) {
            g.active = None;
        }
        Ok(())
    }

    async fn load(&self, id: ProfileId) -> Result<WgConfig> {
        tokio::time::sleep(self.load_delay).await;
        self.lock()
            .profiles
            .get(&id)
            .cloned()
            .ok_or(CoreError::NotFound)
    }

    async fn installed(&self) -> Result<Vec<ProfileId>> {
        Ok(self.lock().profiles.keys().copied().collect())
    }

    async fn activate(&self, id: ProfileId) -> Result<()> {
        if !self.lock().profiles.contains_key(&id) {
            return Err(CoreError::NotFound);
        }
        tokio::time::sleep(self.activate_delay).await;
        let mut g = self.lock();
        g.ops.push("activate");
        if let Some(msg) = g.fail_next_activate.take() {
            return Err(CoreError::backend(msg));
        }
        g.active = Some((id, now_ms()));
        g.drop_active = false;
        Ok(())
    }

    async fn deactivate(&self, id: ProfileId) -> Result<()> {
        let mut g = self.lock();
        g.ops.push("deactivate");
        if g.fail_deactivate {
            return Err(CoreError::backend("simulated deactivation failure"));
        }
        if g.active.map(|a| a.0) == Some(id) {
            g.active = None;
        }
        Ok(())
    }

    async fn status(&self) -> Result<Option<TunnelStatus>> {
        let g = self.lock();
        if g.fail_status {
            return Err(CoreError::backend("simulated status failure"));
        }
        Ok(g.active.map(|(id, since)| {
            // Deterministic synthetic counters so the UI has something to render.
            let secs = now_ms().saturating_sub(since) / 1000;
            TunnelStatus {
                id,
                link: LinkState::Up,
                interface: Some("sim0".into()),
                rx_bytes: if g.silent_peer {
                    0
                } else {
                    secs * 48_000 + g.extra_rx
                },
                tx_bytes: secs * 9_000 + g.extra_tx,
                last_handshake_ms: (!g.silent_peer).then_some(since),
            }
        }))
    }

    fn protection(&self, _profile: &ProfileSummary, _settings: &Settings) -> Protection {
        let u = || Feature::Unsupported(SIM.into());
        Protection {
            kill_switch: u(),
            dns: u(),
            ipv6: u(),
        }
    }

    async fn check_reachability(
        &self,
        _profile: &ProfileSummary,
        _status: &TunnelStatus,
    ) -> Feature {
        self.lock().reachability_checks += 1;
        tokio::time::sleep(self.reachability_delay).await;
        Feature::Unsupported(SIM.into())
    }

    fn system_events(&self) -> Option<tokio::sync::mpsc::Receiver<SystemEvent>> {
        self.events_rx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    async fn apply_kill_switch(
        &self,
        id: ProfileId,
        endpoints: &[std::net::SocketAddr],
        allow_lan: bool,
    ) -> Result<()> {
        let mut g = self.lock();
        if !self.test_kill_switch {
            return Err(CoreError::unsupported(SIM));
        }
        g.ops.push("apply");
        if std::mem::take(&mut g.fail_next_kill_switch) {
            return Err(CoreError::backend("simulated kill switch failure"));
        }
        g.blocking = Some((id, endpoints.to_vec(), allow_lan));
        Ok(())
    }

    async fn remove_kill_switch(&self) -> Result<()> {
        let mut g = self.lock();
        g.ops.push("remove");
        if g.fail_remove_kill_switch {
            return Err(CoreError::backend("simulated firewall removal failure"));
        }
        g.blocking = None;
        if std::mem::take(&mut g.fail_after_remove_kill_switch) {
            return Err(CoreError::backend("simulated marker cleanup failure"));
        }
        Ok(())
    }

    async fn kill_switch_active(&self) -> Result<bool> {
        let g = self.lock();
        if g.fail_kill_switch_status {
            return Err(CoreError::backend("simulated firewall inspection failure"));
        }
        Ok(g.blocking.is_some())
    }

    async fn check_public_ip(&self, _profile: &ProfileSummary, _status: &TunnelStatus) -> Feature {
        Feature::Unsupported(SIM.into())
    }
}
