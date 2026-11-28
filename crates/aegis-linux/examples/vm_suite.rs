//! Disruptive live tests: run ONLY inside the disposable test VM
//! (`scripts/vm/run-vm-tests.sh` copies and runs this as the `aegis` user).
//!
//!   vm_suite <full|full4> <path to test-netns-peer.sh>
//!
//! Covers full-tunnel routing, endpoint exclusion, DNS leak (resolver answer +
//! packet capture on the uplink), IPv6 routing/leak reporting, reconnect after
//! an external drop, NetworkManager restart, and restoration of routes, rules
//! and DNS after disconnect. Root steps use `sudo -n` (passwordless in the VM).

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    use checks::*;
    use std::time::{Duration, Instant};

    use aegis_core::{ConnectionState, Engine, Feature, PeerState, VpnBackend as _};

    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .expect("usage: vm_suite <full|full4> <peer script>");
    let peer_script = args
        .next()
        .expect("usage: vm_suite <full|full4> <peer script>");
    assert!(
        mode == "full" || mode == "full4",
        "mode must be full or full4"
    );
    assert!(
        std::path::Path::new("/etc/aegis-test-vm").exists(),
        "refusing to run: /etc/aegis-test-vm marker missing (this is not the test VM)"
    );
    let full_v6 = mode == "full";

    let mut failures = 0;
    let mut step = |name: &str, ok: bool, detail: &str| {
        let d = detail.trim().replace('\n', " | ");
        println!(
            "{} [{mode}] {name}: {}",
            if ok { "PASS" } else { "FAIL" },
            &d[..d.len().min(300)]
        );
        if !ok {
            failures += 1;
        }
    };

    // Everything below changes the default route; the marker check above and
    // AEGIS_VM_TEST keep it inside the VM.
    let _ = root(&[&peer_script, "down"]);
    let (ok, out) = root(&["env", "AEGIS_VM_TEST=1", &peer_script, "up", &mode]);
    step("peer up", ok, &out);
    if !ok {
        std::process::exit(1);
    }
    let conf = out.lines().last().unwrap_or_default().trim().to_owned();
    let uplink = run(&["ip", "-4", "route", "show", "default"])
        .1
        .split_whitespace()
        .skip_while(|w| *w != "dev")
        .nth(1)
        .unwrap_or("eth0")
        .to_owned();
    // Let the peer's veth finish IPv6 link-local setup (DAD) so the baseline
    // is not taken mid-change.
    for _ in 0..50 {
        let a = run(&[
            "ip",
            "-6",
            "addr",
            "show",
            "dev",
            "aegis-veth0",
            "scope",
            "link",
        ])
        .1;
        if a.contains("inet6") && !a.contains("tentative") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let (nft_ok, nft_out) = root(&["nft", "list", "ruleset"]);
    step(
        "nftables snapshot available",
        nft_ok,
        nft_out.lines().next().unwrap_or("(empty ruleset)"),
    );
    let baseline = net_state();
    // Positive control: without the tunnel, lookups must be visible on the
    // uplink, otherwise a "0 packets" result later would prove nothing.
    let (_, _, control) = dns_probe(&uplink, &format!("control-{}", std::process::id())).await;
    step(
        &format!("control: DNS visible on uplink {uplink} before connecting"),
        control > 0,
        &format!("{control} packets"),
    );

    let data = std::env::temp_dir().join(format!("aegis-vm-{}", std::process::id()));
    let engine = Engine::start(aegis_linux::NmBackend::connect().await.expect("NM"), &data)
        .await
        .expect("engine");
    let p = engine
        .import_file(std::path::Path::new(&conf))
        .await
        .expect("import");
    let id = p.meta.id;
    step(
        "profile route kind",
        true,
        &format!("{:?}", p.meta.summary.route_kind),
    );

    let wait_confirmed = |secs: u64| {
        let engine = &engine;
        async move {
            let deadline = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < deadline {
                let _ = run(&["ping", "-c", "1", "-W", "1", "198.51.100.1"]);
                if matches!(
                    engine.snapshot().state,
                    ConnectionState::Connected {
                        peer: PeerState::Confirmed,
                        ..
                    }
                ) {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            false
        }
    };

    if !full_v6 {
        // Simulate an install from before IPv6 blocking existed: connecting
        // must refresh it rather than leak.
        let (ok, out) = root(&[
            "nmcli",
            "connection",
            "modify",
            &id.to_string(),
            "ipv6.method",
            "disabled",
            "ipv6.routes",
            "",
        ]);
        step("downgrade to pre-blocking NM settings", ok, &out);
    }
    let r = engine.connect(id).await;
    step("connect", r.is_ok(), &format!("{r:?}"));
    step(
        "server response confirmed",
        wait_confirmed(15).await,
        &format!("{:?}", engine.snapshot().state),
    );

    // Routing.
    let r4 = run(&["ip", "-4", "route", "get", "1.1.1.1"]).1;
    step(
        "IPv4 internet route goes through tunnel",
        r4.contains("dev aegis-"),
        &r4,
    );
    let ep = run(&["ip", "-4", "route", "get", "10.200.0.2"]).1;
    step(
        "endpoint route stays outside tunnel",
        ep.contains("dev aegis-veth0"),
        &ep,
    );
    let (ok, out) = run(&["ping", "-c", "2", "-W", "2", "198.51.100.1"]);
    step(
        "ping IPv4 sentinel behind tunnel",
        ok,
        out.lines().rev().nth(1).unwrap_or(""),
    );
    let r6 = run(&["ip", "-6", "route", "get", "2001:db8:ffff::1"]).1;
    if full_v6 {
        step(
            "IPv6 internet route goes through tunnel",
            r6.contains("dev aegis-"),
            &r6,
        );
        let (ok, out) = run(&["ping", "-6", "-c", "2", "-W", "2", "2001:db8:99::1"]);
        step(
            "ping IPv6 sentinel behind tunnel",
            ok,
            out.lines().rev().nth(1).unwrap_or(""),
        );
    } else {
        step(
            "IPv6 on an IPv4-only full tunnel is routed into the tunnel (dropped there)",
            r6.contains("dev aegis-"),
            &r6,
        );
        // Capture on the uplink while pinging: the block must not just fail,
        // it must keep packets off the local network.
        let seen = uplink_sees(
            &uplink,
            "ip6 and dst 2001:db8:ffff::1",
            &["ping", "-6", "-c", "2", "-W", "2", "2001:db8:ffff::1"],
        )
        .await;
        step(
            &format!("no IPv6 packets on uplink {uplink}"),
            seen == Some(false),
            &format!("{seen:?}"),
        );
    }
    // The engine runs the active check on each status poll; give it one.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let prot = engine.snapshot().protection;
    step(
        "DNS protection verified at runtime",
        matches!(prot.dns, Feature::Verified(_)),
        &format!("{:?}", prot.dns),
    );
    step(
        "IPv6 protection verified at runtime",
        matches!(prot.ipv6, Feature::Verified(_)),
        &format!("{:?}", prot.ipv6),
    );
    // Real handshake time: read as root through the helper, not inferred.
    step(
        "handshake time readable (helper + wg present)",
        engine.capabilities().exact_handshake_time.is_supported(),
        &format!("{:?}", engine.capabilities().exact_handshake_time),
    );
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let hs = engine.snapshot().stats.and_then(|s| s.last_handshake_ms);
    step(
        "last handshake time is real and recent (within 60 s)",
        hs.is_some_and(|t| t > 0 && now_ms.abs_diff(t) < 60_000),
        &format!("{hs:?} vs now {now_ms}"),
    );
    let reach = engine.snapshot().reachability;
    step(
        "reachability: tunnel DNS answered",
        matches!(reach, Feature::Verified(_)),
        &format!("{reach:?}"),
    );

    // DNS leak: capture on the uplink while resolving unique names.
    let tag = std::process::id();
    let (q1, q2, leaked_count) = dns_probe(&uplink, &format!("leak-{tag}")).await;
    step(
        "resolvectl answer came from tunnel DNS",
        q1.contains("198.51.100.1"),
        &q1,
    );
    step(
        "libc (getent) answer came from tunnel DNS",
        q2.contains("198.51.100.1"),
        &q2,
    );
    step(
        &format!("no DNS packets on uplink {uplink}"),
        leaked_count == 0,
        &format!("{leaked_count} packets"),
    );

    // Two country-labelled profiles use the same isolated peer. This proves
    // profile switching, not the physical geography of public remote servers.
    let assign = |code: &str| aegis_core::registry::ServerUpdate {
        location: Some(aegis_core::registry::Location {
            country_code: code.into(),
            city: Some("Isolated test fixture".into()),
            region: None,
            time_zone: None,
        }),
        favorite: true,
        enabled: true,
        registration_pending: false,
    };
    let r = engine.update_server(id, assign("SG")).await;
    step(
        "assign first country metadata",
        r.is_ok(),
        "user-assigned SG fixture",
    );
    let second = engine.duplicate(id).await.expect("second test profile");
    let second_id = second.meta.id;
    let r = engine.update_server(second_id, assign("DE")).await;
    step(
        "assign destination country metadata",
        r.is_ok(),
        "user-assigned DE fixture",
    );
    let r = engine.connect(second_id).await;
    step(
        "switch to second country profile",
        r.is_ok(),
        &format!("{r:?}"),
    );
    step(
        "destination peer answered",
        wait_confirmed(15).await,
        &format!("{:?}", engine.snapshot().state),
    );
    let (ok, out) = run(&["ping", "-c", "2", "-W", "2", "198.51.100.1"]);
    step(
        "traffic works after country switch",
        ok,
        out.lines().rev().nth(1).unwrap_or(""),
    );
    step(
        "active destination metadata synchronized",
        engine.snapshot().profile.is_some_and(|p| {
            p.meta.id == second_id
                && p.meta
                    .server
                    .location
                    .is_some_and(|l| l.country_code == "DE")
        }),
        "expected DE test profile",
    );
    let r = engine.connect(id).await;
    step("switch back to first profile", r.is_ok(), &format!("{r:?}"));
    step(
        "original peer answered again",
        wait_confirmed(15).await,
        &format!("{:?}", engine.snapshot().state),
    );
    let r = engine.delete(second_id).await;
    step("delete inactive destination", r.is_ok(), &format!("{r:?}"));

    // Reconnect after the tunnel is taken down behind the app's back.
    let (ok, out) = root(&["nmcli", "connection", "down", "uuid", &id.to_string()]);
    step("external drop (nmcli down)", ok, &out);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_reconnecting = false;
    while Instant::now() < deadline && !saw_reconnecting {
        saw_reconnecting = matches!(
            engine.snapshot().state,
            ConnectionState::Reconnecting { .. }
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    step(
        "engine noticed drop and is reconnecting",
        saw_reconnecting,
        &format!("{:?}", engine.snapshot().state),
    );
    step(
        "reconnected and confirmed",
        wait_confirmed(30).await,
        &format!("{:?}", engine.snapshot().state),
    );

    // Normal disconnect must restore everything exactly (incl. nftables).
    let r = engine.disconnect().await;
    step("disconnect", r.is_ok(), &format!("{r:?}"));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let diff = diff_lines(&baseline, &net_state());
    step(
        "routes, rules, DNS and nftables restored exactly",
        diff.is_empty(),
        &diff.join(" | "),
    );

    // NetworkManager restart while connected, in separate cycles. The NM rule
    // leak (DECISIONS D11) is intermittent, so try several times.
    for cycle in 1..=3 {
        if cycle > 1 {
            let r = engine.disconnect().await;
            step(
                &format!("cycle {cycle}: disconnect"),
                r.is_ok(),
                &format!("{r:?}"),
            );
        }
        let r = engine.connect(id).await;
        step(
            &format!("cycle {cycle}: connect"),
            r.is_ok(),
            &format!("{r:?}"),
        );
        step(
            &format!("cycle {cycle}: confirmed before NM restart"),
            wait_confirmed(15).await,
            &format!("{:?}", engine.snapshot().state),
        );
        let (ok, out) = root(&["systemctl", "restart", "NetworkManager"]);
        step(&format!("cycle {cycle}: restart NetworkManager"), ok, &out);
        tokio::time::sleep(Duration::from_secs(5)).await;
        step(
            &format!("cycle {cycle}: confirmed after NM restart"),
            wait_confirmed(60).await,
            &format!("{:?}", engine.snapshot().state),
        );
        let r4 = run(&["ip", "-4", "route", "get", "1.1.1.1"]).1;
        step(
            &format!("cycle {cycle}: still routed through tunnel"),
            r4.contains("dev aegis-"),
            &r4,
        );
        let (ok, out) = run(&["ping", "-c", "2", "-W", "2", "198.51.100.1"]);
        step(
            &format!("cycle {cycle}: ping sentinel"),
            ok,
            out.lines().rev().nth(1).unwrap_or(""),
        );
    }
    let r = engine.disconnect().await;
    step("disconnect after NM restart", r.is_ok(), &format!("{r:?}"));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let diff = diff_lines(&baseline, &net_state());
    step(
        "nothing left after NM restart cycle",
        diff.is_empty(),
        &diff.join(" | "),
    );
    // Reachability must fail honestly when the server side cannot resolve:
    // stop the tunnel's DNS server, reconnect, expect Failed (never Verified).
    let (ok, out) = root(&["pkill", "-F", "/run/aegis-test/dnsmasq.pid"]);
    step("stop tunnel DNS server", ok, &out);
    let r = engine.connect(id).await;
    step("reconnect without tunnel DNS", r.is_ok(), &format!("{r:?}"));
    wait_confirmed(15).await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let reach = engine.snapshot().reachability;
    step(
        "reachability fails when tunnel DNS does not answer",
        matches!(reach, Feature::Failed(_)),
        &format!("{reach:?}"),
    );
    let r = engine.disconnect().await;
    step(
        "disconnect after reachability check",
        r.is_ok(),
        &format!("{r:?}"),
    );

    // ---------- kill switch (helper installed by run-vm-tests.sh) ----------
    step(
        "kill switch supported (helper, policy, pkexec, nft present)",
        engine.capabilities().kill_switch.is_supported(),
        &format!("{:?}", engine.capabilities().kill_switch),
    );
    let block = engine.backend().kill_switch_active().await;
    step(
        "helper reports no block before apply",
        matches!(block, Ok(false)),
        &format!("{block:?}"),
    );
    let set_ks = |on: bool, lan: bool| {
        let mut s = engine.settings();
        s.kill_switch = on;
        s.kill_switch_allow_lan = lan;
        s.auto_reconnect = false;
        // The monitor holds the operation lock while it checks the tunnel;
        // a settings change in that moment is refused as Busy, so retry.
        for _ in 0..20 {
            match engine.set_settings(s.clone()) {
                Err(aegis_core::CoreError::Busy) => {
                    std::thread::sleep(Duration::from_millis(300));
                }
                r => return r.map(|_| ()),
            }
        }
        Err(aegis_core::CoreError::Busy)
    };
    let ks_table = || root(&["nft", "list", "table", "inet", "aegis_vpn"]).0;
    // Traffic forced out of the uplink: LAN (slirp gateway) and "internet".
    let wan_ping = [
        "ping",
        "-I",
        uplink.as_str(),
        "-c",
        "2",
        "-W",
        "1",
        "1.1.1.1",
    ];
    let lan_seen = || {
        uplink_sees(
            &uplink,
            "icmp and dst 10.0.2.2",
            &["ping", "-c", "2", "-W", "1", "10.0.2.2"],
        )
    };
    let wan_seen = || uplink_sees(&uplink, "icmp and dst 1.1.1.1", &wan_ping);
    let r = set_ks(true, false);
    step(
        "enable kill switch (LAN blocked)",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let r = engine.connect(id).await;
    step("connect with kill switch", r.is_ok(), &format!("{r:?}"));
    step(
        "kill switch: server confirmed through the block",
        wait_confirmed(15).await,
        &format!("{:?}", engine.snapshot().state),
    );
    let ks = engine.snapshot().protection.kill_switch;
    step(
        "kill switch reported Verified",
        matches!(ks, Feature::Verified(_)),
        &format!("{ks:?}"),
    );
    step("nftables table inet aegis_vpn present", ks_table(), "");
    let (ok, out) = run(&["ping", "-c", "2", "-W", "2", "198.51.100.1"]);
    step(
        "kill switch: sentinel reachable through tunnel",
        ok,
        out.lines().rev().nth(1).unwrap_or(""),
    );
    let seen = wan_seen().await;
    step(
        "kill switch: nothing leaves via uplink (forced -I)",
        seen == Some(false),
        &format!("{seen:?}"),
    );
    let seen = lan_seen().await;
    step(
        "kill switch: LAN blocked when not allowed",
        seen == Some(false),
        &format!("{seen:?}"),
    );

    let (ok, out) = root(&["nmcli", "connection", "down", "uuid", &id.to_string()]);
    step("kill switch: external tunnel drop", ok, &out);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline
        && !matches!(engine.snapshot().state, ConnectionState::Failed { .. })
    {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    step(
        "kill switch: engine reports the drop (auto-reconnect off)",
        matches!(engine.snapshot().state, ConnectionState::Failed { .. }),
        &format!("{:?}", engine.snapshot().state),
    );
    step("kill switch: still blocking after the drop", ks_table(), "");
    let recovered = Engine::start(aegis_linux::NmBackend::connect().await.expect("NM"), &data)
        .await
        .expect("recovery engine");
    let recovered_block = recovered.snapshot();
    step(
        "startup recovers installed block from kernel after tunnel drop",
        matches!(
            recovered_block.state,
            ConnectionState::Failed { profile: None, .. }
        ) && matches!(recovered_block.protection.kill_switch, Feature::Verified(_)),
        &format!(
            "{:?}, {:?}",
            recovered_block.state, recovered_block.protection.kill_switch
        ),
    );
    let seen = wan_seen().await;
    step(
        "kill switch: nothing leaves after the drop",
        seen == Some(false),
        &format!("{seen:?}"),
    );

    let r = engine.disconnect().await;
    step("kill switch: disconnect", r.is_ok(), &format!("{r:?}"));
    step("kill switch: table removed on disconnect", !ks_table(), "");
    let block = engine.backend().kill_switch_active().await;
    step(
        "helper reports no block after removal",
        matches!(block, Ok(false)),
        &format!("{block:?}"),
    );
    let seen = wan_seen().await;
    step(
        "control: uplink traffic visible once unblocked",
        seen == Some(true),
        &format!("{seen:?}"),
    );

    let r = engine.connect(id).await;
    step(
        "connect before firewall-loss recovery test",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let (ok, out) = root(&["/usr/libexec/aegis-vpn/aegis-helper", "remove"]);
    step(
        "administrator removes only the Aegis firewall block",
        ok,
        &out,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline
        && !matches!(engine.snapshot().state, ConnectionState::Failed { .. })
    {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let snap = engine.snapshot();
    step(
        "firewall loss revokes Verified and stops the protected tunnel",
        matches!(snap.state, ConnectionState::Failed { .. })
            && matches!(snap.protection.kill_switch, Feature::Failed(_))
            && matches!(engine.backend().status().await, Ok(None)),
        &format!("{:?}, {:?}", snap.state, snap.protection.kill_switch),
    );

    let r = engine.connect(id).await;
    step(
        "connect before policy-change test",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let (ok, out) = root(&[
        "nft",
        "insert",
        "rule",
        "inet",
        "aegis_vpn",
        "output",
        "ip",
        "daddr",
        "192.0.2.254",
        "drop",
    ]);
    step(
        "insert an extra restrictive rule in the owned table",
        ok,
        &out,
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline
        && !matches!(engine.snapshot().state, ConnectionState::Failed { .. })
    {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let snap = engine.snapshot();
    step(
        "changed policy revokes verification and stops the tunnel",
        matches!(snap.state, ConnectionState::Failed { .. })
            && matches!(snap.protection.kill_switch, Feature::Failed(_))
            && matches!(engine.backend().status().await, Ok(None)),
        &format!("{:?}", snap.state),
    );
    let r = engine.disconnect().await;
    step(
        "explicit cleanup of changed policy",
        r.is_ok(),
        &format!("{r:?}"),
    );

    let r = set_ks(true, true);
    step(
        "enable kill switch with local network allowed",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let r = engine.connect(id).await;
    step(
        "connect with kill switch + LAN",
        r.is_ok(),
        &format!("{r:?}"),
    );
    wait_confirmed(15).await;
    let seen = lan_seen().await;
    step(
        "allow LAN: local network reachable",
        seen == Some(true),
        &format!("{seen:?}"),
    );
    let seen = wan_seen().await;
    step(
        "allow LAN: internet still blocked outside tunnel",
        seen == Some(false),
        &format!("{seen:?}"),
    );
    let r = engine.disconnect().await;
    step(
        "kill switch + LAN: disconnect",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let r = set_ks(false, false);
    step("disable kill switch", r.is_ok(), &format!("{r:?}"));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let diff = diff_lines(&baseline, &net_state());
    step(
        "nothing left after kill switch",
        diff.is_empty(),
        &diff.join(" | "),
    );

    // ---------- network switch and a silent server (kill switch on) ----------
    let logged = |needle: &str| engine.events().iter().any(|l| l.message.contains(needle));
    let r = set_ks(true, false);
    step(
        "enable kill switch for switch/silence tests",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let r = engine.connect(id).await;
    step(
        "connect for switch/silence tests",
        r.is_ok(),
        &format!("{r:?}"),
    );
    step(
        "switch/silence tests: server confirmed",
        wait_confirmed(15).await,
        &format!("{:?}", engine.snapshot().state),
    );
    // The tunnel's own start-up makes NetworkManager noise the engine ignores.
    tokio::time::sleep(Duration::from_secs(17)).await;

    // Another uplink takes over as primary connection: a real NM event.
    let (ok, out) = root(&[
        "nmcli",
        "connection",
        "add",
        "type",
        "dummy",
        "ifname",
        "dum0",
        "con-name",
        "dumx",
        "ipv4.method",
        "manual",
        "ipv4.addresses",
        "10.77.0.2/24",
        "ipv4.gateway",
        "10.77.0.1",
        "ipv4.route-metric",
        "50",
        "ipv6.method",
        "disabled",
    ]);
    step("network switch: add a better uplink", ok, &out);
    let (ok, out) = root(&["nmcli", "connection", "up", "dumx"]);
    step("network switch: bring it up", ok, &out);
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline && !logged("The network changed") {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    step(
        "network switch: engine noticed",
        logged("The network changed"),
        "",
    );
    step(
        "network switch: tunnel confirmed again with fresh data",
        wait_confirmed(25).await,
        &format!("{:?}", engine.snapshot().state),
    );
    let (ok, out) = run(&["ping", "-c", "2", "-W", "2", "198.51.100.1"]);
    step(
        "network switch: still routed through tunnel",
        ok,
        out.lines().rev().nth(1).unwrap_or(""),
    );
    let ks = engine.snapshot().protection.kill_switch;
    step(
        "network switch: kill switch still Verified",
        matches!(ks, Feature::Verified(_)),
        &format!("{ks:?}"),
    );
    let seen = wan_seen().await;
    step(
        "network switch: nothing leaves via uplink",
        seen == Some(false),
        &format!("{seen:?}"),
    );
    let (ok, out) = root(&["nmcli", "connection", "delete", "dumx"]);
    step("network switch: remove the extra uplink", ok, &out);
    // Switching back is a second network change: the engine forgets the old
    // proof again and needs fresh data before it says Confirmed.
    tokio::time::sleep(Duration::from_secs(5)).await;
    step(
        "network switch: confirmed again after switching back",
        wait_confirmed(25).await,
        &format!("{:?}", engine.snapshot().state),
    );

    // The server goes quiet while the client keeps sending (server side of the
    // path dies inside the test namespace; nothing on the host is touched).
    let mut traffic = std::process::Command::new("ping")
        .args(["-i", "0.5", "-c", "200", "198.51.100.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("ping");
    let (ok, out) = root(&[
        "ip",
        "netns",
        "exec",
        "aegis-peer",
        "ip",
        "link",
        "set",
        "wg0",
        "down",
    ]);
    step(
        "silent server: stop the server's WireGuard interface",
        ok,
        &out,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    let silent = |e: &Engine<aegis_linux::NmBackend>| {
        matches!(
            e.snapshot().state,
            ConnectionState::Connected {
                peer: PeerState::NoResponse,
                ..
            }
        )
    };
    while Instant::now() < deadline && !silent(&engine) {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    step(
        "silent server: engine reports no response, keeps the tunnel",
        silent(&engine),
        &format!("{:?}", engine.snapshot().state),
    );
    step(
        "silent server: explained in the log",
        logged("stopped answering"),
        "",
    );
    step("silent server: kill switch still blocking", ks_table(), "");
    let seen = wan_seen().await;
    step(
        "silent server: nothing leaves via uplink",
        seen == Some(false),
        &format!("{seen:?}"),
    );
    let (ok, out) = root(&[
        "ip",
        "netns",
        "exec",
        "aegis-peer",
        "ip",
        "link",
        "set",
        "wg0",
        "up",
    ]);
    step(
        "silent server: restart the server's WireGuard interface",
        ok,
        &out,
    );
    step(
        "silent server: confirmed again once it answers",
        wait_confirmed(60).await,
        &format!("{:?}", engine.snapshot().state),
    );
    let _ = traffic.kill();
    let _ = traffic.wait();

    let r = engine.disconnect().await;
    step(
        "switch/silence tests: disconnect",
        r.is_ok(),
        &format!("{r:?}"),
    );
    let r = set_ks(false, false);
    step(
        "switch/silence tests: disable kill switch",
        r.is_ok(),
        &format!("{r:?}"),
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    let diff = diff_lines(&baseline, &net_state());
    step(
        "nothing left after switch/silence tests",
        diff.is_empty(),
        &diff.join(" | "),
    );

    let r = engine.delete(id).await;
    step("delete", r.is_ok(), &format!("{r:?}"));
    let left = engine
        .backend()
        .installed()
        .await
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    step(
        "no leftover NetworkManager connection",
        left == 0,
        &left.to_string(),
    );
    let (ok, out) = root(&[&peer_script, "down"]);
    step("peer down", ok, &out);
    let _ = std::fs::remove_dir_all(&data);

    println!(
        "{} [{mode}]",
        if failures == 0 {
            "ALL PASSED"
        } else {
            "FAILURES"
        }
    );
    std::process::exit(if failures == 0 { 0 } else { 1 });
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod checks {
    use std::process::Command;
    use std::time::Duration;
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn snapshots_detect_duplicate_rules_and_reordering() {
            assert!(!diff_lines("rule a\n", "rule a\nrule a\n").is_empty());
            assert!(!diff_lines("rule a\nrule b\n", "rule b\nrule a\n").is_empty());
            assert!(diff_lines("rule a\n", "rule a\n").is_empty());
        }

        #[test]
        #[should_panic(expected = "capture read failed")]
        fn failed_capture_is_not_a_zero_packet_result() {
            packet_count((false, "tcpdump: cannot read capture".into()));
        }
    }

    fn packet_count(result: (bool, String)) -> usize {
        assert!(result.0, "capture read failed: {}", result.1);
        result
            .1
            .lines()
            .filter(|l| l.contains(".53:") || l.contains(".53 "))
            .count()
    }
    pub(super) fn run(cmd: &[&str]) -> (bool, String) {
        match Command::new(cmd[0]).args(&cmd[1..]).output() {
            Ok(o) => (
                o.status.success(),
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                ),
            ),
            Err(e) => (false, e.to_string()),
        }
    }
    pub(super) fn root(cmd: &[&str]) -> (bool, String) {
        let mut v = vec!["sudo", "-n"];
        v.extend_from_slice(cmd);
        run(&v)
    }
    pub(super) fn net_state() -> String {
        let mut s = String::new();
        for c in [
            &["ip", "-4", "rule"][..],
            &["ip", "-6", "rule"],
            &["ip", "-4", "route", "show", "table", "all"],
            &["ip", "-6", "route", "show", "table", "all"],
            &["resolvectl", "dns"],
            &["resolvectl", "domain"],
            &["sudo", "-n", "nft", "list", "ruleset"],
        ] {
            let (ok, out) = run(c);
            assert!(ok, "snapshot command failed: {}: {out}", c.join(" "));
            s += &format!("$ {}\n{}\n", c.join(" "), out);
        }
        // Drop values that change by themselves between snapshots: RA route
        // lifetimes, nexthop ids and firewall packet counters.
        s.lines()
            .map(|l| {
                let mut out = Vec::new();
                let mut skip = false;
                for w in l.split_whitespace() {
                    if skip {
                        skip = false;
                        continue;
                    }
                    if matches!(w, "expires" | "nhid" | "packets" | "bytes") {
                        skip = true;
                        continue;
                    }
                    out.push(w);
                }
                out.join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub(super) fn diff_lines(before: &str, after: &str) -> Vec<String> {
        if before == after {
            return vec![];
        }
        // Show only the differing middle, so the step output names the leftover.
        let (b, a): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
        let pre = b.iter().zip(&a).take_while(|(x, y)| x == y).count();
        let suf = b[pre..]
            .iter()
            .rev()
            .zip(a[pre..].iter().rev())
            .take_while(|(x, y)| x == y)
            .count();
        let removed = b[pre..b.len() - suf].iter().map(|l| format!("- {l}"));
        let added = a[pre..a.len() - suf].iter().map(|l| format!("+ {l}"));
        let out: Vec<String> = removed.chain(added).collect();
        if out.is_empty() {
            vec!["(whitespace-only difference)".into()]
        } else {
            out
        }
    }

    /// Run `cmd` while capturing `filter` on `iface`. `Some(true)`: a packet
    /// was seen; `Some(false)`: the capture ran its full 6 s and saw nothing;
    /// `None`: the capture itself failed, which must never count as "no leak".
    pub(super) async fn uplink_sees(iface: &str, filter: &str, cmd: &[&str]) -> Option<bool> {
        let cap = Command::new("sudo")
            .args([
                "-n", "timeout", "6", "tcpdump", "-n", "-i", iface, "-c", "1", filter,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = run(cmd);
        match cap.wait_with_output().ok()?.status.code() {
            Some(0) => Some(true),
            Some(124) => Some(false),
            _ => None,
        }
    }

    /// Resolve two unique names (systemd-resolved and libc paths) while
    /// capturing port 53 on `iface`. Returns both answers and the packet count.
    pub(super) async fn dns_probe(iface: &str, label: &str) -> (String, String, usize) {
        let pcap = format!("/tmp/aegis-dns-{label}.pcap");
        let mut cap = Command::new("sudo")
            .args([
                "-n", "tcpdump", "-n", "-i", iface, "-U", "-w", &pcap, "port", "53",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("tcpdump");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            cap.try_wait().expect("capture status").is_none(),
            "capture exited before DNS probe"
        );
        let q1 = run(&[
            "resolvectl",
            "query",
            "-4",
            &format!("{label}-a.aegis.test"),
        ])
        .1;
        let q2 = run(&["getent", "ahostsv4", &format!("{label}-b.aegis.test")]).1;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let pattern = format!("^tcpdump .* -w {pcap} ");
        let (ok, out) = root(&["pkill", "-INT", "-f", &pattern]);
        assert!(ok, "capture stop failed: {out}");
        assert!(
            cap.wait().expect("capture exit").success(),
            "capture failed"
        );
        let count = packet_count(root(&["tcpdump", "-n", "-r", &pcap]));
        let _ = root(&["rm", "-f", &pcap]);
        (q1, q2, count)
    }
}
