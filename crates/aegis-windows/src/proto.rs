//! Wire protocol between the app (`ServiceBackend`) and the LocalSystem
//! service: one length-prefixed JSON frame per message over a local named pipe.
//!
//! Portable on purpose so framing and error mapping are unit-tested on any
//! host. The pipe itself is protected by the service's DACL (see
//! DECISIONS D16); nothing here is a security boundary on its own, but every
//! decoder bounds its input before allocating.

use std::net::{IpAddr, SocketAddr};

use aegis_core::{Capabilities, CoreError, ProfileId, TunnelStatus};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zeroize::Zeroizing;

pub const PIPE_NAME: &str = r"\\.\pipe\aegis-vpn-service";
pub const VERSION: u32 = 2;
/// A config is at most `aegis_config::MAX_CONFIG_BYTES`; JSON escaping at most
/// doubles it. Anything larger is refused before it is read.
pub const MAX_FRAME: usize = 2 * aegis_config::MAX_CONFIG_BYTES + 4096;

/// wg-quick text that contains private keys. Redacted `Debug`, wiped on drop.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretText(Zeroizing<String>);

impl SecretText {
    pub fn new(text: String) -> Self {
        Self(Zeroizing::new(text))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretText(<redacted>)")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Hello {
        version: u32,
    },
    Capabilities,
    Install {
        id: ProfileId,
        name: String,
        config: SecretText,
    },
    Uninstall {
        id: ProfileId,
    },
    Load {
        id: ProfileId,
    },
    Installed,
    Activate {
        id: ProfileId,
    },
    Deactivate {
        id: ProfileId,
    },
    Status,
    ApplyKillSwitch {
        id: ProfileId,
        endpoints: Vec<SocketAddr>,
        allow_lan: bool,
    },
    RemoveKillSwitch,
    KillSwitchActive,
    /// Destinations (IP or CIDR) that skip the tunnel. Sent before `Activate`.
    SetBypass {
        nets: Vec<String>,
    },
    /// DNS servers to use inside the tunnel instead of the profile's. Empty
    /// keeps the profile's. Sent before `Activate`.
    SetDns {
        servers: Vec<IpAddr>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Capabilities { capabilities: Capabilities },
    Config { config: SecretText },
    Installed { ids: Vec<ProfileId> },
    Status { status: Option<TunnelStatus> },
    Active { active: bool },
    Error { kind: ErrorKind, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotFound,
    Invalid,
    Unsupported,
    PermissionDenied,
    Storage,
    Backend,
}

impl Response {
    pub fn error(e: &CoreError) -> Self {
        let kind = match e {
            CoreError::NotFound => ErrorKind::NotFound,
            CoreError::Invalid { .. } | CoreError::Config(_) => ErrorKind::Invalid,
            CoreError::Unsupported { .. } => ErrorKind::Unsupported,
            CoreError::PermissionDenied { .. } => ErrorKind::PermissionDenied,
            CoreError::Storage { .. } => ErrorKind::Storage,
            CoreError::Busy
            | CoreError::Cancelled
            | CoreError::Timeout { .. }
            | CoreError::Backend { .. } => ErrorKind::Backend,
        };
        Self::Error {
            kind,
            message: e.to_string(),
        }
    }

    /// Turn a reply into the value the caller expects, mapping `Error`.
    pub fn into_result(self) -> Result<Self, CoreError> {
        match self {
            Self::Error { kind, message } => Err(match kind {
                ErrorKind::NotFound => CoreError::NotFound,
                ErrorKind::Invalid => CoreError::Invalid { message },
                ErrorKind::Unsupported => CoreError::Unsupported { message },
                ErrorKind::PermissionDenied => CoreError::PermissionDenied { message },
                ErrorKind::Storage => CoreError::Storage { message },
                ErrorKind::Backend => CoreError::Backend { message },
            }),
            ok => Ok(ok),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("pipe I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("message of {0} bytes exceeds the limit")]
    TooLarge(usize),
    #[error("malformed message")]
    Malformed,
}

pub async fn write_frame<W, T>(w: &mut W, msg: &T) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = Zeroizing::new(serde_json::to_vec(msg).map_err(|_| FrameError::Malformed)?);
    if body.len() > MAX_FRAME {
        return Err(FrameError::TooLarge(body.len()));
    }
    let len = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(body.len()))?;
    let mut frame = Zeroizing::new(Vec::with_capacity(4 + body.len()));
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    w.write_all(&frame).await?;
    w.flush().await?;
    Ok(())
}

/// `Ok(None)` is a clean end of stream before a new frame.
pub async fn read_frame<R, T>(r: &mut R) -> Result<Option<T>, FrameError>
where
    R: AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = Zeroizing::new(vec![0u8; len]);
    r.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|_| FrameError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_LIKE: &str = "PrivateKey = aGVsbG8gd29ybGQgdGhpcyBpcyBub3QgYSBrZXk=";

    #[tokio::test]
    async fn request_round_trips_over_a_pipe() {
        let (mut a, mut b) = tokio::io::duplex(1 << 16);
        let id = ProfileId::new();
        let req = Request::Install {
            id,
            name: "x".into(),
            config: SecretText::new(KEY_LIKE.into()),
        };
        write_frame(&mut a, &req).await.unwrap();
        match read_frame::<_, Request>(&mut b).await.unwrap().unwrap() {
            Request::Install {
                id: got, config, ..
            } => {
                assert_eq!(got, id);
                assert_eq!(config.expose(), KEY_LIKE);
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn debug_never_prints_the_config() {
        let req = Request::Install {
            id: ProfileId::new(),
            name: "x".into(),
            config: SecretText::new(KEY_LIKE.into()),
        };
        let shown = format!("{req:?}");
        assert!(!shown.contains("PrivateKey") && shown.contains("redacted"));
        let resp = Response::Config {
            config: SecretText::new(KEY_LIKE.into()),
        };
        assert!(!format!("{resp:?}").contains("PrivateKey"));
    }

    #[tokio::test]
    async fn oversized_frame_is_refused_before_allocating() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&u32::MAX.to_le_bytes()).await.unwrap();
        let got = read_frame::<_, Request>(&mut b).await;
        assert!(matches!(got, Err(FrameError::TooLarge(_))), "{got:?}");
    }

    #[tokio::test]
    async fn garbage_and_truncation_are_errors_not_panics() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&5u32.to_le_bytes()).await.unwrap();
        a.write_all(b"nope!").await.unwrap();
        assert!(matches!(
            read_frame::<_, Request>(&mut b).await,
            Err(FrameError::Malformed)
        ));

        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&100u32.to_le_bytes()).await.unwrap();
        a.write_all(b"{}").await.unwrap();
        drop(a);
        assert!(matches!(
            read_frame::<_, Request>(&mut b).await,
            Err(FrameError::Io(_))
        ));
    }

    #[tokio::test]
    async fn closed_stream_is_none() {
        let (a, mut b) = tokio::io::duplex(64);
        drop(a);
        assert!(read_frame::<_, Request>(&mut b).await.unwrap().is_none());
    }

    #[test]
    fn errors_keep_their_kind_across_the_wire() {
        for e in [
            CoreError::NotFound,
            CoreError::invalid("bad"),
            CoreError::unsupported("no"),
            CoreError::storage("disk"),
            CoreError::backend("boom"),
        ] {
            let back = Response::error(&e).into_result().unwrap_err();
            assert_eq!(std::mem::discriminant(&back), std::mem::discriminant(&e));
        }
        assert!(Response::Ok.into_result().is_ok());
    }
}
