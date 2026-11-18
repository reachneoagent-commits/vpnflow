use std::future::Future;
use std::sync::Arc;

use aegis_config::Severity;
use aegis_core::{BackendKind, Capabilities, CoreError, Support};
use aegis_windows::proto::{self, Request, Response, SecretText};

use crate::store::ConfigStore;
use crate::tunnel::{self, TunnelManager};

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, req: Request) -> impl Future<Output = Response> + Send;
}

pub struct Daemon {
    store: Arc<ConfigStore>,
    tunnel: TunnelManager,
}

impl Daemon {
    pub fn new(store: Arc<ConfigStore>) -> Self {
        Self {
            tunnel: TunnelManager::new(Arc::clone(&store)),
            store,
        }
    }
    pub fn tunnel(&self) -> &TunnelManager {
        &self.tunnel
    }
}

/// Re-validate in the daemon: the app's own check is never trusted. Messages
/// name fields only, never values.
fn validated(text: &SecretText) -> Result<zeroize::Zeroizing<String>, CoreError> {
    let cfg = aegis_config::parse(text.expose()).map_err(CoreError::Config)?;
    let mut bad: Vec<String> = aegis_config::validate(&cfg)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.field.unwrap_or_else(|| "config".into()))
        .collect();
    bad.sort();
    bad.dedup();
    if !bad.is_empty() {
        return Err(CoreError::invalid(format!(
            "the config is not valid ({}); fix these fields and import it again",
            bad.join(", ")
        )));
    }
    Ok(cfg.to_wg_quick())
}

fn done(r: Result<(), CoreError>) -> Response {
    match r {
        Ok(()) => Response::Ok,
        Err(e) => Response::error(&e),
    }
}

fn capabilities() -> Capabilities {
    let avail = match tunnel::available() {
        Ok(()) => Support::Supported,
        Err(m) => Support::Unsupported(m),
    };
    let dep = |s: &Support| match s {
        Support::Supported => Support::Supported,
        Support::Unsupported(m) => Support::Unsupported(m.clone()),
    };
    Capabilities {
        backend: BackendKind::MacosDaemon,
        simulated: false,
        backend_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        kill_switch: dep(&avail),
        dns_protection: dep(&avail),
        // IPv6 is covered by the kill switch (everything not tunnelled is blocked).
        ipv6_protection: dep(&avail),
        exact_handshake_time: dep(&avail),
        available: avail,
        notes: vec!["Untested on a real Mac: see docs/development/MACOS.md".into()],
    }
}

impl Handler for Daemon {
    async fn handle(&self, req: Request) -> Response {
        match req {
            Request::Hello { version } if version == proto::VERSION => Response::Ok,
            Request::Hello { version } => done(Err(CoreError::unsupported(format!(
                "protocol version {version} is not supported (daemon speaks {}); reinstall VPNFlow",
                proto::VERSION
            )))),
            Request::Capabilities => Response::Capabilities {
                capabilities: capabilities(),
            },
            Request::Install { id, config, .. } => {
                let store = Arc::clone(&self.store);
                let r = match validated(&config) {
                    Ok(text) => tokio::task::spawn_blocking(move || store.save(id, &text))
                        .await
                        .unwrap_or_else(|_| Err(CoreError::storage("storage task failed"))),
                    Err(e) => Err(e),
                };
                done(r)
            }
            Request::Uninstall { id } => {
                let _ = self.tunnel.deactivate(id).await;
                done(self.store.remove(id))
            }
            Request::Load { id } => match self.store.load(id) {
                Ok(t) => Response::Config {
                    config: SecretText::new((*t).clone()),
                },
                Err(e) => Response::error(&e),
            },
            Request::Installed => match self.store.list() {
                Ok(ids) => Response::Installed { ids },
                Err(e) => Response::error(&e),
            },
            Request::Activate { id } => done(self.tunnel.activate(id).await),
            Request::Deactivate { id } => done(self.tunnel.deactivate(id).await),
            Request::Status => match self.tunnel.status().await {
                Ok(status) => Response::Status { status },
                Err(e) => Response::error(&e),
            },
            Request::ApplyKillSwitch {
                id,
                endpoints,
                allow_lan,
            } => done(
                self.tunnel
                    .apply_kill_switch(id, &endpoints, allow_lan)
                    .await,
            ),
            Request::RemoveKillSwitch => done(self.tunnel.remove_kill_switch().await),
            Request::KillSwitchActive => match self.tunnel.kill_switch_active().await {
                Ok(active) => Response::Active { active },
                Err(e) => Response::error(&e),
            },
        }
    }
}
