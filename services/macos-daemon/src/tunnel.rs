//! Tunnel lifecycle: `wireguard-go` on a `utun`, addresses, routes, DNS and
//! the pf kill switch. Everything that touches the system lives here; the
//! argument builders it uses are pure and tested in `netcfg`, `uapi` and
//! `aegis_macos::pf`. This file has NOT been run against a real tunnel yet
//! (see docs/development/MACOS.md for the manual test plan).
//!
//! Failure policy: a failed activation tears down everything it did except the
//! kill switch, which stays in place (fail closed).

use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use aegis_config::{Host, WgConfig};
use aegis_core::{CoreError, LinkState, ProfileId, Result, TunnelStatus, now_ms};
use aegis_macos::pf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::netcfg::{self, Gateway};
use crate::store::{ConfigStore, STATE_DIR};
use crate::uapi;

pub const WIREGUARD_GO: &str = "/usr/local/libexec/vpnflow/wireguard-go";
const IFCONFIG: &str = "/sbin/ifconfig";
const ROUTE: &str = "/sbin/route";
const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const PFCTL: &str = "/sbin/pfctl";
const NAME_FILE: &str = "/var/run/vpnflow-tun.name";
const DNS_BACKUP: &str = "dns-backup.json";
/// Safe on IPv4 and IPv6 servers and on home connections that add their own
/// headers (PPPoE, some routers). 1420 can make some sites hang on those.
const DEFAULT_MTU: u16 = 1380;

struct Active {
    id: ProfileId,
    iface: String,
    child: Child,
    host_routes: Vec<IpAddr>,
    bypass_routes: Vec<ipnet::IpNet>,
}

struct KillSpec {
    id: ProfileId,
    endpoints: Vec<SocketAddr>,
    allow_lan: bool,
}

#[derive(Default)]
struct State {
    active: Option<Active>,
    kill: Option<KillSpec>,
    bypass: Vec<ipnet::IpNet>,
    dns_override: Vec<IpAddr>,
    pf_token: Option<String>,
}

pub struct TunnelManager {
    store: Arc<ConfigStore>,
    state: Mutex<State>,
}

async fn run(bin: &str, args: &[String]) -> Result<String> {
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| CoreError::backend(format!("cannot run {bin}: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(CoreError::backend(format!(
            "{} failed: {}",
            Path::new(bin)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(bin),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Root-owned and not writable by group or others, or we will not execute it.
fn trusted_binary(path: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|m| {
        m.is_file() && m.uid() == 0 && m.mode() & 0o022 == 0 && m.mode() & 0o100 != 0
    })
}

pub fn available() -> std::result::Result<(), String> {
    if !trusted_binary(WIREGUARD_GO) {
        return Err(format!(
            "wireguard-go is missing or not root-owned at {WIREGUARD_GO}; reinstall VPNFlow"
        ));
    }
    if !Path::new(PFCTL).exists() {
        return Err("pfctl is missing".into());
    }
    Ok(())
}

async fn uapi_call(iface: &str, request: &str) -> Result<String> {
    let path = format!("/var/run/wireguard/{iface}.sock");
    let mut s = None;
    for _ in 0..30 {
        if let Ok(c) = tokio::net::UnixStream::connect(&path).await {
            s = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut s =
        s.ok_or_else(|| CoreError::backend("wireguard-go did not open its control socket"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        s.write_all(request.as_bytes()).await?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        while !buf.ends_with(b"\n\n") {
            let n = s.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
    })
    .await
    .map_err(|_| CoreError::backend("wireguard-go did not answer"))?
    .map_err(CoreError::backend)
}

async fn resolve_endpoints(cfg: &WgConfig) -> Result<Vec<Option<SocketAddr>>> {
    let mut out = Vec::new();
    for peer in &cfg.peers {
        out.push(match &peer.endpoint {
            None => None,
            Some(ep) => match &ep.host {
                Host::Ip(ip) => Some(SocketAddr::new(*ip, ep.port)),
                Host::Name(n) => Some(
                    tokio::net::lookup_host((n.as_str(), ep.port))
                        .await
                        .map_err(|e| {
                            CoreError::backend(format!("cannot resolve the endpoint host: {e}"))
                        })?
                        .next()
                        .ok_or_else(|| CoreError::backend("the endpoint host has no address"))?,
                ),
            },
        });
    }
    Ok(out)
}

async fn default_gateway(v6: bool) -> Option<Gateway> {
    let mut args = vec!["-n".to_owned(), "get".into()];
    if v6 {
        args.push("-inet6".into());
    }
    args.push("default".into());
    netcfg::parse_default_gateway(&run(ROUTE, &args).await.ok()?)
}

impl TunnelManager {
    pub fn new(store: Arc<ConfigStore>) -> Self {
        Self {
            store,
            state: Mutex::new(State::default()),
        }
    }

    // ----- DNS -----------------------------------------------------------

    fn dns_backup_path() -> std::path::PathBuf {
        Path::new(STATE_DIR).join(DNS_BACKUP)
    }

    async fn set_dns(servers: &[IpAddr]) -> Result<()> {
        if servers.is_empty() {
            return Ok(());
        }
        let services =
            netcfg::parse_services(&run(NETWORKSETUP, &["-listallnetworkservices".into()]).await?);
        let mut backup: Vec<(String, Vec<IpAddr>)> = Vec::new();
        for svc in &services {
            let cur = run(NETWORKSETUP, &["-getdnsservers".into(), svc.clone()])
                .await
                .map(|o| netcfg::parse_dns_servers(&o))
                .unwrap_or_default();
            backup.push((svc.clone(), cur));
        }
        // Persist before changing anything so a crash can be undone.
        let json = serde_json::to_vec(&backup).map_err(CoreError::storage)?;
        std::fs::write(Self::dns_backup_path(), json).map_err(CoreError::storage)?;
        for svc in &services {
            run(NETWORKSETUP, &netcfg::set_dns_args(svc, servers)).await?;
        }
        Ok(())
    }

    /// Restore the DNS settings saved by `set_dns`. Also run at daemon start.
    pub async fn restore_dns() {
        let path = Self::dns_backup_path();
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        if let Ok(backup) = serde_json::from_slice::<Vec<(String, Vec<IpAddr>)>>(&bytes) {
            for (svc, servers) in backup {
                if let Err(e) = run(NETWORKSETUP, &netcfg::set_dns_args(&svc, &servers)).await {
                    tracing::warn!("restoring DNS for one network service failed: {e}");
                }
            }
        }
        let _ = std::fs::remove_file(path);
    }

    // ----- pf ------------------------------------------------------------

    async fn pf_load(&self, st: &mut State, rules: &str) -> Result<()> {
        if st.pf_token.is_none() {
            // `pfctl -E` prints "Token : <n>" on stderr; hold it so we only
            // release our own reference to pf.
            let out = Command::new(PFCTL)
                .arg("-E")
                .stdin(Stdio::null())
                .output()
                .await
                .map_err(|e| CoreError::backend(format!("cannot run pfctl: {e}")))?;
            let text = String::from_utf8_lossy(&out.stderr);
            st.pf_token = text
                .lines()
                .find_map(|l| l.split_once("Token :").map(|(_, t)| t.trim().to_owned()));
        }
        let mut child = Command::new(PFCTL)
            .args(["-a", pf::ANCHOR, "-f", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CoreError::backend(format!("cannot run pfctl: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(rules.as_bytes())
                .await
                .map_err(CoreError::backend)?;
        }
        let out = child.wait_with_output().await.map_err(CoreError::backend)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(CoreError::backend(format!(
                "pfctl rejected the kill-switch rules: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }

    fn render(spec: &KillSpec, iface: Option<&str>, bypass: &[ipnet::IpNet]) -> Result<String> {
        pf::ruleset_with_bypass(iface, &spec.endpoints, spec.allow_lan, bypass).map_err(|e| match e
        {
            pf::PfError::NoEndpoints => {
                CoreError::invalid("the kill switch needs at least one server endpoint")
            }
            pf::PfError::BadInterface => CoreError::backend("unexpected tunnel interface name"),
        })
    }

    pub async fn apply_kill_switch(
        &self,
        id: ProfileId,
        endpoints: &[SocketAddr],
        allow_lan: bool,
    ) -> Result<()> {
        let mut st = self.state.lock().await;
        let spec = KillSpec {
            id,
            endpoints: endpoints.to_vec(),
            allow_lan,
        };
        let iface = st
            .active
            .as_ref()
            .filter(|a| a.id == id)
            .map(|a| a.iface.clone());
        let rules = Self::render(&spec, iface.as_deref(), &st.bypass)?;
        self.pf_load(&mut st, &rules).await?;
        st.kill = Some(spec);
        Ok(())
    }

    /// Replace the bypass list. Takes effect from the next `activate`, so the
    /// list never changes under a running tunnel.
    pub async fn set_bypass(&self, nets: &[String]) -> Result<()> {
        let parsed = nets
            .iter()
            .map(|n| aegis_core::parse_bypass(n))
            .collect::<Result<Vec<_>>>()?;
        if parsed.len() > aegis_core::MAX_BYPASS {
            return Err(CoreError::invalid("too many bypass entries"));
        }
        self.state.lock().await.bypass = parsed;
        Ok(())
    }

    /// Replace the custom DNS servers. Like the bypass list it takes effect
    /// from the next `activate`.
    pub async fn set_dns_override(&self, servers: &[IpAddr]) -> Result<()> {
        if servers.len() > aegis_core::MAX_DNS_SERVERS {
            return Err(CoreError::invalid("too many DNS servers"));
        }
        self.state.lock().await.dns_override = servers.to_vec();
        Ok(())
    }

    pub async fn remove_kill_switch(&self) -> Result<()> {
        let mut st = self.state.lock().await;
        run(
            PFCTL,
            &["-a".into(), pf::ANCHOR.into(), "-F".into(), "all".into()],
        )
        .await?;
        if let Some(t) = st.pf_token.take() {
            let _ = run(PFCTL, &["-X".into(), t]).await;
        }
        st.kill = None;
        Ok(())
    }

    pub async fn kill_switch_active(&self) -> Result<bool> {
        let out = run(PFCTL, &["-a".into(), pf::ANCHOR.into(), "-sr".into()]).await?;
        Ok(out.lines().any(|l| l.trim_start().starts_with("block")))
    }

    // ----- tunnel --------------------------------------------------------

    pub async fn activate(&self, id: ProfileId) -> Result<()> {
        let mut st = self.state.lock().await;
        if st.active.as_ref().is_some_and(|a| a.id == id) {
            return Ok(());
        }
        self.teardown(&mut st).await;
        available().map_err(CoreError::unsupported)?;

        let text = self.store.load(id)?;
        let cfg = aegis_config::parse(&text).map_err(CoreError::Config)?;
        let endpoints = resolve_endpoints(&cfg).await?;
        // Gateways must be read before routes change.
        let gw4 = default_gateway(false).await;
        let gw6 = default_gateway(true).await;

        match self.bring_up(&mut st, id, &cfg, &endpoints, gw4, gw6).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.teardown(&mut st).await;
                Err(e)
            }
        }
    }

    async fn bring_up(
        &self,
        st: &mut State,
        id: ProfileId,
        cfg: &WgConfig,
        endpoints: &[Option<SocketAddr>],
        gw4: Option<Gateway>,
        gw6: Option<Gateway>,
    ) -> Result<()> {
        let _ = std::fs::remove_file(NAME_FILE);
        let child = Command::new(WIREGUARD_GO)
            .args(["-f", "utun"])
            .env("WG_TUN_NAME_FILE", NAME_FILE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| CoreError::backend(format!("cannot start wireguard-go: {e}")))?;
        // Registered immediately so teardown stops it on any later failure.
        st.active = Some(Active {
            id,
            iface: String::new(),
            child,
            host_routes: Vec::new(),
            bypass_routes: Vec::new(),
        });

        let mut iface = None;
        for _ in 0..50 {
            if let Ok(n) = std::fs::read_to_string(NAME_FILE) {
                let n = n.trim().to_owned();
                if pf::valid_tunnel_interface(&n) {
                    iface = Some(n);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let iface = iface
            .ok_or_else(|| CoreError::backend("wireguard-go did not create a tunnel interface"))?;
        if let Some(a) = st.active.as_mut() {
            a.iface.clone_from(&iface);
        }

        let reply = uapi_call(&iface, &uapi::set_request(cfg, endpoints)).await?;
        if !uapi::set_succeeded(&reply) {
            return Err(CoreError::backend(
                "wireguard-go rejected the configuration",
            ));
        }

        for addr in &cfg.interface.addresses {
            run(IFCONFIG, &netcfg::ifconfig_address(&iface, addr)).await?;
        }
        let mtu = cfg.interface.mtu.unwrap_or(DEFAULT_MTU);
        run(
            IFCONFIG,
            &[iface.clone(), "mtu".into(), mtu.to_string(), "up".into()],
        )
        .await?;

        // Endpoint host routes first, or the handshake loops into the tunnel.
        for ep in endpoints.iter().flatten() {
            let gw = if ep.is_ipv4() { &gw4 } else { &gw6 };
            if let Some(gw) = gw {
                run(ROUTE, &netcfg::route_add_endpoint(ep.ip(), gw)).await?;
                if let Some(a) = st.active.as_mut() {
                    a.host_routes.push(ep.ip());
                }
            }
        }
        // Bypass networks go via the pre-VPN gateway; their longer prefix wins
        // over the tunnel's split default routes.
        let bypass = st.bypass.clone();
        for net in &bypass {
            let gw = if net.addr().is_ipv4() { &gw4 } else { &gw6 };
            let Some(gw) = gw else { continue };
            if run(ROUTE, &netcfg::route_add_bypass(net, gw)).await.is_ok()
                && let Some(a) = st.active.as_mut()
            {
                a.bypass_routes.push(*net);
            }
        }
        for peer in &cfg.peers {
            for net in &peer.allowed_ips {
                for n in netcfg::expand_route(net) {
                    run(ROUTE, &netcfg::route_add_interface(&n, &iface)).await?;
                }
            }
        }

        if st.dns_override.is_empty() {
            Self::set_dns(&cfg.interface.dns).await?;
        } else {
            let custom = st.dns_override.clone();
            Self::set_dns(&custom).await?;
        }

        // A kill switch asked for before the interface existed now learns its name.
        if let Some(spec) = st.kill.as_ref().filter(|k| k.id == id) {
            let rules = Self::render(spec, Some(&iface), &st.bypass)?;
            self.pf_load(st, &rules).await?;
        }
        tracing::info!("tunnel up on {iface}");
        Ok(())
    }

    /// Undo everything `bring_up` did except the kill switch, which is
    /// re-rendered without the interface so it keeps blocking.
    async fn teardown(&self, st: &mut State) {
        Self::restore_dns().await;
        if let Some(mut a) = st.active.take() {
            for ip in &a.host_routes {
                let _ = run(ROUTE, &netcfg::route_delete_host(*ip)).await;
            }
            for net in &a.bypass_routes {
                let _ = run(ROUTE, &netcfg::route_delete_net(net)).await;
            }
            let _ = a.child.start_kill();
            let _ = a.child.wait().await;
            let _ = std::fs::remove_file(NAME_FILE);
            let rules = st
                .kill
                .as_ref()
                .and_then(|k| Self::render(k, None, &st.bypass).ok());
            if let Some(Err(e)) = match rules {
                Some(r) => Some(self.pf_load(st, &r).await),
                None => None,
            } {
                tracing::error!("could not tighten the kill switch after teardown: {e}");
            }
        }
    }

    pub async fn deactivate(&self, id: ProfileId) -> Result<()> {
        let mut st = self.state.lock().await;
        if st.active.as_ref().is_some_and(|a| a.id == id) {
            self.teardown(&mut st).await;
        }
        Ok(())
    }

    /// Stop any tunnel (daemon shutdown). The kill switch stays.
    pub async fn shutdown(&self) {
        let mut st = self.state.lock().await;
        self.teardown(&mut st).await;
    }

    pub async fn status(&self) -> Result<Option<TunnelStatus>> {
        let mut st = self.state.lock().await;
        let Some(a) = st.active.as_mut() else {
            return Ok(None);
        };
        if a.child.try_wait().map_err(CoreError::backend)?.is_some() {
            let id = a.id;
            self.teardown(&mut st).await;
            return Ok(Some(TunnelStatus {
                id,
                link: LinkState::Down,
                interface: None,
                rx_bytes: 0,
                tx_bytes: 0,
                last_handshake_ms: None,
            }));
        }
        let (id, iface) = (a.id, a.iface.clone());
        let stats = uapi::parse_get(&uapi_call(&iface, "get=1\n\n").await?);
        let _ = now_ms;
        Ok(Some(TunnelStatus {
            id,
            link: LinkState::Up,
            interface: Some(iface),
            rx_bytes: stats.rx_bytes,
            tx_bytes: stats.tx_bytes,
            last_handshake_ms: stats.last_handshake_ms,
        }))
    }
}
