//! macOS backend. Like Windows, the app never touches the network stack: it
//! talks to a root launchd daemon (`services/macos-daemon`)
//! over a Unix socket using the same framed-JSON protocol as the Windows
//! service. The daemon will run `wireguard-go` on a `utun`, set routes and DNS,
//! and own the `pf` kill switch (`pf` module). See DECISIONS D17.
//!
//! Status: client, daemon and pf kill switch run against a real tunnel (see
//! docs/testing/TEST_RESULTS.md). `verify` checks DNS and IPv6 live.

pub mod events;
pub mod pf;
pub mod verify;

/// Socket the daemon listens on. `/var/run` is root-owned, so only root can
/// have created it; the client still checks ownership before connecting.
pub const SOCKET_PATH: &str = "/var/run/vpnflow.sock";

#[cfg(target_os = "macos")]
mod imp {
    use std::net::SocketAddr;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use std::time::Duration;

    use aegis_config::WgConfig;
    use aegis_core::{
        Capabilities, CoreError, Feature, ProfileId, ProfileSummary, Protection, Result, Settings,
        TunnelStatus, VpnBackend,
    };
    use aegis_windows::proto::{self, Request, Response, SecretText};

    use crate::SOCKET_PATH;

    const CALL_TIMEOUT: Duration = Duration::from_secs(60);
    const UNEXPECTED: &str = "unexpected reply from the VPNFlow daemon";

    pub struct ServiceBackend(());

    fn run(cmd: &str, args: &[&str]) -> String {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    }

    fn probe(iface: &str, route: aegis_config::RouteKind) -> Protection {
        use crate::verify::*;
        let servers: Vec<_> = parse_resolvers(&run("/usr/sbin/scutil", &["--dns"]))
            .into_iter()
            .map(|ip| {
                let i = route_interface(&run("/sbin/route", &["-n", "get", &ip.to_string()]));
                (ip, i)
            })
            .collect();
        let v6 = route_interface(&run(
            "/sbin/route",
            &["-n", "get", "-inet6", "2606:4700:4700::1111"],
        ));
        let native = has_native_ipv6(&run("/sbin/ifconfig", &[]));
        // No DNS server at all means the network is down: say so instead of
        // reporting a route that carries nothing.
        let ipv6 = if servers.is_empty()
            && matches!(
                route,
                aegis_config::RouteKind::Full | aegis_config::RouteKind::FullIpv4Only
            ) {
            Feature::Configured(NO_NETWORK.into())
        } else {
            judge_ipv6(route, v6.as_deref(), native, iface)
        };
        Protection {
            kill_switch: Feature::Off,
            dns: judge_dns(&servers, iface),
            ipv6,
        }
    }

    /// The socket must be a socket owned by root, otherwise a local user could
    /// impersonate the daemon and read configs. Its mode is 0666 by design (the
    /// daemon authorizes by peer uid), and only root can create or replace
    /// entries in `/var/run`, so the mode is not checked here.
    fn verify_socket(path: &str) -> Result<()> {
        let md = std::fs::symlink_metadata(path).map_err(|_| {
            CoreError::unsupported(
                "the VPNFlow background service is not installed. Install it from the VPNFlow \
                 installer (it needs administrator approval once)",
            )
        })?;
        if !md.file_type().is_socket() || md.uid() != 0 {
            return Err(CoreError::unsupported(
                "the VPNFlow service socket has unsafe ownership or permissions; reinstall VPNFlow",
            ));
        }
        Ok(())
    }

    async fn call(req: Request) -> Result<Response> {
        tokio::time::timeout(CALL_TIMEOUT, async {
            verify_socket(SOCKET_PATH)?;
            let mut s = tokio::net::UnixStream::connect(SOCKET_PATH)
                .await
                .map_err(|e| CoreError::backend(format!("cannot reach the VPNFlow daemon: {e}")))?;
            proto::write_frame(&mut s, &req)
                .await
                .map_err(CoreError::backend)?;
            let reply = proto::read_frame::<_, Response>(&mut s)
                .await
                .map_err(CoreError::backend)?
                .ok_or_else(|| CoreError::backend("the VPNFlow daemon closed the connection"))?;
            reply.into_result()
        })
        .await
        .map_err(|_| CoreError::Timeout {
            what: "the VPNFlow daemon".into(),
            seconds: CALL_TIMEOUT.as_secs(),
        })?
    }

    async fn expect_ok(req: Request) -> Result<()> {
        match call(req).await? {
            Response::Ok => Ok(()),
            _ => Err(CoreError::backend(UNEXPECTED)),
        }
    }

    impl ServiceBackend {
        /// Succeeds only when a correctly owned daemon socket exists.
        pub async fn connect() -> Result<Self> {
            verify_socket(SOCKET_PATH)?;
            Ok(Self(()))
        }
    }

    impl VpnBackend for ServiceBackend {
        async fn capabilities(&self) -> Result<Capabilities> {
            match call(Request::Capabilities).await? {
                Response::Capabilities { capabilities } => Ok(capabilities),
                _ => Err(CoreError::backend(UNEXPECTED)),
            }
        }
        async fn install(&self, id: ProfileId, name: &str, cfg: &WgConfig) -> Result<()> {
            expect_ok(Request::Install {
                id,
                name: name.to_owned(),
                config: SecretText::new(cfg.to_wg_quick().to_string()),
            })
            .await
        }
        async fn uninstall(&self, id: ProfileId) -> Result<()> {
            expect_ok(Request::Uninstall { id }).await
        }
        async fn load(&self, id: ProfileId) -> Result<WgConfig> {
            match call(Request::Load { id }).await? {
                Response::Config { config } => {
                    aegis_config::parse(config.expose()).map_err(CoreError::Config)
                }
                _ => Err(CoreError::backend(UNEXPECTED)),
            }
        }
        async fn installed(&self) -> Result<Vec<ProfileId>> {
            match call(Request::Installed).await? {
                Response::Installed { ids } => Ok(ids),
                _ => Err(CoreError::backend(UNEXPECTED)),
            }
        }
        async fn activate(&self, id: ProfileId) -> Result<()> {
            expect_ok(Request::Activate { id }).await
        }
        async fn deactivate(&self, id: ProfileId) -> Result<()> {
            expect_ok(Request::Deactivate { id }).await
        }
        async fn status(&self) -> Result<Option<TunnelStatus>> {
            match call(Request::Status).await? {
                Response::Status { status } => Ok(status),
                _ => Err(CoreError::backend(UNEXPECTED)),
            }
        }
        /// Nothing is reported as protected until it has been checked.
        fn protection(&self, _: &ProfileSummary, _: &Settings) -> Protection {
            Protection::off()
        }
        /// Live checks of the running tunnel: where the system's DNS servers
        /// and the IPv6 internet route right now. The kill switch field is
        /// replaced by the engine from the real firewall state.
        async fn verify_protection(
            &self,
            profile: &ProfileSummary,
            _: &Settings,
            status: &TunnelStatus,
        ) -> Protection {
            let Some(iface) = status.interface.clone() else {
                return Protection::off();
            };
            let route = profile.route_kind;
            tokio::task::spawn_blocking(move || probe(&iface, route))
                .await
                .unwrap_or_else(|_| Protection::off())
        }
        /// A DNS lookup sent to the profile's tunnel DNS server. Run every few
        /// seconds, so a dropped connection shows up as Failed quickly.
        async fn check_reachability(
            &self,
            profile: &ProfileSummary,
            status: &TunnelStatus,
        ) -> Feature {
            let Some(iface) = status.interface.clone() else {
                return Feature::Off;
            };
            let Some(server) = profile
                .dns
                .iter()
                .find_map(|d| d.parse::<std::net::IpAddr>().ok())
            else {
                return Feature::Unsupported(
                    "the profile has no DNS server inside the tunnel".into(),
                );
            };
            tokio::task::spawn_blocking(move || {
                let via = crate::verify::route_interface(&run(
                    "/sbin/route",
                    &["-n", "get", &server.to_string()],
                ));
                if via.as_deref() != Some(iface.as_str()) {
                    return Feature::Failed(format!(
                        "the tunnel's DNS server is reached via {}, not the tunnel",
                        via.as_deref().unwrap_or("no route")
                    ));
                }
                crate::verify::judge_reachability(crate::verify::lookup(server))
            })
            .await
            .unwrap_or(Feature::Off)
        }
        /// Egress IPv4 as the tunnel's DNS resolver sees it (whoami lookup).
        async fn check_public_ip(
            &self,
            profile: &ProfileSummary,
            status: &TunnelStatus,
        ) -> Feature {
            let Some(iface) = status.interface.clone() else {
                return Feature::Off;
            };
            let Some(server) = profile
                .dns
                .iter()
                .find_map(|d| d.parse::<std::net::IpAddr>().ok())
            else {
                return Feature::Unsupported(
                    "the profile has no DNS server inside the tunnel".into(),
                );
            };
            tokio::task::spawn_blocking(move || {
                let via = crate::verify::route_interface(&run(
                    "/sbin/route",
                    &["-n", "get", &server.to_string()],
                ));
                if via.as_deref() != Some(iface.as_str()) {
                    return Feature::Failed(format!(
                        "the tunnel's DNS server is reached via {}, not the tunnel",
                        via.as_deref().unwrap_or("no route")
                    ));
                }
                let first = crate::verify::judge_public_ip(crate::verify::lookup_public_ip(server));
                if matches!(first, Feature::Verified(_)) {
                    return first;
                }
                // The profile's DNS does not answer: ask a second source, but
                // only if that question would travel through the tunnel.
                let fb = crate::verify::FALLBACK_SERVER.to_string();
                let via = crate::verify::route_interface(&run("/sbin/route", &["-n", "get", &fb]));
                if via.as_deref() == Some(iface.as_str()) {
                    let second = crate::verify::judge_public_ip(
                        crate::verify::lookup_public_ip_fallback(),
                    );
                    if matches!(second, Feature::Verified(_)) {
                        return second;
                    }
                }
                first
            })
            .await
            .unwrap_or(Feature::Off)
        }
        fn system_events(&self) -> Option<tokio::sync::mpsc::Receiver<aegis_core::SystemEvent>> {
            Some(crate::events::spawn())
        }
        async fn apply_kill_switch(
            &self,
            id: ProfileId,
            endpoints: &[SocketAddr],
            allow_lan: bool,
        ) -> Result<()> {
            expect_ok(Request::ApplyKillSwitch {
                id,
                endpoints: endpoints.to_vec(),
                allow_lan,
            })
            .await
        }
        async fn apply_bypass(&self, nets: &[ipnet::IpNet]) -> Result<()> {
            expect_ok(Request::SetBypass {
                nets: nets.iter().map(ToString::to_string).collect(),
            })
            .await
        }
        async fn apply_dns(&self, servers: &[std::net::IpAddr]) -> Result<()> {
            expect_ok(Request::SetDns {
                servers: servers.to_vec(),
            })
            .await
        }
        async fn remove_kill_switch(&self) -> Result<()> {
            expect_ok(Request::RemoveKillSwitch).await
        }
        async fn kill_switch_active(&self) -> Result<bool> {
            match call(Request::KillSwitchActive).await? {
                Response::Active { active } => Ok(active),
                _ => Err(CoreError::backend(UNEXPECTED)),
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::ServiceBackend;
