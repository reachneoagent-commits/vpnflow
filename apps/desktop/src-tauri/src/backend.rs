//! Runtime backend selection. The simulated backend is only ever used when
//! explicitly requested with `AEGIS_BACKEND=mock`; if no real backend works we
//! report that instead of silently pretending.

use aegis_config::WgConfig;
use aegis_core::mock::MockBackend;
use aegis_core::{
    BackendKind, Capabilities, CoreError, Feature, ProfileId, ProfileSummary, Protection, Result,
    Settings, Support, TunnelStatus, VpnBackend,
};

pub enum AnyBackend {
    // Boxed: without the Linux variant (Windows builds) the mock dwarfs the rest.
    Mock(Box<MockBackend>),
    #[cfg(target_os = "linux")]
    Nm(aegis_linux::NmBackend),
    #[cfg(target_os = "macos")]
    Mac(aegis_macos::ServiceBackend),
    #[cfg(windows)]
    Win(aegis_windows::ServiceBackend),
    Unavailable(String),
}

pub async fn select() -> AnyBackend {
    if std::env::var("AEGIS_BACKEND").as_deref() == Ok("mock") {
        tracing::warn!("using the simulated backend (AEGIS_BACKEND=mock)");
        return AnyBackend::Mock(Box::default());
    }
    #[cfg(target_os = "linux")]
    {
        match aegis_linux::NmBackend::connect().await {
            Ok(b) => AnyBackend::Nm(b),
            Err(e) => AnyBackend::Unavailable(e.to_string()),
        }
    }
    #[cfg(target_os = "macos")]
    {
        match aegis_macos::ServiceBackend::connect().await {
            Ok(b) => AnyBackend::Mac(b),
            Err(e) => AnyBackend::Unavailable(e.to_string()),
        }
    }
    #[cfg(windows)]
    {
        match aegis_windows::ServiceBackend::connect().await {
            Ok(b) => AnyBackend::Win(b),
            Err(e) => AnyBackend::Unavailable(e.to_string()),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        AnyBackend::Unavailable("this operating system is not supported".into())
    }
}

fn unavailable(reason: &str) -> CoreError {
    CoreError::unsupported(format!("no VPN backend is available: {reason}"))
}

macro_rules! delegate {
    ($self:ident, $b:ident => $call:expr) => {
        match $self {
            AnyBackend::Mock($b) => $call,
            #[cfg(target_os = "linux")]
            AnyBackend::Nm($b) => $call,
            #[cfg(target_os = "macos")]
            AnyBackend::Mac($b) => $call,
            #[cfg(windows)]
            AnyBackend::Win($b) => $call,
            AnyBackend::Unavailable(r) => Err(unavailable(r)),
        }
    };
}

impl VpnBackend for AnyBackend {
    async fn capabilities(&self) -> Result<Capabilities> {
        if let AnyBackend::Unavailable(reason) = self {
            let u = || Support::Unsupported("no backend".into());
            return Ok(Capabilities {
                backend: BackendKind::None,
                simulated: false,
                backend_version: None,
                available: Support::Unsupported(reason.clone()),
                kill_switch: u(),
                dns_protection: u(),
                ipv6_protection: u(),
                exact_handshake_time: u(),
                notes: vec![],
            });
        }
        delegate!(self, b => b.capabilities().await)
    }
    async fn install(&self, id: ProfileId, name: &str, cfg: &WgConfig) -> Result<()> {
        delegate!(self, b => b.install(id, name, cfg).await)
    }
    async fn uninstall(&self, id: ProfileId) -> Result<()> {
        delegate!(self, b => b.uninstall(id).await)
    }
    async fn load(&self, id: ProfileId) -> Result<WgConfig> {
        delegate!(self, b => b.load(id).await)
    }
    async fn installed(&self) -> Result<Vec<ProfileId>> {
        if matches!(self, AnyBackend::Unavailable(_)) {
            return Ok(vec![]);
        }
        delegate!(self, b => b.installed().await)
    }
    async fn activate(&self, id: ProfileId) -> Result<()> {
        delegate!(self, b => b.activate(id).await)
    }
    async fn deactivate(&self, id: ProfileId) -> Result<()> {
        delegate!(self, b => b.deactivate(id).await)
    }
    async fn status(&self) -> Result<Option<TunnelStatus>> {
        if matches!(self, AnyBackend::Unavailable(_)) {
            return Ok(None);
        }
        delegate!(self, b => b.status().await)
    }
    fn protection(&self, profile: &ProfileSummary, settings: &Settings) -> Protection {
        match self {
            AnyBackend::Mock(b) => b.protection(profile, settings),
            #[cfg(target_os = "linux")]
            AnyBackend::Nm(b) => b.protection(profile, settings),
            #[cfg(target_os = "macos")]
            AnyBackend::Mac(b) => b.protection(profile, settings),
            #[cfg(windows)]
            AnyBackend::Win(b) => b.protection(profile, settings),
            AnyBackend::Unavailable(_) => Protection::off(),
        }
    }

    async fn check_reachability(
        &self,
        profile: &ProfileSummary,
        status: &TunnelStatus,
    ) -> aegis_core::Feature {
        match self {
            AnyBackend::Mock(b) => b.check_reachability(profile, status).await,
            #[cfg(target_os = "linux")]
            AnyBackend::Nm(b) => b.check_reachability(profile, status).await,
            #[cfg(target_os = "macos")]
            AnyBackend::Mac(b) => b.check_reachability(profile, status).await,
            #[cfg(windows)]
            AnyBackend::Win(b) => b.check_reachability(profile, status).await,
            AnyBackend::Unavailable(_) => aegis_core::Feature::Off,
        }
    }

    async fn verify_protection(
        &self,
        profile: &ProfileSummary,
        settings: &Settings,
        status: &TunnelStatus,
    ) -> Protection {
        match self {
            AnyBackend::Mock(b) => b.verify_protection(profile, settings, status).await,
            #[cfg(target_os = "linux")]
            AnyBackend::Nm(b) => b.verify_protection(profile, settings, status).await,
            #[cfg(target_os = "macos")]
            AnyBackend::Mac(b) => b.verify_protection(profile, settings, status).await,
            #[cfg(windows)]
            AnyBackend::Win(b) => b.verify_protection(profile, settings, status).await,
            AnyBackend::Unavailable(_) => Protection::off(),
        }
    }

    fn system_events(&self) -> Option<tokio::sync::mpsc::Receiver<aegis_core::SystemEvent>> {
        match self {
            AnyBackend::Mock(b) => b.system_events(),
            #[cfg(target_os = "linux")]
            AnyBackend::Nm(b) => b.system_events(),
            #[cfg(target_os = "macos")]
            AnyBackend::Mac(b) => b.system_events(),
            #[cfg(windows)]
            AnyBackend::Win(b) => b.system_events(),
            AnyBackend::Unavailable(_) => None,
        }
    }

    async fn apply_kill_switch(
        &self,
        id: ProfileId,
        endpoints: &[std::net::SocketAddr],
        allow_lan: bool,
    ) -> Result<()> {
        delegate!(self, b => b.apply_kill_switch(id, endpoints, allow_lan).await)
    }

    async fn apply_bypass(&self, nets: &[ipnet::IpNet]) -> Result<()> {
        delegate!(self, b => b.apply_bypass(nets).await)
    }

    async fn remove_kill_switch(&self) -> Result<()> {
        // Nothing was ever applied without a backend; removing is a no-op.
        if matches!(self, AnyBackend::Unavailable(_)) {
            return Ok(());
        }
        delegate!(self, b => b.remove_kill_switch().await)
    }

    async fn kill_switch_active(&self) -> Result<bool> {
        match self {
            AnyBackend::Mock(b) => b.kill_switch_active().await,
            #[cfg(target_os = "linux")]
            AnyBackend::Nm(b) => b.kill_switch_active().await,
            #[cfg(target_os = "macos")]
            AnyBackend::Mac(b) => b.kill_switch_active().await,
            #[cfg(windows)]
            AnyBackend::Win(b) => b.kill_switch_active().await,
            AnyBackend::Unavailable(_) => Ok(false),
        }
    }

    async fn check_public_ip(&self, profile: &ProfileSummary, status: &TunnelStatus) -> Feature {
        match self {
            AnyBackend::Mock(b) => b.check_public_ip(profile, status).await,
            #[cfg(target_os = "linux")]
            AnyBackend::Nm(b) => b.check_public_ip(profile, status).await,
            #[cfg(target_os = "macos")]
            AnyBackend::Mac(b) => b.check_public_ip(profile, status).await,
            #[cfg(windows)]
            AnyBackend::Win(b) => b.check_public_ip(profile, status).await,
            AnyBackend::Unavailable(_) => Feature::Off,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wrapper once dropped the kill-switch calls (trait defaults refused),
    /// which made the feature fail in the real app while backend tests passed.
    #[tokio::test]
    async fn kill_switch_calls_reach_the_wrapped_backend() {
        let b = AnyBackend::Mock(Box::new(MockBackend::with_test_kill_switch()));
        let id = ProfileId::new();
        let ep = ["203.0.113.5:51820".parse().unwrap()];
        b.apply_kill_switch(id, &ep, false).await.unwrap();
        assert!(b.kill_switch_active().await.unwrap());
        b.remove_kill_switch().await.unwrap();
        assert!(!b.kill_switch_active().await.unwrap());
    }

    #[tokio::test]
    async fn firewall_inspection_errors_reach_the_caller() {
        let mock = MockBackend::with_test_kill_switch();
        mock.fail_kill_switch_status(true);
        let backend = AnyBackend::Mock(Box::new(mock));
        assert!(backend.kill_switch_active().await.is_err());
    }

    #[tokio::test]
    async fn unavailable_backend_never_blocks_and_never_pretends() {
        let b = AnyBackend::Unavailable("test".into());
        assert!(
            b.apply_kill_switch(ProfileId::new(), &[], false)
                .await
                .is_err()
        );
        assert!(!b.kill_switch_active().await.unwrap());
        assert!(b.remove_kill_switch().await.is_ok());
    }
}
