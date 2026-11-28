//! Kill switch through the root helper (`services/linux-helper`), started
//! with pkexec. The app itself never gains privileges: it passes validated
//! arguments and asks the helper to inspect its kernel table.

use std::path::Path;
use std::process::{Command, Stdio};

use aegis_core::{CoreError, Result, Support};

pub const HELPER: &str = "/usr/libexec/aegis-vpn/aegis-helper";
pub const POLICY: &str = "/usr/share/polkit-1/actions/org.aegisvpn.helper.policy";
const PKEXEC: &str = "/usr/bin/pkexec";
const NFT: &str = "/usr/sbin/nft";
const WG: &str = "/usr/bin/wg";

pub fn support() -> Support {
    let missing: Vec<&str> = [
        (HELPER, "the Aegis helper"),
        (POLICY, "its Polkit policy"),
        (PKEXEC, "pkexec"),
        (NFT, "nftables (nft)"),
    ]
    .into_iter()
    .filter(|(path, _)| !Path::new(path).is_file())
    .map(|(_, what)| what)
    .collect();
    if missing.is_empty() {
        Support::Supported
    } else {
        Support::Unsupported(format!(
            "missing {}. Reinstall the Linux package, or from source run \
             `cargo build --release -p aegis-helper && sudo scripts/install-helper.sh install target/release/aegis-helper`",
            missing.join(", ")
        ))
    }
}

pub async fn active() -> Result<bool> {
    // Without an installed helper this backend cannot have installed a block.
    if !Path::new(HELPER).exists() && !Path::new("/run/aegis-vpn/kill-switch").exists() {
        return Ok(false);
    }
    parse_status(run_capture(vec!["status".into()]).await)
}

fn parse_status(reply: Result<String>) -> Result<bool> {
    match reply {
        Ok(text) if text.trim() == "1" => Ok(true),
        Ok(text) if text.trim() == "0" => Ok(false),
        Ok(_) => Err(CoreError::backend(
            "reinstall the Aegis helper; firewall status was not understood",
        )),
        Err(CoreError::Backend { message }) if message.contains("usage: aegis-helper") => {
            Err(CoreError::unsupported(
                "the installed Aegis helper is too old; reinstall the helper binary and Polkit policy together",
            ))
        }
        Err(e) => Err(e),
    }
}

/// Handshake times need the helper (root) and `wg`.
pub fn handshake_support() -> Support {
    if !matches!(support(), Support::Supported) {
        return Support::Unsupported(
            "reading WireGuard handshake times needs the Aegis helper (see the kill switch note)"
                .into(),
        );
    }
    if Path::new(WG).is_file() {
        Support::Supported
    } else {
        Support::Unsupported(
            "reading WireGuard handshake times needs wireguard-tools (`wg`); Aegis infers a server response from received data"
                .into(),
        )
    }
}

pub async fn run(args: Vec<String>) -> Result<()> {
    run_capture(args).await.map(|_| ())
}

/// Run the helper through pkexec and return its stdout. Blocking work happens
/// off the async runtime.
pub async fn run_capture(args: Vec<String>) -> Result<String> {
    let out = tokio::task::spawn_blocking(move || {
        Command::new(PKEXEC)
            .arg(HELPER)
            .args(&args)
            .stdin(Stdio::null())
            .output()
    })
    .await
    .map_err(|e| CoreError::backend(format!("kill-switch helper task failed: {e}")))?
    .map_err(|e| CoreError::unsupported(format!("cannot run pkexec: {e}")))?;
    match out.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        // pkexec: 126 = not authorized or dialog dismissed, 127 = authentication failed.
        Some(126 | 127) => Err(CoreError::PermissionDenied {
            message: "permission to use the Aegis helper was refused. Firewall changes need administrator \
                      authentication; read-only checks are allowed for the active desktop user."
                .into(),
        }),
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg = err.trim().trim_start_matches("aegis-helper: ");
            Err(CoreError::backend(format!("the kill-switch helper failed: {msg}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_requires_a_valid_reply_and_identifies_an_old_helper() {
        assert!(parse_status(Ok("1\n".into())).unwrap());
        assert!(!parse_status(Ok("0\n".into())).unwrap());
        assert!(parse_status(Ok("unknown".into())).is_err());
        assert!(matches!(
            parse_status(Err(CoreError::backend(
                "usage: aegis-helper apply ... | remove"
            ))),
            Err(CoreError::Unsupported { .. })
        ));
        assert!(matches!(
            parse_status(Err(CoreError::PermissionDenied {
                message: "denied".into()
            })),
            Err(CoreError::PermissionDenied { .. })
        ));
    }
}
