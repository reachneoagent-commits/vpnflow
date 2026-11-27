//! `aegis-helper`: the only Aegis code that runs as root (via pkexec).
//!
//! It manages one dedicated nftables table, `inet aegis_vpn`, and never
//! touches other tables or global state. Usage:
//!
//! ```text
//! aegis-helper apply <iface> [--allow-lan] <ip:port>...   install or replace the kill switch
//! aegis-helper remove                                     delete the table (idempotent)
//! aegis-helper handshake <iface>                          newest handshake time, read-only
//! aegis-helper status                                     verified block state, read-only
//! ```
//!
//! While applied, everything is blocked except loopback, the tunnel
//! interface, UDP to the WireGuard endpoints, DHCP and IPv6 neighbour
//! discovery (plus private/link-local ranges with `--allow-lan`). Rules live in
//! the kernel, so they keep blocking if the app crashes. The status command
//! inspects that kernel state directly, including after an interrupted apply.
//!
//! Every argument is validated before anything runs; the environment from
//! pkexec is ignored and `nft` is called by absolute path with a clean env.

#[cfg(unix)]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

const TABLE: &str = "aegis_vpn";
const NFT: &str = "/usr/sbin/nft";
const WG: &str = "/usr/bin/wg";
const MARKER_DIR: &str = "/run/aegis-vpn";
const MARKER: &str = "/run/aegis-vpn/kill-switch";
const MAX_ENDPOINTS: usize = 16;

const LAN4: &str =
    "10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, 224.0.0.0/4, 255.255.255.255";
const LAN6: &str = "fc00::/7, fe80::/10, ff00::/8";
/// Tailscale-style mesh networks (Tailscale, Headscale/TailFlow), which keep
/// working beside the kill switch: device addresses and MagicDNS, and the home
/// network over a subnet route (sent with a tailnet source address).
const TAILNET4: &str = "100.64.0.0/10";
const TAILNET6: &str = "fd7a:115c:a1e0::/48";
/// Tailscale's fixed WireGuard port: its direct peer links leave the physical
/// interface, so they are passed by port (relays over TCP 443 are not).
const TAILSCALE_PORT: u16 = 41641;

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    Apply {
        iface: String,
        allow_lan: bool,
        endpoints: Vec<SocketAddr>,
    },
    Remove,
    Status,
    /// Print the newest peer handshake time (unix seconds, 0 = none yet).
    Handshake {
        iface: String,
    },
}

fn parse(args: &[String]) -> Result<Cmd, String> {
    match args.first().map(String::as_str) {
        Some("remove") if args.len() == 1 => Ok(Cmd::Remove),
        Some("status") if args.len() == 1 => Ok(Cmd::Status),
        Some("handshake") if args.len() == 2 && valid_iface(&args[1]) => Ok(Cmd::Handshake {
            iface: args[1].clone(),
        }),
        Some("handshake") => Err("usage: aegis-helper handshake <aegis-iface>".into()),
        Some("apply") => {
            let iface = args.get(1).ok_or("missing interface")?;
            if !valid_iface(iface) {
                return Err("interface must be an Aegis tunnel (aegis-<8 hex digits>)".into());
            }
            let mut allow_lan = false;
            let mut endpoints = Vec::new();
            for a in &args[2..] {
                if a == "--allow-lan" {
                    allow_lan = true;
                    continue;
                }
                let ep: SocketAddr = a
                    .parse()
                    .map_err(|_| "endpoints must be IP:port literals".to_string())?;
                if ep.port() == 0 || ep.ip().is_unspecified() || ep.ip().is_multicast() {
                    return Err("endpoint address or port is not usable".into());
                }
                endpoints.push(ep);
            }
            if endpoints.is_empty() || endpoints.len() > MAX_ENDPOINTS {
                return Err(format!("give 1 to {MAX_ENDPOINTS} endpoints"));
            }
            Ok(Cmd::Apply {
                iface: iface.clone(),
                allow_lan,
                endpoints,
            })
        }
        _ => Err("usage: aegis-helper apply <iface> [--allow-lan] <ip:port>... | remove | status | handshake <iface>".into()),
    }
}

/// Exactly what `aegis_linux::settings::iface_name` produces.
fn valid_iface(s: &str) -> bool {
    s.strip_prefix("aegis-").is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// One atomic transaction: creating then deleting the table first makes the
/// script work whether or not it exists, and nft applies it all or nothing.
fn ruleset(iface: &str, allow_lan: bool, endpoints: &[SocketAddr]) -> String {
    let mut ep = String::new();
    for e in endpoints {
        let fam = if e.is_ipv4() { "ip" } else { "ip6" };
        ep += &format!("\t\t{fam} daddr {} udp dport {} accept\n", e.ip(), e.port());
    }
    let (lan_out, lan_in) = if allow_lan {
        (
            format!("\t\tip daddr {{ {LAN4} }} accept\n\t\tip6 daddr {{ {LAN6} }} accept\n"),
            format!("\t\tip saddr {{ {LAN4} }} accept\n\t\tip6 saddr {{ {LAN6} }} accept\n"),
        )
    } else {
        (String::new(), String::new())
    };
    format!(
        "table inet {TABLE} {{}}
delete table inet {TABLE}
table inet {TABLE} {{
\tchain output {{
\t\ttype filter hook output priority filter - 10; policy drop;
\t\toifname \"lo\" accept
\t\toifname \"{iface}\" accept
{ep}\t\tip daddr {TAILNET4} accept
\t\tip saddr {TAILNET4} accept
\t\tip6 daddr {TAILNET6} accept
\t\tip6 saddr {TAILNET6} accept
\t\tudp sport {TAILSCALE_PORT} accept
\t\tudp sport 68 udp dport 67 accept
\t\tudp sport 546 udp dport 547 accept
\t\ticmpv6 type {{ nd-router-solicit, nd-neighbor-solicit, nd-neighbor-advert, mld2-listener-report }} accept
{lan_out}\t\treject with icmpx type admin-prohibited
\t}}
\tchain input {{
\t\ttype filter hook input priority filter - 10; policy drop;
\t\tiifname \"lo\" accept
\t\tiifname \"{iface}\" accept
\t\tct state established,related accept
\t\tip saddr {TAILNET4} accept
\t\tip6 saddr {TAILNET6} accept
\t\tudp dport {TAILSCALE_PORT} accept
\t\tudp sport 67 udp dport 68 accept
\t\tudp sport 547 udp dport 546 accept
\t\ticmpv6 type {{ nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert, nd-redirect }} accept
{lan_in}\t}}
}}
"
    )
}

fn nft(args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    let mut child = Command::new(NFT)
        .args(args)
        .env_clear()
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {NFT}: {e}; install nftables"))?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take())
        && let Err(e) = pipe.write_all(text.as_bytes())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("writing to nft: {e}"));
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("waiting for nft: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        // nft can echo rules containing imported endpoint addresses.
        Err(format!(
            "nft {} failed ({}); check nftables permissions and firewall configuration",
            args.join(" "),
            out.status
        ))
    }
}

fn installed_rules(listed: &str, iface: Option<&str>) -> bool {
    let owned_iface = listed.lines().find_map(|line| {
        line.trim()
            .strip_prefix("oifname \"")?
            .strip_suffix("\" accept")
            .filter(|name| valid_iface(name))
    });
    owned_iface.is_some_and(|name| iface.is_none_or(|want| want == name))
        && listed.contains("chain output {")
        && listed.contains("type filter hook output")
        && listed.contains("chain input {")
        && listed.contains("type filter hook input")
        && listed.matches("policy drop").count() == 2
}

fn table_exists(
    nft: &mut impl FnMut(&[&str], Option<&str>) -> Result<String, String>,
) -> Result<bool, String> {
    let tables = nft(&["list", "tables"], None)?;
    Ok(tables
        .lines()
        .any(|line| line.trim() == format!("table inet {TABLE}")))
}

/// Newest handshake in `wg show <iface> latest-handshakes` output
/// (`<public key>\t<unix seconds>` per peer). Only the number leaves the helper.
fn newest_handshake(text: &str) -> u64 {
    text.lines()
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

fn wg_latest_handshakes(iface: &str) -> Result<String, String> {
    let out = Command::new(WG)
        .args(["show", iface, "latest-handshakes"])
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {WG}: {e}; install wireguard-tools"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

fn effective_uid() -> Option<u32> {
    // Effective uid is the second field of the Uid line.
    fs::read_to_string("/proc/self/status").ok().and_then(|s| {
        s.lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .and_then(|v| v.split_whitespace().nth(1)?.parse().ok())
    })
}

#[cfg(unix)]
fn state_file(path: &Path, mode: u32) -> io::Result<File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || Some(meta.uid()) != effective_uid() || meta.nlink() != 1 {
        return Err(io::Error::other("unsafe helper state file"));
    }
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    Ok(file)
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn lock_runtime(dir: &Path) -> io::Result<File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match fs::DirBuilder::new().mode(0o755).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || Some(meta.uid()) != effective_uid() {
        return Err(io::Error::other("unsafe helper runtime directory"));
    }
    // Fix old permissive directories before opening any entries inside them.
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
    let lock = state_file(&dir.join("operation.lock"), 0o600)?;
    // SAFETY: flock only uses this live descriptor; closing File releases the lock.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(lock)
}

#[cfg(not(unix))]
fn lock_runtime(_dir: &Path) -> io::Result<File> {
    Err(io::Error::other("the helper requires Linux"))
}

fn apply(
    iface: &str,
    allow_lan: bool,
    endpoints: &[SocketAddr],
    dir: &Path,
    mut nft: impl FnMut(&[&str], Option<&str>) -> Result<String, String>,
) -> Result<(), String> {
    let _lock = lock_runtime(dir).map_err(|e| format!("cannot lock helper state; retry: {e}"))?;
    nft(&["-f", "-"], Some(&ruleset(iface, allow_lan, endpoints)))?;
    // A failed readback keeps the fail-closed block, which status can rediscover.
    let listed = nft(&["list", "table", "inet", TABLE], None)
        .map_err(|e| format!("cannot verify the installed block; choose Stop blocking: {e}"))?;
    if !installed_rules(&listed, Some(iface)) {
        return Err("rules did not read back as installed; choose Stop blocking".into());
    }
    Ok(())
}

fn run(cmd: Cmd) -> Result<(), String> {
    match cmd {
        Cmd::Status => {
            if !table_exists(&mut nft)? {
                println!("0");
                return Ok(());
            }
            let listed = nft(&["list", "table", "inet", TABLE], None)?;
            if !installed_rules(&listed, None) {
                return Err("the Aegis firewall table could not be verified; choose Disconnect to remove it".into());
            }
            println!("1");
            Ok(())
        }
        Cmd::Apply {
            iface,
            allow_lan,
            endpoints,
        } => apply(&iface, allow_lan, &endpoints, Path::new(MARKER_DIR), nft),
        Cmd::Handshake { iface } => {
            println!("{}", newest_handshake(&wg_latest_handshakes(&iface)?));
            Ok(())
        }
        Cmd::Remove => {
            let _lock = lock_runtime(Path::new(MARKER_DIR))
                .map_err(|e| format!("cannot lock helper state; retry: {e}"))?;
            if table_exists(&mut nft)? {
                nft(&["delete", "table", "inet", TABLE], None)?;
            }
            match std::fs::remove_file(MARKER) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("{MARKER}: {e}")),
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = parse(&args).and_then(|cmd| {
        if effective_uid() != Some(0) {
            return Err("must run as root (through pkexec)".into());
        }
        run(cmd)
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aegis-helper: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_only_strict_input() {
        assert_eq!(
            parse(&args(
                "apply aegis-0b1e8a3c --allow-lan 203.0.113.5:51820 [2001:db8::1]:51820"
            )),
            Ok(Cmd::Apply {
                iface: "aegis-0b1e8a3c".into(),
                allow_lan: true,
                endpoints: vec![
                    "203.0.113.5:51820".parse().unwrap(),
                    "[2001:db8::1]:51820".parse().unwrap()
                ],
            })
        );
        assert_eq!(parse(&args("remove")), Ok(Cmd::Remove));
        assert_eq!(parse(&args("status")), Ok(Cmd::Status));
        for bad in [
            "",
            "remove extra",
            "status extra",
            "apply",
            "apply eth0 1.2.3.4:51820",
            "apply aegis-0B1E8A3C 1.2.3.4:51820",
            "apply aegis-0b1e8a3c\" 1.2.3.4:51820",
            "apply aegis-0b1e8a3c",
            "apply aegis-0b1e8a3c vpn.example.com:51820",
            "apply aegis-0b1e8a3c 1.2.3.4:0",
            "apply aegis-0b1e8a3c 0.0.0.0:51820",
            "apply aegis-0b1e8a3c 1.2.3.4:51820;flush ruleset",
            "flush",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?} must be rejected");
        }
        let many = format!(
            "apply aegis-0b1e8a3c {}",
            "1.2.3.4:1 ".repeat(MAX_ENDPOINTS + 1)
        );
        assert!(parse(&args(&many)).is_err());
    }

    #[test]
    fn handshake_command_is_strict_and_output_is_only_a_number() {
        assert_eq!(
            parse(&args("handshake aegis-0b1e8a3c")),
            Ok(Cmd::Handshake {
                iface: "aegis-0b1e8a3c".into()
            })
        );
        for bad in [
            "handshake",
            "handshake wg0",
            "handshake aegis-0b1e8a3c extra",
            "handshake aegis-0b1e8a3c;id",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
        let text = "AAAA=\t1700000100\nBBBB=\t1700000300\nCCCC=\t0\n";
        assert_eq!(newest_handshake(text), 1_700_000_300);
        assert_eq!(newest_handshake(""), 0);
        assert_eq!(newest_handshake("garbage\nline two\n"), 0);
    }

    #[test]
    fn ruleset_touches_only_our_table_and_blocks_by_default() {
        let eps = [
            "203.0.113.5:51820".parse().unwrap(),
            "[2001:db8::1]:443".parse().unwrap(),
        ];
        let r = ruleset("aegis-0b1e8a3c", false, &eps);
        for line in r
            .lines()
            .filter(|l| l.starts_with("table") || l.starts_with("delete"))
        {
            assert!(line.contains("inet aegis_vpn"), "{line}");
        }
        assert!(!r.contains("flush"));
        assert_eq!(r.matches("policy drop").count(), 2);
        assert!(r.contains("oifname \"aegis-0b1e8a3c\" accept"));
        assert!(r.contains("ip daddr 203.0.113.5 udp dport 51820 accept"));
        assert!(r.contains("ip6 daddr 2001:db8::1 udp dport 443 accept"));
        assert!(!r.contains("192.168.0.0/16"), "LAN blocked unless asked");
        assert!(ruleset("aegis-0b1e8a3c", true, &eps).contains("192.168.0.0/16"));
        // Tailscale/TailFlow keep working beside the kill switch.
        for rule in [
            "ip daddr 100.64.0.0/10 accept",
            "ip saddr 100.64.0.0/10 accept",
            "ip6 daddr fd7a:115c:a1e0::/48 accept",
        ] {
            assert!(r.contains(rule), "{rule}");
        }
        let input = &r[r.find("chain input").unwrap()..];
        assert!(input.contains("ip saddr 100.64.0.0/10 accept"));
        assert!(input.contains("udp dport 41641 accept"));
        assert!(r[..r.find("chain input").unwrap()].contains("udp sport 41641 accept"));
    }

    #[cfg(unix)]
    #[test]
    fn runtime_paths_reject_links_and_hold_one_kernel_lock() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("state");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
        let lock = lock_runtime(&dir).unwrap();
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            lock_runtime(&dir).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(lock);
        drop(lock_runtime(&dir).unwrap());

        let victim = temp.path().join("victim");
        fs::write(&victim, "unchanged").unwrap();
        let link = dir.join("operation.lock");
        fs::remove_file(&link).unwrap();
        symlink(&victim, &link).unwrap();
        assert!(lock_runtime(&dir).is_err());
        fs::remove_file(&link).unwrap();
        fs::hard_link(&victim, &link).unwrap();
        assert!(lock_runtime(&dir).is_err());
        assert_eq!(fs::read_to_string(&victim).unwrap(), "unchanged");
        let linked_dir = temp.path().join("linked");
        symlink(&dir, &linked_dir).unwrap();
        assert!(lock_runtime(&linked_dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn verification_failure_keeps_the_block_removable() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("state");
        let mut calls = Vec::new();
        let err = apply("aegis-0b1e8a3c", false, &[], &dir, |argv, _| {
            calls.push(argv.join(" "));
            match calls.len() {
                1 => Ok(String::new()),
                2 => Err("readback failed".into()),
                _ => panic!("must not remove a fail-closed block"),
            }
        })
        .unwrap_err();
        assert!(err.contains("Stop blocking"), "{err}");
        assert_eq!(calls.len(), 2);
        assert!(!dir.join("kill-switch").exists());
    }

    #[cfg(unix)]
    #[test]
    fn apply_never_writes_through_a_legacy_marker() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("state");
        fs::create_dir(&dir).unwrap();
        let victim = temp.path().join("victim");
        fs::write(&victim, "unchanged").unwrap();
        symlink(&victim, dir.join("kill-switch")).unwrap();
        let mut call = 0;
        apply("aegis-0b1e8a3c", false, &[], &dir, |_, _| {
            call += 1;
            match call {
                1 => Ok(String::new()),
                2 => Ok(ruleset("aegis-0b1e8a3c", false, &[])),
                _ => panic!("unexpected nft call"),
            }
        })
        .unwrap();
        assert_eq!(call, 2);
        assert_eq!(fs::read_to_string(&victim).unwrap(), "unchanged");
    }

    #[test]
    fn inspection_never_treats_command_errors_as_an_absent_table() {
        let err = table_exists(&mut |_, _| Err("cannot run nft: No such file or directory".into()));
        assert!(err.is_err());
        assert!(!table_exists(&mut |_, _| Ok("table inet aegis_vpn_other\n".into())).unwrap());
        assert!(table_exists(&mut |_, _| Ok("table inet aegis_vpn\n".into())).unwrap());
        let rules = ruleset("aegis-0b1e8a3c", false, &[]);
        assert!(installed_rules(&rules, None));
        assert!(!installed_rules(&rules, Some("aegis-01234567")));
        assert!(!installed_rules(
            &rules.replace("policy drop", "policy accept"),
            None
        ));
    }
}
