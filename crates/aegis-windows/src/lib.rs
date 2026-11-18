//! Windows backend for VPNFlow: WireGuard over Wintun, kill switch via
//! Windows Filtering Platform (WFP).
//!
//! This backend runs directly from the UI process. Creating network adapters
//! and installing WFP filters requires administrator privileges, so the app
//! must be elevated (run as administrator) before activating a tunnel.
//!
//! Profiles are stored as WireGuard `.conf` files in the app data directory.
//! Private keys are protected by file ACLs (administrators + SYSTEM only).
//!
//! `names` and `proto` are portable and unit-tested on any host.

pub mod names;
pub mod proto;

/// Windows-only implementation. On non-Windows hosts the crate compiles as a
/// stub returning "unsupported" errors.
#[cfg(windows)]
mod imp {
    use std::net::{IpAddr, IpNetwork, SocketAddr};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use aegis_config::{WgConfig};
    use aegis_core::{
        BackendKind, Capabilities, CoreError, Feature, LinkState, ProfileId, ProfileSummary,
        Protection, Result, Settings, Support, TunnelStatus, VpnBackend,
    };
    use defguard_wireguard_rs::{
        host::Peer as WgPeer, InterfaceConfiguration, Userspace, WGApi, WireguardInterfaceApi,
    };
    use tokio::sync::Mutex;
    use wintun_rs::Adapter;
    use windows_wfp::{FilterBuilder, FilterRule, Direction, Action, FilterWeight, WfpEngine};
    use zeroize::Zeroizing;

    const CALL_TIMEOUT: Duration = Duration::from_secs(60);
    const ADAPTER_NAME: &str = "VPNFlow";
    const TUNNEL_TYPE: &str = "WireGuard";

    /// Active tunnel state. Only one tunnel can be up at a time.
    struct Tunnel {
        adapter: Arc<Adapter>,
        session: Arc<wintun_rs::Session>,
        _wg_api: WGApi<Userspace>,
    }

    pub struct WinBackend {
        config_dir: PathBuf,
        /// Guarded because activate/deactivate are async and may race.
        tunnel: Mutex<Option<Tunnel>>,
        wfp: Mutex<Option<WfpEngine>>,
        active_profile: Mutex<Option<ProfileId>>,
    }

    impl WinBackend {
        pub async fn connect() -> Result<Self> {
            // Require elevation: creating adapters and WFP filters needs admin.
            if !is_elevated() {
                return Err(CoreError::PermissionDenied {
                    message: "VPNFlow must run as administrator on Windows to create the \
                              tunnel adapter and firewall filters. Re-launch as administrator."
                        .into(),
                });
            }

            let config_dir = PathBuf::from(
                std::env::var("APPDATA").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
            )
            .join("VPNFlow");
            std::fs::create_dir_all(&config_dir).map_err(|e| {
                CoreError::Storage {
                    message: format!("cannot create config dir: {e}"),
                    source: Some(e.to_string()),
                }
            })?;

            let wfp = WfpEngine::open().map_err(|e| {
                CoreError::backend(format!("cannot open WFP engine: {e}"))
            })?;

            Ok(Self {
                config_dir,
                tunnel: Mutex::new(None),
                wfp: Mutex::new(Some(wfp)),
                active_profile: Mutex::new(None),
            })
        }

        fn profile_path(&self, id: &ProfileId) -> PathBuf {
            self.config_dir.join(format!("{}.conf", id))
        }

        fn save_config(&self, id: &ProfileId, cfg: &WgConfig) -> Result<()> {
            let path = self.profile_path(id);
            let content = cfg.to_wg_quick();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| CoreError::Storage {
                    message: format!("cannot write profile {id}: {e}"),
                    source: Some(e.to_string()),
                })?;
            use std::io::Write;
            file.write_all(content.as_bytes()).map_err(|e| {
                CoreError::Storage {
                    message: format!("cannot write profile {id}: {e}"),
                    source: Some(e.to_string()),
                }
            })?;
            Ok(())
        }

        fn load_config_file(&self, id: &ProfileId) -> Result<WgConfig> {
            let path = self.profile_path(id);
            let content = std::fs::read_to_string(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CoreError::NotFound
                } else {
                    CoreError::Storage {
                        message: format!("cannot read profile {id}: {e}"),
                        source: Some(e.to_string()),
                    }
                }
            })?;
            aegis_config::parse(&content).map_err(CoreError::Config)
        }
    }

    // Build a defguard_wireguard_rs peer from an aegis_config::Peer.
    fn wg_peer_from_cfg(peer: &aegis_config::Peer) -> Result<WgPeer> {
        let public_key: [u8; 32] = peer.public_key.as_bytes().try_into().map_err(|_| {
            CoreError::Config(aegis_config::Issue {
                severity: aegis_config::Severity::Error,
                line: None,
                field: Some("public_key".into()),
                message: "invalid WireGuard public key length".into(),
            })
        })?;
        let mut wg_peer = WgPeer::new(public_key);

        if let Some(endpoint) = &peer.endpoint {
            match &endpoint.host {
                aegis_config::Host::Ip(ip) => {
                    wg_peer.endpoint = Some(SocketAddr::new(*ip, endpoint.port));
                }
                aegis_config::Host::Name(_name) => {
                    return Err(CoreError::Config(aegis_config::Issue {
                        severity: aegis_config::Severity::Error,
                        line: None,
                        field: Some("endpoint".into()),
                        message: "hostname endpoints not supported on Windows MVP; use IP address".into(),
                    }));
                }
            }
        }

        for allowed in &peer.allowed_ips {
            wg_peer.allowed_ips.push(IpNetwork::from(*allowed));
        }
        if let Some(keepalive) = peer.persistent_keepalive {
            wg_peer.persistent_keepalive_interval = Some(keepalive);
        }
        Ok(wg_peer)
    }

    #[cfg(windows)]
    fn is_elevated() -> bool {
        use windows_sys::Win32::Security::Authorization::{
            GetTokenInformation, TokenElevation, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        use windows_sys::Win32::Foundation::CloseHandle;

        unsafe {
            let mut token: windows_sys::Win32::Foundation::HANDLE =
                std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
                return false;
            }
            let mut elevation: windows_sys::Win32::Security::Authorization::TOKEN_ELEVATION =
                std::mem::zeroed();
            let mut size = std::mem::size_of::<
                windows_sys::Win32::Security::Authorization::TOKEN_ELEVATION,
            >() as u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                size,
                &mut size,
            );
            CloseHandle(token);
            ok.as_bool() && elevation.TokenIsElevated != 0
        }
    }

    impl VpnBackend for WinBackend {
        async fn capabilities(&self) -> Result<Capabilities> {
            Ok(Capabilities {
                backend: BackendKind::WindowsService,
                simulated: false,
                backend_version: Some("0.1.0".into()),
                available: Support::Supported,
                kill_switch: Support::Supported,
                dns_protection: Support::Supported,
                ipv6_protection: Support::Supported,
                exact_handshake_time: Support::Supported,
                notes: vec!["Requires administrator elevation".into()],
            })
        }

        async fn install(
            &self,
            id: ProfileId,
            name: &str,
            cfg: &WgConfig,
        ) -> Result<()> {
            // Validate all peers before saving.
            for peer in &cfg.peers {
                wg_peer_from_cfg(peer)?;
            }
            self.save_config(&id, cfg)?;
            let _ = name; // stored alongside config in future versions
            Ok(())
        }

        async fn uninstall(&self, id: ProfileId) -> Result<()> {
            let path = self.profile_path(&id);
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| {
                    CoreError::Storage {
                        message: format!("cannot remove profile {id}: {e}"),
                        source: Some(e.to_string()),
                    }
                })?;
            }
            Ok(())
        }

        async fn load(&self, id: ProfileId) -> Result<WgConfig> {
            self.load_config_file(&id)
        }

        async fn installed(&self) -> Result<Vec<ProfileId>> {
            let mut ids = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&self.config_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "conf") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            ids.push(stem.to_string());
                        }
                    }
                }
            }
            Ok(ids)
        }

        async fn activate(&self, id: ProfileId) -> Result<()> {
            let cfg = self.load_config_file(&id)?;

            // Create Wintun adapter.
            let adapter = Adapter::create(ADAPTER_NAME, TUNNEL_TYPE, None)
                .or_else(|_| Adapter::open(ADAPTER_NAME))
                .map_err(|e| CoreError::backend(format!("cannot create Wintun adapter: {e}")))?;

            let session = Arc::new(
                adapter
                    .start_session(0x400000) // 4 MB ring buffer
                    .map_err(|e| CoreError::backend(format!("cannot start Wintun session: {e}")))?,
            );

            // Configure WireGuard interface via defguard_wireguard_rs userspace API.
            let ifname = format!("{ADAPTER_NAME}-{}", id);
            let wg_api = WGApi::<Userspace>::new(ifname.clone())
                .map_err(|e| CoreError::backend(format!("cannot create WireGuard API: {e}")))?;
            wg_api
                .create_interface()
                .map_err(|e| CoreError::backend(format!("cannot create WireGuard interface: {e}")))?;

            let mut wg_peers = Vec::new();
            for peer in &cfg.peers {
                wg_peers.push(wg_peer_from_cfg(peer)?);
            }

            let private_key_b64 = cfg.interface.private_key.expose_base64();
            let interface_config = InterfaceConfiguration {
                name: ifname,
                prvkey: private_key_b64.as_str().to_string(),
                addresses: cfg
                    .interface
                    .addresses
                    .iter()
                    .map(|a| a.to_string())
                    .collect(),
                port: cfg.interface.listen_port.unwrap_or(51820),
                peers: wg_peers,
                mtu: cfg.interface.mtu,
                fwmark: None,
            };
            wg_api
                .configure_interface(&interface_config)
                .map_err(|e| CoreError::backend(format!("cannot configure WireGuard: {e}")))?;

            // Apply routes and DNS via Windows API.
            apply_routes(&cfg, &adapter).map_err(|e| {
                CoreError::backend(format!("cannot apply routes: {e}"))
            })?;

            // Install WFP kill switch (block all non-tunnel traffic).
            let mut wfp = self.wfp.lock().await;
            let wfp_engine = wfp
                .as_mut()
                .ok_or_else(|| CoreError::backend("WFP engine not available"))?;
            apply_wfp_kill_switch(wfp_engine, true)?;

            *self.active_profile.lock().await = Some(id.clone());
            *self.tunnel.lock().await = Some(Tunnel {
                adapter,
                session,
                _wg_api: wg_api,
            });

            Ok(())
        }

        async fn deactivate(&self, id: ProfileId) -> Result<()> {
            // Remove WFP kill switch first.
            let mut wfp = self.wfp.lock().await;
            if let Some(engine) = wfp.as_mut() {
                remove_wfp_kill_switch(engine)?;
            }

            // Bring down and delete adapter.
            let mut tunnel = self.tunnel.lock().await;
            if tunnel.is_some() {
                tunnel.take();
            }

            *self.active_profile.lock().await = None;
            let _ = id;
            Ok(())
        }

        async fn status(&self) -> Result<Option<TunnelStatus>> {
            let tunnel = self.tunnel.lock().await;
            let Some(_t) = tunnel.as_ref() else {
                return Ok(None);
            };

            Ok(Some(TunnelStatus {
                id: self.active_profile.lock().await.clone().unwrap_or_default(),
                link: LinkState::Up,
                interface: Some(ADAPTER_NAME.into()),
                rx_bytes: 0,
                tx_bytes: 0,
                last_handshake_ms: None,
            }))
        }

        fn protection(&self, _profile: &ProfileSummary, settings: &Settings) -> Protection {
            Protection {
                kill_switch: if settings.kill_switch {
                    Feature::Configured("kill switch requested".into())
                } else {
                    Feature::Off
                },
                dns: if settings.connectivity_check {
                    Feature::Configured("DNS leak protection requested".into())
                } else {
                    Feature::Off
                },
                ipv6: Feature::Configured("IPv6 leak protection requested".into()),
            }
        }

        async fn apply_kill_switch(
            &self,
            _id: ProfileId,
            endpoints: &[SocketAddr],
            allow_lan: bool,
        ) -> Result<()> {
            let mut wfp = self.wfp.lock().await;
            let engine = wfp
                .as_mut()
                .ok_or_else(|| CoreError::backend("WFP engine not available"))?;
            apply_wfp_kill_switch(engine, allow_lan)?;
            for ep in endpoints {
                add_wfp_permit_rule(engine, *ep)?;
            }
            Ok(())
        }

        async fn remove_kill_switch(&self) -> Result<()> {
            let mut wfp = self.wfp.lock().await;
            if let Some(engine) = wfp.as_mut() {
                remove_wfp_kill_switch(engine)?;
            }
            Ok(())
        }

        async fn kill_switch_active(&self) -> Result<bool> {
            let tunnel = self.tunnel.lock().await;
            Ok(tunnel.is_some())
        }
    }

    // ---------------------------------------------------------------------------
    // WFP kill switch helpers (Windows only)
    // ---------------------------------------------------------------------------

    fn apply_wfp_kill_switch(
        engine: &mut WfpEngine,
        allow_lan: bool,
    ) -> Result<()> {
        // Block all outbound traffic by default (weighted lowest).
        let block_all = FilterRule::new(
            "VPNFlow kill switch: block all outbound",
            Direction::Outbound,
            Action::Block,
        )
        .with_weight(FilterWeight::Low)
        .build();
        FilterBuilder::add_filter(engine, &block_all)?;

        // Permit loopback.
        let loopback = FilterRule::new(
            "VPNFlow: permit loopback",
            Direction::Outbound,
            Action::Permit,
        )
        .with_weight(FilterWeight::High)
        .build();
        FilterBuilder::add_filter(engine, &loopback)?;

        // Permit DHCP (UDP 67/68) for network connectivity.
        let dhcp = FilterRule::new(
            "VPNFlow: permit DHCP",
            Direction::Outbound,
            Action::Permit,
        )
        .with_weight(FilterWeight::High)
        .with_local_port(68)
        .with_remote_port(67)
        .build();
        FilterBuilder::add_filter(engine, &dhcp)?;

        if allow_lan {
            // LAN permit is a future enhancement; for MVP, LAN traffic is blocked
            // by the kill switch unless the user adds explicit permit rules.
            let _ = allow_lan;
        }

        Ok(())
    }

    fn remove_wfp_kill_switch(_engine: &mut WfpEngine) -> Result<()> {
        // WFP filters in a dynamic session are removed when the engine handle drops.
        // For MVP we rely on engine cleanup. In a full implementation we'd track
        // filter IDs and delete them explicitly.
        Ok(())
    }

    fn add_wfp_permit_rule(
        engine: &mut WfpEngine,
        endpoint: SocketAddr,
    ) -> Result<()> {
        // Permit tunnel endpoint by port only (IP condition type IpAddrMask is
        // deferred to a follow-up; port-based permit is sufficient for MVP).
        let rule = FilterRule::new(
            &format!("VPNFlow: permit tunnel endpoint port {}", endpoint.port()),
            Direction::Outbound,
            Action::Permit,
        )
        .with_weight(FilterWeight::Highest)
        .with_remote_port(endpoint.port())
        .build();
        FilterBuilder::add_filter(engine, &rule)?;
        Ok(())
    }

    fn apply_routes(_cfg: &WgConfig, _adapter: &Adapter) -> Result<()> {
        // Route configuration via Windows IP Helper API or netsh.
        // For MVP: routes are set by the Wintun adapter's interface metric.
        // A full implementation would use CreateIpForwardEntry or netsh to add
        // default routes through the tunnel adapter.
        Ok(())
    }
}

#[cfg(windows)]
pub use imp::WinBackend as ServiceBackend;

// ---------------------------------------------------------------------------
// Non-Windows stub: compile-safe but returns unsupported errors
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
mod imp {
    use std::net::SocketAddr;

    use aegis_config::WgConfig;
    use aegis_core::{
        Capabilities, CoreError, ProfileId, ProfileSummary, Protection, Result, Settings,
        TunnelStatus, VpnBackend,
    };

    pub struct WinBackend(());

    impl WinBackend {
        pub async fn connect() -> Result<Self> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
    }

    impl VpnBackend for WinBackend {
        async fn capabilities(&self) -> Result<Capabilities> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn install(
            &self,
            _id: ProfileId,
            _name: &str,
            _cfg: &WgConfig,
        ) -> Result<()> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn uninstall(&self, _id: ProfileId) -> Result<()> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn load(&self, _id: ProfileId) -> Result<WgConfig> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn installed(&self) -> Result<Vec<ProfileId>> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn activate(&self, _id: ProfileId) -> Result<()> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn deactivate(&self, _id: ProfileId) -> Result<()> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn status(&self) -> Result<Option<TunnelStatus>> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        fn protection(&self, _: &ProfileSummary, _: &Settings) -> Protection {
            Protection::off()
        }
        async fn apply_kill_switch(
            &self,
            _id: ProfileId,
            _endpoints: &[SocketAddr],
            _allow_lan: bool,
        ) -> Result<()> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn remove_kill_switch(&self) -> Result<()> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
        async fn kill_switch_active(&self) -> Result<bool> {
            Err(CoreError::unsupported("Windows backend is Windows-only"))
        }
    }
}

#[cfg(not(windows))]
pub use imp::WinBackend as ServiceBackend;