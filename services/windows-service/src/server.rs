//! The pipe server loop. Bounded: a fixed number of connections, an idle timeout
//! per request, frames capped by `proto::MAX_FRAME`. Mutating requests run one
//! at a time under a single lock, like the engine's operation lock.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use aegis_windows::proto::{self, ErrorKind, FrameError, Request, Response};
use tokio::net::windows::named_pipe::NamedPipeServer;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::handler::Handler;
use crate::security::{SecurityDescriptor, create_server};

/// Connections served at once. One more instance is always listening.
const MAX_CONNECTIONS: usize = 8;
/// A client that sends nothing for this long is dropped.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Name of a request for logs. Never print the request itself: it can hold a config.
fn command_name(req: &Request) -> &'static str {
    match req {
        Request::Hello { .. } => "hello",
        Request::Capabilities => "capabilities",
        Request::Install { .. } => "install",
        Request::Uninstall { .. } => "uninstall",
        Request::Load { .. } => "load",
        Request::Installed => "installed",
        Request::Activate { .. } => "activate",
        Request::Deactivate { .. } => "deactivate",
        Request::Status => "status",
        Request::ApplyKillSwitch { .. } => "apply_kill_switch",
        Request::RemoveKillSwitch => "remove_kill_switch",
        Request::KillSwitchActive => "kill_switch_active",
        Request::SetBypass { .. } => "set_bypass",
        Request::SetDns { .. } => "set_dns",
        Request::SetTailscaleRelays { .. } => "set_tailscale_relays",
        Request::SetTailnetDns { .. } => "set_tailnet_dns",
        Request::SetSiteBypass { .. } => "set_site_bypass",
    }
}

fn is_mutating(req: &Request) -> bool {
    matches!(
        req,
        Request::Install { .. }
            | Request::Uninstall { .. }
            | Request::Activate { .. }
            | Request::Deactivate { .. }
            | Request::ApplyKillSwitch { .. }
            | Request::RemoveKillSwitch
            | Request::SetBypass { .. }
            | Request::SetDns { .. }
            | Request::SetTailscaleRelays { .. }
            | Request::SetTailnetDns { .. }
            | Request::SetSiteBypass { .. }
    )
}

const MAX_INSTANCES: usize = MAX_CONNECTIONS + 2;

/// A pipe the service owns but is not yet serving. Binding is separate from
/// running so the caller can report failure (name already taken) before it
/// tells the Service Control Manager that it is running.
pub struct PipeServer {
    name: String,
    security: SecurityDescriptor,
    listener: NamedPipeServer,
}

impl PipeServer {
    /// Fails if any other process already owns the pipe name. Must be called
    /// inside a tokio runtime.
    pub fn bind(name: &str) -> io::Result<Self> {
        Self::bind_with(name, SecurityDescriptor::production()?)
    }

    /// Like [`bind`](Self::bind) with a caller-chosen DACL. Only tests pass
    /// anything but the production one.
    pub fn bind_with(name: &str, security: SecurityDescriptor) -> io::Result<Self> {
        let listener = create_server(name, &security, true, MAX_INSTANCES)?;
        Ok(Self {
            name: name.to_owned(),
            security,
            listener,
        })
    }

    /// Serve until `shutdown` is cancelled.
    pub async fn run<H: Handler>(
        self,
        handler: Arc<H>,
        shutdown: CancellationToken,
    ) -> io::Result<()> {
        let Self {
            name,
            security,
            mut listener,
        } = self;
        let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let op_lock = Arc::new(Mutex::new(()));
        tracing::info!("pipe server listening");

        loop {
            let connected = tokio::select! {
                () = shutdown.cancelled() => return Ok(()),
                r = listener.connect() => r,
            };
            // Keep one free instance listening before doing anything with this client.
            let fresh = create_server(&name, &security, false, MAX_INSTANCES)?;
            let client = std::mem::replace(&mut listener, fresh);
            if let Err(e) = connected {
                tracing::warn!(os_error = ?e.raw_os_error(), "pipe accept failed");
                continue;
            }
            match Arc::clone(&slots).try_acquire_owned() {
                Ok(permit) => {
                    tokio::spawn(connection(
                        client,
                        Arc::clone(&handler),
                        Arc::clone(&op_lock),
                        shutdown.clone(),
                        permit,
                    ));
                }
                Err(_) => tracing::warn!("too many pipe connections; refusing one"),
            }
        }
    }
}

/// Bind and run in one call.
pub async fn serve<H: Handler>(
    pipe_name: &str,
    handler: Arc<H>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    PipeServer::bind(pipe_name)?.run(handler, shutdown).await
}

async fn connection<H: Handler>(
    mut pipe: NamedPipeServer,
    handler: Arc<H>,
    op_lock: Arc<Mutex<()>>,
    shutdown: CancellationToken,
    _permit: OwnedSemaphorePermit,
) {
    loop {
        let read = tokio::select! {
            () = shutdown.cancelled() => return,
            r = tokio::time::timeout(IDLE_TIMEOUT, proto::read_frame::<_, Request>(&mut pipe)) => r,
        };
        let req = match read {
            Err(_) => {
                tracing::debug!("idle client dropped");
                return;
            }
            Ok(Ok(Some(req))) => req,
            Ok(Ok(None)) => return,
            Ok(Err(e)) => {
                tracing::debug!(error = %frame_error_kind(&e), "bad request frame");
                let _ = proto::write_frame(
                    &mut pipe,
                    &Response::Error {
                        kind: ErrorKind::Invalid,
                        message: "malformed request".into(),
                    },
                )
                .await;
                return;
            }
        };

        let name = command_name(&req);
        let response = {
            let _guard = if is_mutating(&req) {
                Some(op_lock.lock().await)
            } else {
                None
            };
            handler.handle(req).await
        };
        if let Response::Error { kind, .. } = &response {
            tracing::debug!(command = name, ?kind, "request failed");
        }
        if proto::write_frame(&mut pipe, &response).await.is_err() {
            return;
        }
    }
}

fn frame_error_kind(e: &FrameError) -> &'static str {
    match e {
        FrameError::Io(_) => "io",
        FrameError::TooLarge(_) => "too_large",
        FrameError::Malformed => "malformed",
    }
}
