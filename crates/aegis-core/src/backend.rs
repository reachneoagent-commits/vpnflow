use std::future::Future;

use aegis_config::WgConfig;

use crate::error::{CoreError, Result};
use crate::types::{
    Capabilities, Feature, ProfileId, ProfileSummary, Protection, Settings, TunnelStatus,
};

/// A platform networking backend.
///
/// Backends own the secret material: `install` hands the full configuration
/// to the privileged store (NetworkManager, the Windows service, or memory for
/// the mock) and `load` is the only way to get it back. Implementations must
/// only ever touch tunnels they created.
pub trait VpnBackend: Send + Sync + 'static {
    fn capabilities(&self) -> impl Future<Output = Result<Capabilities>> + Send;

    /// Create or replace a managed profile, including its secrets.
    fn install(
        &self,
        id: ProfileId,
        name: &str,
        cfg: &WgConfig,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Remove a managed profile. Removing a missing profile is not an error.
    fn uninstall(&self, id: ProfileId) -> impl Future<Output = Result<()>> + Send;

    /// Read back a managed profile, including secrets (for edit/export).
    fn load(&self, id: ProfileId) -> impl Future<Output = Result<WgConfig>> + Send;

    /// Ids of all profiles this backend manages (used for crash recovery).
    fn installed(&self) -> impl Future<Output = Result<Vec<ProfileId>>> + Send;

    /// Bring the tunnel up. Resolves once the backend reports the link up,
    /// or with an error. Callers apply their own timeout and cancellation.
    fn activate(&self, id: ProfileId) -> impl Future<Output = Result<()>> + Send;

    /// Bring the tunnel down. Deactivating an inactive profile is not an error.
    fn deactivate(&self, id: ProfileId) -> impl Future<Output = Result<()>> + Send;

    /// Status of the currently active managed tunnel, if any.
    fn status(&self) -> impl Future<Output = Result<Option<TunnelStatus>>> + Send;

    /// Protection actually in effect for an active tunnel. Must never report
    /// more than is really established.
    fn protection(&self, profile: &ProfileSummary, settings: &Settings) -> Protection;

    /// Active check of the running tunnel (route lookups, resolver state).
    /// The engine calls it on every status poll while connected and shows
    /// its result instead of [`Self::protection`]. Default: no active check.
    /// OS events (resume, network switch) for the engine to react to. Called
    /// once at start-up; `None` means this backend cannot observe them.
    fn system_events(&self) -> Option<tokio::sync::mpsc::Receiver<crate::SystemEvent>> {
        None
    }

    /// Kill switch: block all traffic except loopback, tunnel `id`'s
    /// interface, UDP to `endpoints`, DHCP and IPv6 neighbour discovery (and
    /// private/link-local ranges when `allow_lan`). Replaces an existing block
    /// atomically; returns only once the rules are in place, never half-applied.
    /// The block must survive app crashes (it lives in the kernel).
    ///
    /// These three have no default on purpose: a wrapper or new backend that
    /// forgets to forward them must fail to compile, not silently lose the
    /// kill switch.
    fn apply_kill_switch(
        &self,
        id: ProfileId,
        endpoints: &[std::net::SocketAddr],
        allow_lan: bool,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Destinations that must skip the tunnel on the next connection. Sent
    /// before the kill switch and `activate`. Default: unsupported, so a
    /// backend that cannot do it fails loudly instead of ignoring the list.
    fn apply_bypass(&self, nets: &[ipnet::IpNet]) -> impl Future<Output = Result<()>> + Send {
        let empty = nets.is_empty();
        async move {
            if empty {
                Ok(())
            } else {
                Err(CoreError::unsupported(
                    "the bypass list is not available on this system yet",
                ))
            }
        }
    }

    /// DNS servers to use inside the tunnel instead of the profile's. Sent
    /// before `activate`; empty means the profile's own. Default: unsupported
    /// when servers are given, so a backend cannot silently ignore them.
    fn apply_dns(&self, servers: &[std::net::IpAddr]) -> impl Future<Output = Result<()>> + Send {
        let empty = servers.is_empty();
        async move {
            if empty {
                Ok(())
            } else {
                Err(CoreError::unsupported(
                    "custom DNS is not available on this system yet",
                ))
            }
        }
    }

    /// Remove the kill switch block. Removing when none exists is not an error.
    fn remove_kill_switch(&self) -> impl Future<Output = Result<()>> + Send;

    /// Whether a block is installed right now, for example one left by a
    /// previous run of the app. Errors must not be treated as an absent block.
    fn kill_switch_active(&self) -> impl Future<Output = Result<bool>> + Send;

    /// Light check that the server side can reach the internet, run when the
    /// tunnel comes up and then once a minute. Default: not tested (`Off`).
    fn check_reachability(
        &self,
        profile: &ProfileSummary,
        status: &TunnelStatus,
    ) -> impl Future<Output = Feature> + Send {
        let _ = (profile, status);
        async { Feature::Off }
    }

    /// Observe the tunnel's egress IPv4 via the profile DNS (whoami). Run once
    /// when connected. Default: not tested (`Off`).
    fn check_public_ip(
        &self,
        profile: &ProfileSummary,
        status: &TunnelStatus,
    ) -> impl Future<Output = Feature> + Send {
        let _ = (profile, status);
        async { Feature::Off }
    }

    fn verify_protection(
        &self,
        profile: &ProfileSummary,
        settings: &Settings,
        status: &TunnelStatus,
    ) -> impl Future<Output = Protection> + Send {
        let _ = status;
        let p = self.protection(profile, settings);
        async move { p }
    }
}
