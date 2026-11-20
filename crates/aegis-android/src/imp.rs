use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;

use aegis_config::{RouteKind, WgConfig};
use aegis_core::{
    BackendKind, Capabilities, CoreError, Feature, LinkState, ProfileId, ProfileSummary,
    Protection, Result, Settings, Support, TunnelStatus, VpnBackend,
};
use serde::Deserialize;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Wry};

const PLUGIN_ID: &str = "org.vpnflow.vpn";
const PLUGIN_CLASS: &str = "VpnPlugin";

/// Handle to the Kotlin plugin, kept in Tauri's state.
struct Plugin(PluginHandle<Wry>);

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("vpnflow")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PLUGIN_ID, PLUGIN_CLASS)?;
            app.manage(Plugin(handle));
            Ok(())
        })
        .build()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StatusReply {
    up: bool,
    rx: u64,
    tx: u64,
    last_handshake_ms: u64,
}

#[derive(Deserialize)]
struct Granted {
    granted: bool,
}

pub struct AndroidBackend {
    plugin: PluginHandle<Wry>,
    dir: PathBuf,
    dns: Mutex<Vec<IpAddr>>,
    active: Mutex<Option<ProfileId>>,
}

impl AndroidBackend {
    pub fn new(app: &tauri::AppHandle) -> std::result::Result<Self, String> {
        let plugin = app
            .try_state::<Plugin>()
            .ok_or("the Android VPN plugin did not start")?
            .0
            .clone();
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("vpn-configs");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(Self {
            plugin,
            dir,
            dns: Mutex::new(Vec::new()),
            active: Mutex::new(None),
        })
    }

    fn path(&self, id: ProfileId) -> PathBuf {
        self.dir.join(format!("{id}.conf"))
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        cmd: &str,
        payload: serde_json::Value,
    ) -> Result<T> {
        self.plugin
            .run_mobile_plugin_async(cmd, payload)
            .await
            .map_err(CoreError::backend)
    }
}

impl VpnBackend for AndroidBackend {
    async fn capabilities(&self) -> Result<Capabilities> {
        Ok(Capabilities {
            backend: BackendKind::AndroidVpn,
            simulated: false,
            backend_version: None,
            available: Support::Supported,
            kill_switch: Support::Unsupported(
                "On Android the system does this. Open Android's VPN settings and turn on 'Block connections without VPN'.".into(),
            ),
            dns_protection: Support::Supported,
            ipv6_protection: Support::Unsupported("not checked on Android yet".into()),
            exact_handshake_time: Support::Supported,
            notes: vec![],
        })
    }

    async fn install(&self, id: ProfileId, _name: &str, cfg: &WgConfig) -> Result<()> {
        let text = cfg.to_wg_quick();
        tokio::fs::write(self.path(id), text.as_bytes())
            .await
            .map_err(CoreError::storage)
    }

    async fn uninstall(&self, id: ProfileId) -> Result<()> {
        match tokio::fs::remove_file(self.path(id)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::storage(e)),
        }
    }

    async fn load(&self, id: ProfileId) -> Result<WgConfig> {
        let text = tokio::fs::read_to_string(self.path(id))
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => CoreError::NotFound,
                _ => CoreError::storage(e),
            })?;
        Ok(aegis_config::parse(&text)?)
    }

    async fn installed(&self) -> Result<Vec<ProfileId>> {
        let mut ids = Vec::new();
        let mut rd = tokio::fs::read_dir(&self.dir)
            .await
            .map_err(CoreError::storage)?;
        while let Some(e) = rd.next_entry().await.map_err(CoreError::storage)? {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".conf")
                && let Ok(u) = uuid::Uuid::parse_str(stem)
            {
                ids.push(ProfileId(u));
            }
        }
        Ok(ids)
    }

    async fn activate(&self, id: ProfileId) -> Result<()> {
        let mut cfg = self.load(id).await?;
        let custom = self.dns.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !custom.is_empty() {
            cfg.interface.dns = custom;
        }
        let granted: Granted = self.call("prepare", serde_json::json!({})).await?;
        if !granted.granted {
            return Err(CoreError::PermissionDenied {
                message: "Android needs your OK to set up the VPN. Press Connect again and tap OK."
                    .into(),
            });
        }
        let text = cfg.to_wg_quick();
        let _: serde_json::Value = self
            .call("connect", serde_json::json!({ "config": text.as_str() }))
            .await?;
        *self.active.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
        Ok(())
    }

    async fn deactivate(&self, _id: ProfileId) -> Result<()> {
        let _: serde_json::Value = self.call("disconnect", serde_json::json!({})).await?;
        *self.active.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }

    async fn status(&self) -> Result<Option<TunnelStatus>> {
        let Some(id) = *self.active.lock().unwrap_or_else(|e| e.into_inner()) else {
            return Ok(None);
        };
        let s: StatusReply = self.call("status", serde_json::json!({})).await?;
        if !s.up {
            return Ok(None);
        }
        Ok(Some(TunnelStatus {
            id,
            link: LinkState::Up,
            interface: Some("tun0".into()),
            rx_bytes: s.rx,
            tx_bytes: s.tx,
            last_handshake_ms: (s.last_handshake_ms > 0).then_some(s.last_handshake_ms),
        }))
    }

    fn protection(&self, _: &ProfileSummary, _: &Settings) -> Protection {
        Protection::off()
    }

    async fn verify_protection(
        &self,
        profile: &ProfileSummary,
        _: &Settings,
        _: &TunnelStatus,
    ) -> Protection {
        let custom = !self
            .dns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty();
        let dns = if custom || !profile.dns.is_empty() {
            Feature::Configured("Android sends name lookups through the VPN's DNS.".into())
        } else {
            Feature::Off
        };
        let ipv6 = match profile.route_kind {
            RouteKind::Full | RouteKind::FullIpv6Only => {
                Feature::Configured("The profile routes IPv6 through the tunnel.".into())
            }
            _ => Feature::Unsupported("not checked on Android yet".into()),
        };
        Protection {
            kill_switch: Feature::Unsupported(
                "Turn on 'Block connections without VPN' in Android's VPN settings.".into(),
            ),
            dns,
            ipv6,
        }
    }

    async fn apply_kill_switch(
        &self,
        _: ProfileId,
        _: &[std::net::SocketAddr],
        _: bool,
    ) -> Result<()> {
        Err(CoreError::unsupported(
            "Android's kill switch is a system setting: 'Block connections without VPN'",
        ))
    }

    async fn apply_dns(&self, servers: &[IpAddr]) -> Result<()> {
        *self.dns.lock().unwrap_or_else(|e| e.into_inner()) = servers.to_vec();
        Ok(())
    }

    async fn remove_kill_switch(&self) -> Result<()> {
        Ok(())
    }

    async fn kill_switch_active(&self) -> Result<bool> {
        Ok(false)
    }
}
