use super::*;

#[test]
fn default_routes_need_a_tunnel_address_of_the_same_family() {
    let mut cfg = parse(include_str!(
        "../../../tests/fixtures/valid/full-tunnel.conf"
    ))
    .unwrap();
    cfg.interface.addresses.retain(|a| a.addr().is_ipv4());
    assert!(
        validate(&cfg)
            .iter()
            .any(|i| i.field.as_deref() == Some("Address"))
    );
}
use proptest::prelude::*;

const FULL: &str = include_str!("../../../tests/fixtures/valid/full-tunnel.conf");
const SPLIT: &str = include_str!("../../../tests/fixtures/valid/split.conf");
const POSTUP: &str = include_str!("../../../tests/fixtures/invalid/postup.conf");
const PRIV: &str = "dwdtCnMYpX08FsFyUbJmRd9ML4frwJkqsXf7pR25LCo=";
const PEER: &str = "3p7bfXt9wbTTW2HC7OQ1Nz+DQ8hbeGdNrfx+FG+IK08=";

fn minimal(extra_iface: &str, extra_peer: &str) -> String {
    let endpoint = if extra_peer.contains("Endpoint") {
        ""
    } else {
        "Endpoint = 198.51.100.1:51820"
    };
    format!(
        "[Interface]\nPrivateKey = {PRIV}\nAddress = 10.0.0.2/32\n{extra_iface}\n[Peer]\nPublicKey = {PEER}\n{endpoint}\nAllowedIPs = 10.0.0.0/24\n{extra_peer}\n"
    )
}

fn messages(text: &str) -> Vec<String> {
    parse(text)
        .unwrap_err()
        .issues
        .into_iter()
        .map(|i| i.to_string())
        .collect()
}

#[test]
fn parses_full_tunnel() {
    let c = parse(FULL).unwrap();
    assert_eq!(c.interface.addresses.len(), 2);
    assert_eq!(c.interface.dns.len(), 2);
    assert_eq!(c.interface.mtu, Some(1420));
    let p = &c.peers[0];
    assert!(p.preshared_key.is_some());
    assert_eq!(
        p.endpoint.as_ref().unwrap().to_string(),
        "vpn.example.com:51820"
    );
    assert_eq!(p.persistent_keepalive, Some(25));
    assert_eq!(c.route_kind(), RouteKind::Full);
    assert!(warnings(&c).is_empty());
}

#[test]
fn parses_split_lowercase_ipv6_endpoint_and_search_domain() {
    let c = parse(SPLIT).unwrap();
    assert_eq!(c.route_kind(), RouteKind::Split);
    assert_eq!(c.interface.dns_search, vec!["corp.example".to_string()]);
    assert_eq!(c.peers[0].allowed_ips.len(), 2);
    assert_eq!(
        c.peers[0].endpoint.as_ref().unwrap().to_string(),
        "[2001:db8::10]:51820"
    );
    // Interface address keeps its host bits.
    assert_eq!(c.interface.addresses[0].to_string(), "10.99.0.2/24");
}

#[test]
fn rejects_scripts_and_never_echoes_their_content() {
    let msgs = messages(POSTUP);
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(msgs[0].contains("PostUp"));
    assert!(msgs[0].contains("never executes scripts"));
    assert!(!msgs[0].contains("evil"));
    for k in ["PreUp", "PostDown", "PreDown", "postup"] {
        assert!(parse(&minimal(&format!("{k} = true"), "")).is_err(), "{k}");
    }
}

#[test]
fn rejects_routing_directives_instead_of_dropping_them() {
    for line in [
        "Table = off",
        "Table = 1234",
        "FwMark = 0x51820",
        "SaveConfig = true",
    ] {
        assert!(parse(&minimal(line, "")).is_err(), "{line}");
    }
}

#[test]
fn rejects_unknown_keys_and_sections() {
    assert!(parse(&minimal("Foo = bar", "")).is_err());
    assert!(parse(&minimal("", "Bar = baz")).is_err());
    assert!(parse(&format!("{}\n[Wat]\n", minimal("", ""))).is_err());
    assert!(parse("Address = 10.0.0.1/32\n").is_err());
}

#[test]
fn error_messages_do_not_echo_values() {
    // A wrong file (e.g. /etc/passwd-like content inside a section) must not
    // be reflected back to the UI.
    let text = format!("[Interface]\nPrivateKey = {PRIV}\nAddress = root:x:0:0:secretstuff\n");
    for m in messages(&text) {
        assert!(!m.contains("secretstuff"), "{m}");
        assert!(!m.contains(PRIV), "{m}");
    }
    let bad_key = "[Interface]\nPrivateKey = notakeybutsomethingprivate\n";
    for m in messages(bad_key) {
        assert!(!m.contains("somethingprivate"), "{m}");
    }
}

#[test]
fn validates_semantics() {
    let cases = [
        (minimal("MTU = 100", ""), "MTU"),
        (minimal("", "Endpoint = host:0"), "Endpoint"),
        (minimal("", "Endpoint = 2001:db8::1:51820"), "brackets"),
        (minimal("", "Endpoint = -bad-.example:1"), "Endpoint"),
        (minimal("Address = 10.0.0.300/32", ""), "Address"),
        (minimal("", "AllowedIPs = 10.0.0.0/33"), "AllowedIPs"),
        (minimal("ListenPort = 70000", ""), "ListenPort"),
        (
            minimal("", "PersistentKeepalive = soon"),
            "PersistentKeepalive",
        ),
        (minimal("PrivateKey = x", ""), "more than once"),
        (
            format!(
                "[Interface]\nPrivateKey = {PRIV}\n[Peer]\nPublicKey = {PEER}\nAllowedIPs = 0.0.0.0/0\n"
            ),
            "Endpoint",
        ),
        (
            "[Interface]\nAddress = 10.0.0.1/32\n".to_string(),
            "PrivateKey",
        ),
    ];
    for (text, needle) in cases {
        let msgs = messages(&text);
        assert!(
            msgs.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {msgs:?}"
        );
    }
}

#[test]
fn rejects_own_and_duplicate_peer_keys() {
    let own = "hSDwCYkwp1R0i33ctD73Wg2/Og0mOBr066SpjqqbTmo="; // public key of PRIV
    let text = format!(
        "[Interface]\nPrivateKey = {PRIV}\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey = {own}\nEndpoint = 1.2.3.4:5\nAllowedIPs = 10.1.0.0/24\n[Peer]\nPublicKey = {PEER}\nAllowedIPs = 10.2.0.0/24\n[Peer]\nPublicKey = {PEER}\nAllowedIPs = 10.2.0.0/24\n"
    );
    let msgs = messages(&text);
    assert!(
        msgs.iter().any(|m| m.contains("own public key")),
        "{msgs:?}"
    );
    assert!(msgs.iter().any(|m| m.contains("duplicates")), "{msgs:?}");
    assert!(
        msgs.iter().any(|m| m.contains("repeats network")),
        "{msgs:?}"
    );
}

#[test]
fn route_kinds() {
    let kind = |allowed: &str| {
        parse(&minimal(
            "Address = fd99::2/128",
            &format!("AllowedIPs = {allowed}"),
        ))
        .unwrap()
        .route_kind()
    };
    assert_eq!(kind("0.0.0.0/0"), RouteKind::FullIpv4Only);
    assert_eq!(kind("::/0"), RouteKind::FullIpv6Only);
    assert_eq!(
        kind("0.0.0.0/1, 128.0.0.0/1, ::/1, 8000::/1"),
        RouteKind::Full
    );
    assert_eq!(kind("0.0.0.0/1"), RouteKind::Split);
}

#[test]
fn ipv4_only_full_tunnel_warns_about_ipv6() {
    let c = parse(&minimal("DNS = 1.1.1.1", "AllowedIPs = 0.0.0.0/0")).unwrap();
    let w = warnings(&c);
    assert!(w.iter().any(|i| i.message.contains("IPv6")), "{w:?}");
}

#[test]
fn rejects_oversized_input() {
    let big = "#".repeat(MAX_CONFIG_BYTES + 1);
    assert!(messages(&big)[0].contains("larger"));
}

#[test]
fn handles_bom_crlf_and_comments() {
    let text = format!("\u{feff}{}", minimal("", "").replace('\n', "\r\n"));
    parse(&text).unwrap();
}

#[test]
fn roundtrips_fixtures() {
    for f in [FULL, SPLIT] {
        let c = parse(f).unwrap();
        let again = parse(&c.to_wg_quick()).unwrap();
        assert_eq!(c, again);
    }
}

#[test]
fn debug_output_contains_no_secrets() {
    let c = parse(FULL).unwrap();
    let dbg = format!("{c:?}");
    assert!(!dbg.contains(PRIV));
    assert!(!dbg.contains("MFujEGz6ut24WLYNi1ay"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn never_panics_on_arbitrary_text(s in "\\PC{0,400}") {
        let _ = parse(&s);
    }

    #[test]
    fn never_panics_on_config_shaped_text(
        lines in prop::collection::vec(
            prop_oneof![
                Just("[Interface]".to_string()),
                Just("[Peer]".to_string()),
                "(PrivateKey|PublicKey|Address|DNS|AllowedIPs|Endpoint|MTU|PersistentKeepalive) = [ -~]{0,60}",
            ],
            0..20,
        )
    ) {
        let _ = parse(&lines.join("\n"));
    }

    #[test]
    fn generated_configs_roundtrip(
        a in 1u8..=254, b in 1u8..=254, prefix in 8u8..=32, port in 1u16..,
        keepalive in prop::option::of(1u16..),
        mtu in prop::option::of(1280u16..=9000),
    ) {
        let text = format!(
            "[Interface]\nPrivateKey = {PRIV}\nAddress = 10.{a}.{b}.2/32\n{}\n[Peer]\nPublicKey = {PEER}\nEndpoint = 192.0.2.{a}:{port}\nAllowedIPs = 10.{a}.0.0/{prefix}\n{}\n",
            mtu.map(|m| format!("MTU = {m}")).unwrap_or_default(),
            keepalive.map(|k| format!("PersistentKeepalive = {k}")).unwrap_or_default(),
        );
        let c = parse(&text).unwrap();
        prop_assert_eq!(&c, &parse(&c.to_wg_quick()).unwrap());
    }
}

#[test]
fn import_turns_ipv4_only_address_with_ipv6_route_into_ipv4_only() {
    let text = format!(
        "[Interface]\nPrivateKey = {PRIV}\nAddress = 10.2.0.2/32\nDNS = 10.2.0.1\n[Peer]\nPublicKey = {PEER}\nEndpoint = 198.51.100.1:51820\nAllowedIPs = 0.0.0.0/0, ::/0\n"
    );
    assert!(parse(&text).is_err());
    let (cfg, fixed) = crate::parse_import(&text).unwrap();
    assert!(fixed);
    assert_eq!(cfg.route_kind(), RouteKind::FullIpv4Only);
    // A file that is already consistent is untouched.
    let ok = text.replace(
        "Address = 10.2.0.2/32",
        "Address = 10.2.0.2/32, fd00::2/128",
    );
    let (cfg, fixed) = crate::parse_import(&ok).unwrap();
    assert!(!fixed);
    assert_eq!(cfg.route_kind(), RouteKind::Full);
}
