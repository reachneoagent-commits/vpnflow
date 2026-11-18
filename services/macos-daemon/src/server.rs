//! Unix-socket server. Bounded: a fixed number of connections, an idle
//! timeout per request, frames capped by `proto::MAX_FRAME`, mutating
//! requests one at a time. Every connection's peer uid is checked.

use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aegis_windows::proto::{self, ErrorKind, FrameError, Request, Response};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::handler::Handler;

const MAX_CONNECTIONS: usize = 8;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Root, or whoever owns the console (the logged-in desktop user).
pub fn console_user_policy(uid: u32) -> bool {
    uid == 0 || std::fs::metadata("/dev/console").is_ok_and(|m| m.uid() == uid)
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
    )
}

pub struct Server<A> {
    listener: UnixListener,
    path: PathBuf,
    allow: A,
}

impl<A: Fn(u32) -> bool + Send + Sync + 'static> Server<A> {
    /// Replaces a stale socket, but never anything that is not a socket.
    pub fn bind(path: &Path, allow: A) -> io::Result<Self> {
        if let Ok(md) = std::fs::symlink_metadata(path) {
            if md.file_type().is_socket() {
                std::fs::remove_file(path)?;
            } else {
                return Err(io::Error::other(
                    "the socket path exists and is not a socket",
                ));
            }
        }
        let listener = UnixListener::bind(path)?;
        // Access control is the peer-uid check, not the file mode.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
        Ok(Self {
            listener,
            path: path.to_owned(),
            allow,
        })
    }

    pub async fn run<H: Handler>(self, handler: Arc<H>, stop: CancellationToken) {
        let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let lock = Arc::new(Mutex::new(()));
        let allow = Arc::new(self.allow);
        loop {
            let stream = tokio::select! {
                () = stop.cancelled() => break,
                r = self.listener.accept() => match r {
                    Ok((s, _)) => s,
                    Err(e) => { tracing::warn!("accept failed: {e}"); continue; }
                },
            };
            let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
                continue; // too many clients: drop this one
            };
            let (h, l, a) = (Arc::clone(&handler), Arc::clone(&lock), Arc::clone(&allow));
            tokio::spawn(async move {
                serve_one(stream, h, l, a).await;
                drop(permit);
            });
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn serve_one<H: Handler, A: Fn(u32) -> bool>(
    mut s: UnixStream,
    handler: Arc<H>,
    lock: Arc<Mutex<()>>,
    allow: Arc<A>,
) {
    let uid = match s.peer_cred() {
        Ok(c) => c.uid(),
        Err(_) => return,
    };
    if !allow(uid) {
        tracing::warn!("refused a connection from uid {uid}");
        let _ = proto::write_frame(
            &mut s,
            &Response::Error {
                kind: ErrorKind::PermissionDenied,
                message: "only the logged-in user may control VPNFlow".into(),
            },
        )
        .await;
        return;
    }
    loop {
        let req = match tokio::time::timeout(IDLE_TIMEOUT, proto::read_frame::<_, Request>(&mut s))
            .await
        {
            Ok(Ok(Some(r))) => r,
            Ok(Err(FrameError::Malformed | FrameError::TooLarge(_))) => {
                let _ = proto::write_frame(
                    &mut s,
                    &Response::Error {
                        kind: ErrorKind::Invalid,
                        message: "malformed request".into(),
                    },
                )
                .await;
                return;
            }
            _ => return,
        };
        let reply = if is_mutating(&req) {
            let _g = lock.lock().await;
            handler.handle(req).await
        } else {
            handler.handle(req).await
        };
        if proto::write_frame(&mut s, &reply).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::ProfileId;

    struct Fake;
    impl Handler for Fake {
        async fn handle(&self, req: Request) -> Response {
            match req {
                Request::Installed => Response::Installed {
                    ids: vec![ProfileId::new()],
                },
                _ => Response::Ok,
            }
        }
    }

    async fn call(path: &Path, req: Request) -> Response {
        let mut c = UnixStream::connect(path).await.unwrap();
        proto::write_frame(&mut c, &req).await.unwrap();
        proto::read_frame::<_, Response>(&mut c)
            .await
            .unwrap()
            .unwrap()
    }

    fn sock() -> PathBuf {
        std::env::temp_dir().join(format!("vf-{}.sock", uuid::Uuid::new_v4().simple()))
    }

    #[tokio::test]
    async fn serves_allowed_and_refuses_others() {
        let (p1, p2) = (sock(), sock());
        let stop = CancellationToken::new();
        let a = Server::bind(&p1, |_| true).unwrap();
        let b = Server::bind(&p2, |_| false).unwrap();
        tokio::spawn(a.run(Arc::new(Fake), stop.clone()));
        tokio::spawn(b.run(Arc::new(Fake), stop.clone()));

        assert!(
            matches!(call(&p1, Request::Installed).await, Response::Installed { ids } if ids.len() == 1)
        );
        assert!(matches!(
            call(&p2, Request::Installed).await,
            Response::Error {
                kind: ErrorKind::PermissionDenied,
                ..
            }
        ));
        stop.cancel();
    }

    #[test]
    fn refuses_to_replace_a_regular_file() {
        let p = std::env::temp_dir().join(format!("vf-file-{}", uuid::Uuid::new_v4().simple()));
        std::fs::write(&p, b"x").unwrap();
        assert!(Server::bind(&p, |_| true).is_err());
        std::fs::remove_file(p).unwrap();
    }
}
