//! Request handling. [`Handler`] is the seam the pipe server dispatches to, so
//! the server is tested with a fake and the real tunnel code plugs in later.

use std::future::Future;
use std::sync::Arc;

use aegis_config::Severity;
use aegis_core::{BackendKind, Capabilities, CoreError, Support};
use aegis_windows::proto::{self, ErrorKind, Request, Response, SecretText};

use crate::store::{ConfigStore, StoreError};

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, req: Request) -> impl Future<Output = Response> + Send;
}

/// The service with a real encrypted config store behind it. Tunnels cannot be
/// brought up yet, so everything beyond config storage is answered by
/// [`Skeleton`] and reported `Unsupported`.
pub struct Service {
    store: Arc<ConfigStore>,
}

impl Service {
    pub fn new(store: ConfigStore) -> Self {
        Self {
            store: Arc::new(store),
        }
    }
}

fn store_error(e: &StoreError) -> Response {
    let kind = match e {
        StoreError::NotFound => ErrorKind::NotFound,
        StoreError::NameCollision => ErrorKind::Invalid,
        StoreError::UnsafeDirectory(_) | StoreError::Corrupt | StoreError::Io(_) => {
            ErrorKind::Storage
        }
    };
    Response::Error {
        kind,
        message: e.to_string(),
    }
}

/// Parse and re-validate in the service: the app's own check is never trusted.
/// Messages name fields only, never values from the config.
fn validated(text: &SecretText) -> Result<zeroize::Zeroizing<String>, String> {
    let cfg = aegis_config::parse(text.expose()).map_err(|e| CoreError::Config(e).to_string())?;
    let mut bad: Vec<String> = aegis_config::validate(&cfg)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.field.unwrap_or_else(|| "config".into()))
        .collect();
    bad.sort();
    bad.dedup();
    if !bad.is_empty() {
        return Err(format!(
            "the config is not valid ({}); fix these fields and import it again",
            bad.join(", ")
        ));
    }
    // Store the validated canonical form, which is exactly what the tunnel will load.
    Ok(cfg.to_wg_quick())
}

/// Run blocking DPAPI/file work off the async threads.
async fn blocking<T: Send + 'static>(
    store: &Arc<ConfigStore>,
    job: impl FnOnce(&ConfigStore) -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    let store = Arc::clone(store);
    tokio::task::spawn_blocking(move || job(&store))
        .await
        .unwrap_or_else(|_| Err(StoreError::Io(std::io::Error::other("storage task failed"))))
}

impl Handler for Service {
    async fn handle(&self, req: Request) -> Response {
        match req {
            Request::Install { id, config, .. } => {
                let text = match validated(&config) {
                    Ok(t) => t,
                    Err(message) => {
                        return Response::Error {
                            kind: ErrorKind::Invalid,
                            message,
                        };
                    }
                };
                match blocking(&self.store, move |s| s.save(id, &text)).await {
                    Ok(()) => Response::Ok,
                    Err(e) => store_error(&e),
                }
            }
            Request::Uninstall { id } => match blocking(&self.store, move |s| s.remove(id)).await {
                Ok(()) => Response::Ok,
                Err(e) => store_error(&e),
            },
            Request::Load { id } => match blocking(&self.store, move |s| s.load(id)).await {
                Ok(text) => Response::Config {
                    config: SecretText::new((*text).clone()),
                },
                Err(e) => store_error(&e),
            },
            Request::Installed => match blocking(&self.store, ConfigStore::list).await {
                Ok(ids) => Response::Installed { ids },
                Err(e) => store_error(&e),
            },
            other => Skeleton.handle(other).await,
        }
    }
}

/// What the service can honestly do today: speak the protocol and report that it
/// cannot bring up tunnels. Every capability is `Unsupported` with the reason, so
/// the app blocks connecting instead of pretending (AGENTS.md rule 5). Replaced
/// piece by piece as the real implementation lands.
pub struct Skeleton;

const NOT_YET: &str = "this build of the Aegis service cannot manage tunnels yet";

impl Handler for Skeleton {
    async fn handle(&self, req: Request) -> Response {
        match req {
            Request::Hello { version } if version == proto::VERSION => Response::Ok,
            Request::Hello { version } => Response::Error {
                kind: ErrorKind::Unsupported,
                message: format!(
                    "protocol version {version} is not supported (service speaks {}); \
                     reinstall Aegis VPN so the app and service match",
                    proto::VERSION
                ),
            },
            Request::Capabilities => Response::Capabilities {
                capabilities: Capabilities {
                    backend: BackendKind::WindowsService,
                    simulated: false,
                    backend_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                    available: Support::Unsupported(NOT_YET.into()),
                    kill_switch: Support::Unsupported(NOT_YET.into()),
                    dns_protection: Support::Unsupported(NOT_YET.into()),
                    ipv6_protection: Support::Unsupported(NOT_YET.into()),
                    exact_handshake_time: Support::Unsupported(NOT_YET.into()),
                    notes: Vec::new(),
                },
            },
            // True statements about an empty service.
            Request::Installed => Response::Installed { ids: Vec::new() },
            Request::Status => Response::Status { status: None },
            Request::KillSwitchActive => Response::Active { active: false },
            Request::RemoveKillSwitch | Request::Deactivate { .. } | Request::Uninstall { .. } => {
                Response::Ok
            }
            Request::Load { .. } => Response::Error {
                kind: ErrorKind::NotFound,
                message: "no such profile in the service".into(),
            },
            Request::Install { .. }
            | Request::Activate { .. }
            | Request::SetBypass { .. }
            | Request::SetDns { .. }
            | Request::ApplyKillSwitch { .. } => Response::Error {
                kind: ErrorKind::Unsupported,
                message: NOT_YET.into(),
            },
        }
    }
}
