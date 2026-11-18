//! macOS backend. Like Windows, the app never touches the network stack: it
//! talks to a root launchd daemon (`services/macos-daemon`, not written yet)
//! over a Unix socket using the same framed-JSON protocol as the Windows
//! service. The daemon will run `wireguard-go` on a `utun`, set routes and DNS,
//! and own the `pf` kill switch (`pf` module). See DECISIONS D17.
//!
//! Status: the client and the pf ruleset exist and are unit-tested; the daemon
//! does not, so `connect()` reports the backend unsupported until the daemon
//! is installed. Nothing here has run against a real tunnel.

pub mod pf;

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
        Capabilities, CoreError, ProfileId, ProfileSummary, Protection, Result, Settings,
        TunnelStatus, VpnBackend,
    };
    use aegis_windows::proto::{self, Request, Response, SecretText};

    use crate::SOCKET_PATH;

    const CALL_TIMEOUT: Duration = Duration::from_secs(60);
    const UNEXPECTED: &str = "unexpected reply from the VPNFlow daemon";

    pub struct ServiceBackend(());

    /// The socket must be a socket, owned by root and not writable by others,
    /// otherwise a local user could impersonate the daemon and read configs.
    fn verify_socket(path: &str) -> Result<()> {
        let md = std::fs::symlink_metadata(path).map_err(|_| {
            CoreError::unsupported(
                "the VPNFlow background service is not installed. Install it from the VPNFlow \
                 installer (it needs administrator approval once)",
            )
        })?;
        if !md.file_type().is_socket() || md.uid() != 0 || md.mode() & 0o002 != 0 {
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
        /// Nothing is reported as protected until the daemon verifies it.
        fn protection(&self, _: &ProfileSummary, _: &Settings) -> Protection {
            Protection::off()
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
