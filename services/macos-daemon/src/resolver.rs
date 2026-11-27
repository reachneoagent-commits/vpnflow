//! Tailnet name lookups while VPNFlow is connected.
//!
//! Tailscale's "Use Tailscale DNS" makes 100.100.100.100 the Mac's resolver
//! for every name, and Tailscale then forwards ordinary lookups to public DNS
//! outside the tunnel. The kill switch blocks that (it is a DNS leak), so every
//! app loses DNS. The desktop app turns that Tailscale option off while
//! connected and asks us for a per-domain resolver instead:
//! `/etc/resolver/<tailnet domain>` sends only tailnet names to Tailscale,
//! which the kill switch already lets through. Everything else uses the
//! tunnel's DNS.
//!
//! We only ever touch files that carry [`MARKER`], so a resolver file the user
//! or another app wrote is never changed or removed.

use std::path::Path;

pub const RESOLVER_DIR: &str = "/etc/resolver";
pub const MARKER: &str = "# Written by VPNFlow while connected; removed when it disconnects.";
/// Tailscale's MagicDNS address (also used by Headscale/TailFlow).
pub const MAGIC_DNS: &str = "100.100.100.100";

/// A tailnet DNS domain such as `example-tailnet.ts.net` or a Headscale base
/// domain. Plain lowercase DNS labels only, so it is always a safe file name.
pub fn valid_domain(d: &str) -> bool {
    if d.is_empty() || d.len() > 253 || d.starts_with('.') || d.ends_with('.') {
        return false;
    }
    let labels: Vec<&str> = d.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

pub fn file_contents() -> String {
    format!("{MARKER}\nnameserver {MAGIC_DNS}\n")
}

fn ours(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.starts_with(MARKER))
}

/// Make `dir` hold exactly one VPNFlow resolver file, for `domain`, or none
/// when `domain` is `None`. Files without the marker are left alone; if one
/// already exists for `domain`, nothing is written (the user's wins).
pub fn apply(dir: &Path, domain: Option<&str>) -> std::io::Result<()> {
    if let Some(d) = domain
        && !valid_domain(d)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a tailnet domain",
        ));
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            let keep = domain.is_some_and(|d| e.file_name() == d);
            if !keep && p.is_file() && ours(&p) {
                std::fs::remove_file(&p)?;
            }
        }
    }
    let Some(d) = domain else {
        return Ok(());
    };
    let path = dir.join(d);
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, file_contents())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        assert!(valid_domain("taild0bab.ts.net"));
        assert!(valid_domain("home.example"));
        for bad in [
            "",
            "net",
            ".ts.net",
            "ts.net.",
            "a..b",
            "../etc",
            "a/b",
            "A.ts.net",
            "-a.ts.net",
            "a b.ts.net",
        ] {
            assert!(!valid_domain(bad), "{bad}");
        }
    }

    #[test]
    fn writes_switches_and_removes_only_its_own_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("corp.example"), "nameserver 10.0.0.1\n").unwrap();

        apply(d, Some("one.ts.net")).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join("one.ts.net")).unwrap(),
            file_contents()
        );

        apply(d, Some("two.ts.net")).unwrap();
        assert!(!d.join("one.ts.net").exists());
        assert!(d.join("two.ts.net").exists());

        apply(d, None).unwrap();
        assert!(!d.join("two.ts.net").exists());
        assert!(d.join("corp.example").exists(), "user file kept");
    }

    #[test]
    fn never_overwrites_a_users_file() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("x.ts.net"), "nameserver 10.0.0.1\n").unwrap();
        apply(d, Some("x.ts.net")).unwrap();
        apply(d, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join("x.ts.net")).unwrap(),
            "nameserver 10.0.0.1\n"
        );
    }

    #[test]
    fn rejects_bad_domain_without_touching_anything() {
        let dir = tempfile::tempdir().unwrap();
        apply(dir.path(), Some("one.ts.net")).unwrap();
        assert!(apply(dir.path(), Some("../x")).is_err());
        assert!(dir.path().join("one.ts.net").exists());
    }

    #[test]
    fn missing_dir_is_fine_when_clearing() {
        let dir = tempfile::tempdir().unwrap();
        apply(&dir.path().join("nope"), None).unwrap();
    }
}
