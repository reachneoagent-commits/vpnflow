use std::sync::Arc;
use std::time::Duration;

use crate::mock::MockBackend;
use crate::{CoreError, Engine, VpnBackend, types::*};

const CONF: &str = include_str!("../../../tests/fixtures/valid/full-tunnel.conf");
const SPLIT: &str = include_str!("../../../tests/fixtures/valid/split.conf");
const PRIV: &str = "dwdtCnMYpX08FsFyUbJmRd9ML4frwJkqsXf7pR25LCo=";
const PSK: &str = "MFujEGz6ut24WLYNi1ay+2fdQM2dc66lyn8CjB3arsM=";

async fn engine() -> (Arc<Engine<MockBackend>>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (
        Engine::start(MockBackend::instant(), dir.path())
            .await
            .unwrap(),
        dir,
    )
}

async fn wait_for(e: &Engine<MockBackend>, what: &str, pred: impl Fn(&ConnectionState) -> bool) {
    for _ in 0..600 {
        if pred(&e.snapshot().state) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!(
        "timed out waiting for {what}; state = {:?}",
        e.snapshot().state
    );
}

fn is_connected(s: &ConnectionState) -> bool {
    matches!(s, ConnectionState::Connected { .. })
}

#[tokio::test(start_paused = true)]
async fn import_connect_disconnect() {
    let (e, _d) = engine().await;
    let p = e.import_text("Work", CONF).await.unwrap();
    assert_eq!(e.profiles().len(), 1);
    e.connect(p.meta.id).await.unwrap();
    assert!(is_connected(&e.snapshot().state));
    assert!(e.snapshot().simulated);
    // Mock reports a handshake, so the monitor confirms the peer.
    wait_for(&e, "peer confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
    // Simulated protection is never reported as active.
    let prot = e.snapshot().protection;
    assert!(
        matches!(prot.kill_switch, Feature::Unsupported(_)),
        "{prot:?}"
    );
    e.disconnect().await.unwrap();
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
    assert!(e.backend().status().await.unwrap().is_none());
}

#[tokio::test]
async fn invalid_import_installs_nothing() {
    let (e, _d) = engine().await;
    let err = e
        .import_text("bad", "[Interface]\nPostUp = rm -rf /\n")
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Config(_)));
    assert!(e.profiles().is_empty());
    assert!(e.backend().installed().await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_activation_reports_failure_and_rolls_back() {
    let (e, _d) = engine().await;
    let p = e.import_text("Work", CONF).await.unwrap();
    e.backend().fail_next_activate("endpoint unreachable");
    assert!(e.connect(p.meta.id).await.is_err());
    assert!(matches!(e.snapshot().state, ConnectionState::Failed { .. }));
    assert!(e.backend().status().await.unwrap().is_none());
}

#[tokio::test]
async fn kill_switch_request_fails_closed_when_unsupported() {
    let (e, _d) = engine().await;
    let p = e.import_text("Work", CONF).await.unwrap();
    let mut s = e.settings();
    s.kill_switch = true;
    e.set_settings(s).unwrap();
    let err = e.connect(p.meta.id).await.unwrap_err();
    assert!(matches!(err, CoreError::Unsupported { .. }), "{err:?}");
    assert!(err.to_string().contains("Disable the kill switch"));
    assert!(
        e.backend().status().await.unwrap().is_none(),
        "must not connect without requested protection"
    );
}

#[tokio::test]
async fn unsupported_protection_cannot_be_enabled_during_a_session() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    let mut s = e.settings();
    s.kill_switch = true;
    assert!(e.set_settings(s).is_err());
    assert!(!e.settings().kill_switch);
    e.disconnect().await.unwrap();
}

#[tokio::test]
async fn disconnect_failure_is_retryable_and_aborts_profile_switch() {
    let (e, _d) = engine().await;
    let a = e.import_text("A", CONF).await.unwrap().meta.id;
    let b = e.import_text("B", SPLIT).await.unwrap().meta.id;
    e.connect(a).await.unwrap();
    e.backend().fail_deactivate(true);
    assert!(e.disconnect().await.is_err());
    assert!(!e.snapshot().state.is_idle());
    assert!(e.disconnect().await.is_err());
    assert!(e.connect(b).await.is_err());
    assert_eq!(e.backend().status().await.unwrap().unwrap().id, a);
    assert!(e.delete(a).await.is_err());
    e.backend().fail_deactivate(false);
    e.disconnect().await.unwrap();
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
    assert!(e.backend().status().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn status_failure_invalidates_protection_and_recovers_without_activation() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    e.backend().fail_status(true);
    wait_for(&e, "status uncertainty", |s| !is_connected(s)).await;
    assert!(!e.snapshot().state.is_idle());
    assert!(matches!(e.snapshot().protection.dns, Feature::Failed(_)));
    assert!(e.delete(id).await.is_err());
    e.backend().fail_status(false);
    wait_for(&e, "status recovery", is_connected).await;
    e.disconnect().await.unwrap();
}

#[tokio::test]
async fn recovery_surfaces_unmet_protection_without_dropping_live_tunnel() {
    let (e, dir) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    let mut s = e.settings();
    s.kill_switch = true;
    e.set_settings(s).unwrap();
    drop(e);
    let b = MockBackend::instant();
    b.install(id, "Work", &aegis_config::parse(CONF).unwrap())
        .await
        .unwrap();
    b.activate(id).await.unwrap();
    let e = Engine::start(b, dir.path()).await.unwrap();
    assert!(!is_connected(&e.snapshot().state));
    assert!(!e.snapshot().state.is_idle());
    assert!(e.backend().status().await.unwrap().is_some());
    assert!(e.connect(id).await.is_err());
    e.disconnect().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn startup_status_failure_stays_unknown_until_reconciled() {
    let (e, dir) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    drop(e);
    let b = MockBackend::instant();
    b.install(id, "Work", &aegis_config::parse(CONF).unwrap())
        .await
        .unwrap();
    b.activate(id).await.unwrap();
    b.fail_status(true);
    let e = Engine::start(b, dir.path()).await.unwrap();
    assert!(!e.snapshot().state.is_idle());
    assert!(!is_connected(&e.snapshot().state));
    assert!(e.disconnect().await.is_err());
    assert!(e.delete(id).await.is_err());
    assert!(e.connect(id).await.is_err());
    assert!(!e.snapshot().state.is_idle());
    e.backend()
        .fail_next_activate("reconciliation must not activate");
    e.backend().fail_status(false);
    wait_for(&e, "startup reconciliation", is_connected).await;
    e.disconnect().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn recovered_protection_request_blocks_reactivation_after_drop() {
    let (e, dir) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    let mut s = e.settings();
    s.kill_switch = true;
    s.reconnect_max_attempts = 1;
    e.set_settings(s).unwrap();
    drop(e);
    let b = MockBackend::instant();
    b.install(id, "Work", &aegis_config::parse(CONF).unwrap())
        .await
        .unwrap();
    b.activate(id).await.unwrap();
    let e = Engine::start(b, dir.path()).await.unwrap();
    e.backend().drop_tunnel();
    wait_for(&e, "unsupported retry stopped", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;
    assert!(e.backend().status().await.unwrap().is_none());
    assert!(e.settings().kill_switch);
}

#[tokio::test]
async fn failed_activation_and_rollback_remain_retryable() {
    let (e, _dir) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.backend().fail_next_activate("activation failed");
    e.backend().fail_deactivate(true);
    assert!(e.connect(id).await.is_err());
    assert!(!e.snapshot().state.is_idle());
    e.backend().fail_deactivate(false);
    e.disconnect().await.unwrap();
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test(start_paused = true)]
async fn cancel_reconnect_cleans_up_and_stops_retrying() {
    let (e, _dir) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    e.backend().drop_tunnel();
    wait_for(&e, "retry scheduled", |s| {
        matches!(s, ConnectionState::Reconnecting { .. })
    })
    .await;
    e.cancel();
    wait_for(&e, "retry cancelled", |s| {
        *s == ConnectionState::Disconnected
    })
    .await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
    assert!(e.backend().status().await.unwrap().is_none());
}

#[tokio::test]
async fn cancel_during_connect() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockBackend::with_delay(Duration::from_secs(10));
    let e = Engine::start(mock, dir.path()).await.unwrap();
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    let e2 = Arc::clone(&e);
    let task = tokio::spawn(async move { e2.connect(id).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(matches!(
        e.snapshot().state,
        ConnectionState::Connecting { .. }
    ));
    e.cancel();
    assert!(matches!(task.await.unwrap(), Err(CoreError::Cancelled)));
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test(start_paused = true)]
async fn reconnects_after_unexpected_drop() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    e.backend().drop_tunnel();
    wait_for(&e, "reconnecting", |s| {
        matches!(s, ConnectionState::Reconnecting { .. })
    })
    .await;
    wait_for(&e, "reconnected", is_connected).await;
    assert!(
        e.events()
            .iter()
            .any(|l| l.message.contains("went down unexpectedly"))
    );
    e.disconnect().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn recovered_session_with_kill_switch_does_not_reactivate_after_drop() {
    let (e, dir) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    let mut s = e.settings();
    s.kill_switch = true;
    s.reconnect_max_attempts = 2;
    e.set_settings(s).unwrap();
    drop(e);
    let b = MockBackend::instant();
    b.install(id, "Work", &aegis_config::parse(CONF).unwrap())
        .await
        .unwrap();
    b.activate(id).await.unwrap();
    let e = Engine::start(b, dir.path()).await.unwrap();
    assert!(!is_connected(&e.snapshot().state));
    assert!(!e.snapshot().state.is_idle());
    e.backend().drop_tunnel();
    wait_for(&e, "failed without reactivation", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;
    assert!(
        e.backend().status().await.unwrap().is_none(),
        "must not reactivate when requested kill switch is unavailable"
    );
    assert!(
        e.events().iter().any(|l| {
            l.message.contains("kill switch")
                || l.message.contains("Reconnect attempt")
                || l.message.contains("went down unexpectedly")
        }),
        "expected reconnect refusal evidence in events: {:?}",
        e.events()
    );
}

#[tokio::test]
async fn activation_failure_with_failed_cleanup_stays_uncertain_and_retryable() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.backend().fail_next_activate("endpoint unreachable");
    e.backend().fail_deactivate(true);
    assert!(e.connect(id).await.is_err());
    assert!(
        matches!(e.snapshot().state, ConnectionState::Uncertain { .. }),
        "{:?}",
        e.snapshot().state
    );
    assert!(!e.snapshot().state.is_idle());
    e.backend().fail_deactivate(false);
    e.disconnect().await.unwrap();
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test(start_paused = true)]
async fn drop_without_auto_reconnect_fails() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    let mut s = e.settings();
    s.auto_reconnect = false;
    e.set_settings(s).unwrap();
    e.connect(id).await.unwrap();
    e.backend().drop_tunnel();
    wait_for(&e, "failed", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;
}

#[tokio::test]
async fn switching_profiles_keeps_one_tunnel() {
    let (e, _d) = engine().await;
    let a = e.import_text("A", CONF).await.unwrap().meta.id;
    let b = e.import_text("B", SPLIT).await.unwrap().meta.id;
    e.connect(a).await.unwrap();
    e.connect(b).await.unwrap();
    assert_eq!(e.snapshot().state.profile(), Some(b));
    assert_eq!(e.backend().status().await.unwrap().unwrap().id, b);
}

#[tokio::test]
async fn active_profile_cannot_be_deleted_or_edited() {
    let (e, _d) = engine().await;
    let id = e.import_text("A", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    assert!(e.delete(id).await.is_err());
    let d = e.draft(id).await.unwrap();
    assert!(e.update(id, &d).await.is_err());
    e.disconnect().await.unwrap();
    e.delete(id).await.unwrap();
    assert!(e.backend().installed().await.unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_rename_and_unique_names() {
    let (e, _d) = engine().await;
    let id = e.import_text("Home", CONF).await.unwrap().meta.id;
    let again = e.import_text("home", CONF).await.unwrap();
    assert_eq!(again.meta.name, "home (2)");
    let dup = e.duplicate(id).await.unwrap();
    assert_eq!(dup.meta.name, "Home copy");
    e.rename(id, "Office").await.unwrap();
    assert!(e.profiles().iter().any(|p| p.meta.name == "Office"));
    // Duplicates keep the same secrets.
    assert_eq!(
        e.backend().load(dup.meta.id).await.unwrap(),
        e.backend().load(id).await.unwrap()
    );
}

#[tokio::test]
async fn recovery_preserves_untracked_credentials_and_adopts_known_tunnel() {
    let dir = tempfile::tempdir().unwrap();
    let e = Engine::start(MockBackend::instant(), dir.path())
        .await
        .unwrap();
    let id = e.import_text("A", CONF).await.unwrap().meta.id;
    drop(e);

    // A fresh UI process finds the tunnel still up plus an orphan profile.
    let mock = MockBackend::instant();
    let cfg = aegis_config::parse(CONF).unwrap();
    let orphan = ProfileId::new();
    mock.install(id, "A", &cfg).await.unwrap();
    mock.install(orphan, "orphan", &cfg).await.unwrap();
    mock.activate(id).await.unwrap();
    let e = Engine::start(mock, dir.path()).await.unwrap();
    assert!(is_connected(&e.snapshot().state));
    let installed = e.backend().installed().await.unwrap();
    assert!(installed.contains(&id) && installed.contains(&orphan));
}

#[tokio::test]
async fn ui_facing_data_never_contains_secrets() {
    let (e, _d) = engine().await;
    let id = e.import_text("A", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    let blobs = [
        serde_json::to_string(&e.profiles()).unwrap(),
        serde_json::to_string(&e.snapshot()).unwrap(),
        serde_json::to_string(&e.draft(id).await.unwrap()).unwrap(),
        serde_json::to_string(&e.events()).unwrap(),
        crate::diagnostics::bundle(&e, "test").await,
    ];
    for b in blobs {
        assert!(!b.contains(PRIV) && !b.contains(PSK), "secret leaked: {b}");
    }
}

#[tokio::test]
async fn export_is_private_and_complete() {
    let (e, d) = engine().await;
    let id = e.import_text("A", CONF).await.unwrap().meta.id;
    let out = d.path().join("export.conf");
    e.export(id, &out).await.unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains(PRIV));
    assert_eq!(
        aegis_config::parse(&text).unwrap(),
        aegis_config::parse(CONF).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn import_file_rejects_non_files_and_large_files() {
    let (e, d) = engine().await;
    assert!(e.import_file(d.path()).await.is_err());
    let big = d.path().join("big.conf");
    std::fs::write(&big, vec![b'#'; aegis_config::MAX_CONFIG_BYTES + 10]).unwrap();
    assert!(e.import_file(&big).await.is_err());
    let ok = d.path().join("Office VPN.conf");
    std::fs::write(&ok, CONF).unwrap();
    assert_eq!(e.import_file(&ok).await.unwrap().meta.name, "Office VPN");
    #[cfg(unix)]
    {
        let link = d.path().join("link.conf");
        std::os::unix::fs::symlink(&ok, &link).unwrap();
        assert!(e.import_file(&link).await.is_err());
        let fifo = d.path().join("fifo.conf");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(e.import_file(&fifo).await.is_err());
    }
}

fn server_update(code: &str) -> crate::registry::ServerUpdate {
    crate::registry::ServerUpdate {
        location: Some(crate::registry::Location {
            country_code: code.into(),
            city: Some("Test fixture".into()),
            region: None,
            time_zone: None,
        }),
        favorite: true,
        enabled: true,
        registration_pending: false,
    }
}

#[tokio::test]
async fn server_metadata_migrates_and_persists_without_credentials() {
    let (e, dir) = engine().await;
    let a = e.import_text("Same name", CONF).await.unwrap().meta.id;
    let b = e.import_text("Same name", CONF).await.unwrap().meta.id;
    assert_ne!(e.profiles()[0].meta.name, e.profiles()[1].meta.name);
    // Duplicate endpoints are allowed: two client identities may use one server.
    assert_eq!(
        e.profiles()[0].meta.summary.endpoints,
        e.profiles()[1].meta.summary.endpoints
    );
    e.update_server(a, server_update("sg")).await.unwrap();
    assert!(e.update_server(b, server_update("ZZ")).await.is_err());
    assert!(
        e.update_server(ProfileId::new(), server_update("SG"))
            .await
            .is_err()
    );
    let saved = crate::store::Store::open(dir.path())
        .unwrap()
        .profiles()
        .unwrap();
    assert_eq!(
        saved[0].server.location.as_ref().unwrap().country_code,
        "SG"
    );
    assert!(saved[0].server.favorite);
    let mut settings = e.settings();
    settings.favorite_countries = vec!["SG".into()];
    settings.quick_connect_country = Some("SG".into());
    settings.preferred_profile = Some(a);
    e.set_settings(settings).unwrap();
    let settings = crate::store::Store::open(dir.path())
        .unwrap()
        .settings()
        .unwrap();
    assert_eq!(settings.favorite_countries, ["SG"]);
    assert_eq!(settings.quick_connect_country.as_deref(), Some("SG"));
    assert_eq!(settings.preferred_profile, Some(a));
    let json = std::fs::read_to_string(dir.path().join("profiles.json")).unwrap();
    assert!(!json.contains(PRIV) && !json.contains(PSK));
    let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
    for p in legacy.as_array_mut().unwrap() {
        p.as_object_mut().unwrap().remove("server");
    }
    std::fs::write(
        dir.path().join("profiles.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let migrated = crate::store::Store::open(dir.path())
        .unwrap()
        .profiles()
        .unwrap();
    assert_eq!(migrated.len(), 2);
    assert!(
        migrated
            .iter()
            .all(|p| p.server.location.is_none() && p.server.enabled)
    );
    e.delete(a).await.unwrap();
    assert_eq!(e.profiles().len(), 1);
}

#[tokio::test]
async fn switch_preflight_keeps_working_tunnel_when_destination_is_invalid() {
    let (e, _dir) = engine().await;
    let a = e
        .import_text("Singapore simulated", CONF)
        .await
        .unwrap()
        .meta
        .id;
    let b = e
        .import_text("Germany simulated", SPLIT)
        .await
        .unwrap()
        .meta
        .id;
    e.update_server(a, server_update("SG")).await.unwrap();
    e.update_server(b, server_update("DE")).await.unwrap();
    e.connect(a).await.unwrap();
    e.backend().uninstall(b).await.unwrap();
    assert!(e.connect(b).await.is_err());
    assert_eq!(e.backend().status().await.unwrap().unwrap().id, a);
    assert!(e.snapshot().profile.unwrap().active);
    let mut disabled = server_update("SG");
    disabled.enabled = false;
    assert!(e.update_server(a, disabled).await.is_err());
    e.disconnect().await.unwrap();
}

#[tokio::test]
async fn country_switch_uses_same_engine_and_failure_stays_explicit() {
    let (e, _dir) = engine().await;
    let a = e
        .import_text("Singapore simulated", CONF)
        .await
        .unwrap()
        .meta
        .id;
    let b = e
        .import_text("Germany simulated", SPLIT)
        .await
        .unwrap()
        .meta
        .id;
    e.update_server(a, server_update("SG")).await.unwrap();
    e.update_server(b, server_update("DE")).await.unwrap();
    e.connect(a).await.unwrap();
    e.connect(b).await.unwrap();
    assert_eq!(
        e.snapshot()
            .profile
            .unwrap()
            .meta
            .server
            .location
            .unwrap()
            .country_code,
        "DE"
    );
    assert_eq!(e.backend().status().await.unwrap().unwrap().id, b);
    e.backend()
        .fail_next_activate("simulated unreachable destination");
    assert!(e.connect(a).await.is_err());
    assert!(
        matches!(e.snapshot().state, ConnectionState::Failed { profile: Some(id), .. } if id == a)
    );
    assert!(e.backend().status().await.unwrap().is_none());
    let profiles = e.profiles();
    assert!(
        profiles
            .iter()
            .find(|p| p.meta.id == a)
            .unwrap()
            .meta
            .server
            .last_connected_ms
            .is_none()
    );
    let choice = crate::registry::quick_connect(
        &profiles.iter().map(|p| p.meta.clone()).collect::<Vec<_>>(),
        &e.settings(),
        now_ms(),
    )
    .unwrap();
    assert_eq!(choice.id, b, "failed destination must be in cooldown");
}

#[tokio::test(start_paused = true)]
async fn successful_history_requires_peer_evidence_and_survives_reopening() {
    let (e, dir) = engine().await;
    let id = e.import_text("Simulated", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    assert!(e.profiles()[0].meta.server.last_connected_ms.is_none());
    wait_for(&e, "confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
    assert!(e.profiles()[0].meta.server.last_connected_ms.is_some());
    assert!(e.server_health()[0].measured_rtt_ms.is_none());
    e.disconnect().await.unwrap();
    let restored = Engine::start(MockBackend::instant(), dir.path())
        .await
        .unwrap();
    assert!(
        restored.profiles()[0]
            .meta
            .server
            .last_connected_ms
            .is_some()
    );
    assert!(!restored.profiles()[0].active);
}

#[tokio::test(start_paused = true)]
async fn competing_connection_requests_return_busy_and_can_be_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let e = Engine::start(MockBackend::with_delay(Duration::from_secs(10)), dir.path())
        .await
        .unwrap();
    let a = e.import_text("A", CONF).await.unwrap().meta.id;
    let b = e.import_text("B", SPLIT).await.unwrap().meta.id;
    let connecting = {
        let e = e.clone();
        tokio::spawn(async move { e.connect(a).await })
    };
    tokio::task::yield_now().await;
    assert!(matches!(e.connect(b).await, Err(CoreError::Busy)));
    assert!(matches!(e.quick_connect().await, Err(CoreError::Busy)));
    e.cancel();
    assert!(matches!(
        connecting.await.unwrap(),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test]
async fn generated_identity_is_pending_until_registered_and_disabled_servers_stay_disabled() {
    let (e, _dir) = engine().await;
    let cfg = aegis_config::parse(CONF).unwrap();
    let mut draft = crate::draft::ProfileDraft::from_config("Manual simulated", &cfg);
    draft.private_key = crate::draft::KeyAction::Generate;
    let p = e.create(&draft).await.unwrap();
    assert!(p.meta.server.registration_pending);
    assert!(e.connect(p.meta.id).await.is_err());
    assert!(e.quick_connect().await.is_err());
    let mut update = server_update("TH");
    update.enabled = false;
    e.update_server(p.meta.id, update).await.unwrap();
    assert!(e.connect(p.meta.id).await.is_err());
    e.update_server(p.meta.id, server_update("TH"))
        .await
        .unwrap();
    e.connect(p.meta.id).await.unwrap();
    e.disconnect().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn cancel_during_destination_preflight_preserves_current_tunnel() {
    let dir = tempfile::tempdir().unwrap();
    let mut backend = MockBackend::instant();
    backend.load_delay = Duration::from_secs(10);
    let e = Engine::start(backend, dir.path()).await.unwrap();
    let a = e.import_text("A simulated", CONF).await.unwrap().meta.id;
    let b = e.import_text("B simulated", SPLIT).await.unwrap().meta.id;
    e.connect(a).await.unwrap();
    let switching = {
        let e = e.clone();
        tokio::spawn(async move { e.connect(b).await })
    };
    tokio::task::yield_now().await;
    e.cancel();
    assert!(matches!(
        switching.await.unwrap(),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(e.backend().status().await.unwrap().unwrap().id, a);
    e.disconnect().await.unwrap();
}

/// Regression: the reachability probe ran during the WireGuard handshake and
/// reported a healthy server as "no answer within 3 seconds". It must wait for
/// the peer to be confirmed.
#[tokio::test(start_paused = true)]
async fn connectivity_checks_wait_for_peer_confirmation() {
    let (e, _d) = engine().await;
    let mut s = e.settings();
    s.connectivity_check = true;
    e.set_settings(s).unwrap();
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.backend().silent_peer(true);
    e.connect(id).await.unwrap();
    tokio::time::sleep(Duration::from_secs(10)).await; // several monitor polls
    assert_eq!(
        e.backend().reachability_checks(),
        0,
        "probed before the server answered"
    );

    e.backend().silent_peer(false);
    wait_for(&e, "peer confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        e.backend().reachability_checks() >= 1,
        "never probed after confirmation"
    );
    e.disconnect().await.unwrap();
}

// ---------- kill switch ----------

async fn ks_engine(allow_lan: bool) -> (Arc<Engine<MockBackend>>, tempfile::TempDir, ProfileId) {
    let dir = tempfile::tempdir().unwrap();
    let e = Engine::start(MockBackend::with_test_kill_switch(), dir.path())
        .await
        .unwrap();
    let id = e
        .import_text("Work", &CONF.replace("vpn.example.com", "203.0.113.5"))
        .await
        .unwrap()
        .meta
        .id;
    let mut s = e.settings();
    s.kill_switch = true;
    s.kill_switch_allow_lan = allow_lan;
    s.auto_reconnect = false;
    e.set_settings(s).unwrap();
    (e, dir, id)
}

#[tokio::test]
async fn kill_switch_blocks_before_the_tunnel_and_until_disconnect() {
    let (e, _d, id) = ks_engine(true).await;
    e.connect(id).await.unwrap();
    let (owner, eps, lan) = e.backend().blocking().expect("blocking while connected");
    assert_eq!((owner, lan), (id, true));
    assert_eq!(eps, vec!["203.0.113.5:51820".parse().unwrap()]);
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Verified(_)
    ));
    // Settings cannot change underneath a protected session.
    let mut s = e.settings();
    s.kill_switch = false;
    assert!(e.set_settings(s).is_err());

    e.disconnect().await.unwrap();
    assert!(e.backend().blocking().is_none());
    assert_eq!(
        e.backend().ops(),
        ["apply", "activate", "deactivate", "remove"]
    );
    assert_eq!(e.snapshot().protection.kill_switch, Feature::Off);
}

#[tokio::test(start_paused = true)]
async fn kill_switch_keeps_blocking_after_a_drop_until_the_user_disconnects() {
    let (e, _d, id) = ks_engine(false).await;
    e.connect(id).await.unwrap();
    e.backend().drop_tunnel();
    wait_for(&e, "failed after drop", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;
    assert!(
        e.backend().blocking().is_some(),
        "a dropped tunnel must not unblock"
    );
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Verified(_)
    ));
    e.disconnect().await.unwrap();
    assert!(e.backend().blocking().is_none());
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test]
async fn kill_switch_failed_activation_keeps_blocking_and_cancel_unblocks() {
    let (e, _d, id) = ks_engine(false).await;
    e.backend().fail_next_activate("endpoint unreachable");
    assert!(e.connect(id).await.is_err());
    assert!(matches!(e.snapshot().state, ConnectionState::Failed { .. }));
    assert!(
        e.backend().blocking().is_some(),
        "session started at Connect"
    );
    e.disconnect().await.unwrap();
    assert!(e.backend().blocking().is_none());
}

#[tokio::test(start_paused = true)]
async fn cancelled_reconnect_keeps_blocking_when_cleanup_fails() {
    let (e, _d, id) = ks_engine(false).await;
    let mut settings = e.settings();
    settings.auto_reconnect = true;
    e.set_settings(settings).unwrap();
    e.connect(id).await.unwrap();
    e.backend().drop_tunnel();
    wait_for(&e, "retry scheduled", |s| {
        matches!(s, ConnectionState::Reconnecting { .. })
    })
    .await;

    e.backend().fail_deactivate(true);
    e.cancel();
    wait_for(&e, "cleanup uncertain", |s| {
        matches!(s, ConnectionState::Uncertain { .. })
    })
    .await;
    assert!(
        e.backend().blocking().is_some(),
        "failed tunnel cleanup must not remove the firewall block"
    );
    assert!(!e.backend().ops().contains(&"remove"));

    e.backend().fail_deactivate(false);
    e.disconnect().await.unwrap();
    assert!(e.backend().blocking().is_none());
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test]
async fn kill_switch_refuses_hostnames_and_failed_apply_never_connects() {
    let (e, _d, _) = ks_engine(false).await;
    let named = e.import_text("Named", CONF).await.unwrap().meta.id;
    let err = e.connect(named).await.unwrap_err();
    assert!(err.to_string().contains("IP address"), "{err}");
    assert!(
        err.to_string().contains("turn off the kill switch"),
        "{err}"
    );
    assert!(e.backend().ops().is_empty(), "nothing applied or activated");

    let id = e
        .import_text("Ip", &CONF.replace("vpn.example.com", "198.51.100.7"))
        .await
        .unwrap()
        .meta
        .id;
    e.backend().fail_next_kill_switch();
    assert!(e.connect(id).await.is_err());
    assert_eq!(e.backend().ops(), ["apply"], "no tunnel without the block");
    assert!(e.backend().status().await.unwrap().is_none());
    assert!(matches!(e.snapshot().state, ConnectionState::Failed { .. }));
}

#[tokio::test]
async fn kill_switch_left_by_a_previous_run_is_reported_and_removable() {
    let dir = tempfile::tempdir().unwrap();
    let backend = MockBackend::with_test_kill_switch();
    backend.leave_block(ProfileId::new());
    let e = Engine::start(backend, dir.path()).await.unwrap();
    assert!(matches!(
        e.snapshot().state,
        ConnectionState::Failed { profile: None, .. }
    ));
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Verified(_)
    ));
    e.disconnect().await.unwrap();
    assert!(e.backend().blocking().is_none());
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test(start_paused = true)]
async fn firewall_inspection_failure_stays_unknown_and_cleanup_is_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let backend = MockBackend::with_test_kill_switch();
    backend.leave_block(ProfileId::new());
    backend.fail_kill_switch_status(true);
    backend.fail_remove_kill_switch(true);
    let e = Engine::start(backend, dir.path()).await.unwrap();
    assert!(matches!(
        e.snapshot().state,
        ConnectionState::Uncertain { profile: None, .. }
    ));
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Failed(_)
    ));
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!e.snapshot().state.is_idle());
    assert!(e.disconnect().await.is_err());
    assert!(!e.snapshot().state.is_idle());
    assert!(e.backend().blocking().is_some());
    e.backend().fail_remove_kill_switch(false);
    e.disconnect().await.unwrap();
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
    assert!(e.backend().blocking().is_none());
}

#[tokio::test]
async fn failed_apply_adopts_a_remaining_block_for_explicit_cleanup() {
    let (e, _dir, id) = ks_engine(false).await;
    e.backend().leave_block(id);
    e.backend().fail_next_kill_switch();
    assert!(e.connect(id).await.is_err());
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Verified(_)
    ));
    assert!(e.backend().status().await.unwrap().is_none());
    e.disconnect().await.unwrap();
    assert!(e.backend().blocking().is_none());
}

#[tokio::test]
async fn cleanup_error_after_removing_rules_does_not_claim_a_block() {
    let (e, _dir, id) = ks_engine(false).await;
    e.connect(id).await.unwrap();
    e.backend().fail_after_remove_kill_switch();
    assert!(e.disconnect().await.is_err());
    assert!(e.backend().blocking().is_none());
    assert_eq!(e.snapshot().protection.kill_switch, Feature::Off);
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);
}

#[tokio::test(start_paused = true)]
async fn recovery_with_supported_but_missing_kill_switch_blocks_reactivation() {
    let (e, dir, id) = ks_engine(false).await;
    let mut settings = e.settings();
    settings.auto_reconnect = true;
    settings.reconnect_max_attempts = 1;
    e.set_settings(settings).unwrap();
    drop(e);
    let backend = MockBackend::with_test_kill_switch();
    let cfg = aegis_config::parse(&CONF.replace("vpn.example.com", "203.0.113.5")).unwrap();
    backend.install(id, "Work", &cfg).await.unwrap();
    backend.activate(id).await.unwrap();
    let e = Engine::start(backend, dir.path()).await.unwrap();

    assert!(matches!(
        e.snapshot().state,
        ConnectionState::Uncertain { .. }
    ));
    assert!(e.backend().status().await.unwrap().is_some());
    assert!(e.backend().blocking().is_none());
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!is_connected(&e.snapshot().state));
    e.backend().drop_tunnel();
    wait_for(&e, "unprotected retry refused", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;
    assert!(e.backend().status().await.unwrap().is_none());
    assert_eq!(e.backend().ops(), ["activate", "deactivate"]);
    e.disconnect().await.unwrap();
    e.connect(id).await.unwrap();
    assert!(e.backend().blocking().is_some());
    e.disconnect().await.unwrap();
}

#[tokio::test]
async fn protected_country_switch_is_refused_without_touching_tunnel_or_block() {
    let (e, _dir, first) = ks_engine(false).await;
    let second = e
        .import_text(
            "Destination simulated",
            &CONF.replace("vpn.example.com", "198.51.100.7"),
        )
        .await
        .unwrap()
        .meta
        .id;
    e.update_server(first, server_update("SG")).await.unwrap();
    e.update_server(second, server_update("DE")).await.unwrap();
    e.connect(first).await.unwrap();
    let before = e.backend().ops();
    let err = e.connect(second).await.unwrap_err();
    assert!(err.to_string().contains("protected switching"));
    assert_eq!(
        e.backend().ops(),
        before,
        "must neither remove nor replace the protected session"
    );
    assert_eq!(e.backend().blocking().unwrap().0, first);
    assert_eq!(e.backend().status().await.unwrap().unwrap().id, first);
    e.disconnect().await.unwrap();
}

#[tokio::test]
async fn retrying_failed_protected_server_is_not_an_active_country_switch() {
    let (e, _dir, id) = ks_engine(false).await;
    e.backend().fail_next_activate("simulated startup failure");
    assert!(e.connect(id).await.is_err());
    assert!(matches!(e.snapshot().state, ConnectionState::Failed { .. }));
    assert_eq!(e.backend().blocking().unwrap().0, id);
    e.connect(id).await.unwrap();
    assert_eq!(e.backend().blocking().unwrap().0, id);
    assert!(is_connected(&e.snapshot().state));
    assert!(!e.backend().ops().contains(&"remove"));
    e.disconnect().await.unwrap();
}

#[tokio::test]
async fn preferred_server_change_notifies_profile_consumers() {
    let (e, _dir) = engine().await;
    let id = e
        .import_text("Simulated preference", CONF)
        .await
        .unwrap()
        .meta
        .id;
    let mut events = e.subscribe();
    let mut settings = e.settings();
    settings.preferred_profile = Some(id);
    e.set_settings(settings).unwrap();
    assert!(matches!(
        events.try_recv(),
        Ok(EngineEvent::ProfilesChanged)
    ));
    assert!(e.profiles()[0].preferred);
}

#[tokio::test]
async fn connect_explains_missing_backend_config() {
    let dir = tempfile::tempdir().unwrap();
    let e = Engine::start(MockBackend::instant(), dir.path())
        .await
        .unwrap();
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    // Fresh empty mock + same profiles.json: metadata without a backend config.
    let e = Engine::start(MockBackend::instant(), dir.path())
        .await
        .unwrap();
    let err = e.connect(id).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("simulated backend") && msg.contains("Re-import"),
        "unexpected message: {msg}"
    );
}

#[tokio::test]
async fn mock_store_keeps_configs_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("mock-backend");
    let e = Engine::start(MockBackend::with_store(&store).unwrap(), dir.path())
        .await
        .unwrap();
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    e.disconnect().await.unwrap();
    let e = Engine::start(MockBackend::with_store(&store).unwrap(), dir.path())
        .await
        .unwrap();
    e.connect(id).await.unwrap();
    assert!(is_connected(&e.snapshot().state));
}

// ---------- sleep/resume and network switching ----------

fn peer_of(e: &Engine<MockBackend>) -> Option<PeerState> {
    match e.snapshot().state {
        ConnectionState::Connected { peer, .. } => Some(peer),
        _ => None,
    }
}

fn logged(e: &Engine<MockBackend>, needle: &str) -> bool {
    e.events().iter().any(|l| l.message.contains(needle))
}

#[tokio::test(start_paused = true)]
async fn network_change_discards_old_proof_until_the_server_answers_again() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    wait_for(&e, "confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
    tokio::time::sleep(Duration::from_secs(20)).await; // past the tunnel's own start-up noise

    e.backend().send_event(SystemEvent::NetworkChanged);
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(
        peer_of(&e),
        Some(PeerState::Pending),
        "old proof must not stand after a network switch"
    );
    assert!(logged(&e, "The network changed"));
    assert!(
        e.snapshot().reachability == Feature::Off,
        "stale health results are cleared"
    );

    e.backend().peer_sends(1_000);
    wait_for(&e, "confirmed again", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn network_noise_from_our_own_activation_is_ignored() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    wait_for(&e, "confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
    e.backend().send_event(SystemEvent::NetworkChanged);
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(peer_of(&e), Some(PeerState::Confirmed));
    assert!(!logged(&e, "The network changed"));
}

#[tokio::test(start_paused = true)]
async fn resume_restarts_a_session_that_ran_out_of_retries() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    let mut s = e.settings();
    s.reconnect_max_attempts = 1;
    e.set_settings(s).unwrap();
    e.connect(id).await.unwrap();
    wait_for(&e, "confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;

    // Asleep: the tunnel disappears and the one retry fails with no network.
    e.backend().drop_tunnel();
    e.backend().fail_next_activate("network is unreachable");
    wait_for(&e, "gave up", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;

    e.backend().send_event(SystemEvent::Resumed);
    wait_for(&e, "reconnected after resume", is_connected).await;
    assert!(logged(&e, "woke from sleep"));
}

#[tokio::test(start_paused = true)]
async fn resume_never_reconnects_what_the_user_ended_or_never_had() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;

    // The user disconnected.
    e.connect(id).await.unwrap();
    e.disconnect().await.unwrap();
    e.backend().send_event(SystemEvent::Resumed);
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(e.snapshot().state, ConnectionState::Disconnected);

    // The very first attempt failed: nothing was ever up.
    e.backend().fail_next_activate("endpoint unreachable");
    assert!(e.connect(id).await.is_err());
    e.backend().send_event(SystemEvent::Resumed);
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert!(matches!(e.snapshot().state, ConnectionState::Failed { .. }));
}

#[tokio::test(start_paused = true)]
async fn kill_switch_stays_on_through_sleep_and_resume() {
    let (e, _d, id) = ks_engine(false).await;
    let mut s = e.settings();
    s.auto_reconnect = true; // ks_engine turns it off
    s.reconnect_max_attempts = 1;
    e.set_settings(s).unwrap();
    e.connect(id).await.unwrap();
    e.backend().drop_tunnel();
    e.backend().fail_next_activate("network is unreachable");
    wait_for(&e, "gave up", |s| {
        matches!(s, ConnectionState::Failed { .. })
    })
    .await;
    assert!(
        e.backend().blocking().is_some(),
        "blocked while asleep with no tunnel"
    );

    e.backend().send_event(SystemEvent::Resumed);
    wait_for(&e, "reconnected after resume", is_connected).await;
    assert!(
        e.backend().blocking().is_some(),
        "still blocking after resume"
    );
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Verified(_)
    ));
}

#[tokio::test(start_paused = true)]
async fn a_server_that_stops_answering_is_reported_and_recovers() {
    let (e, _d) = engine().await;
    let id = e.import_text("Work", CONF).await.unwrap().meta.id;
    e.connect(id).await.unwrap();
    // Answered once: confirmed by data received.
    e.backend().peer_sends(1_000);
    wait_for(&e, "confirmed", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;

    // Idle is fine: nothing sent, nothing expected back.
    tokio::time::sleep(Duration::from_secs(90)).await;
    assert_eq!(
        peer_of(&e),
        Some(PeerState::Confirmed),
        "an idle tunnel is not a dead one"
    );

    // Now the client keeps sending and nothing returns.
    e.backend().client_sends(5_000);
    wait_for(&e, "no response", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::NoResponse,
                ..
            }
        )
    })
    .await;
    assert!(logged(&e, "stopped answering"));
    assert!(
        is_connected(&e.snapshot().state),
        "the tunnel itself is kept"
    );

    e.backend().peer_sends(1_000);
    wait_for(&e, "confirmed again", |s| {
        matches!(
            s,
            ConnectionState::Connected {
                peer: PeerState::Confirmed,
                ..
            }
        )
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn externally_removed_firewall_revokes_verified_and_stops_protected_tunnel() {
    let (e, _dir, id) = ks_engine(false).await;
    e.connect(id).await.unwrap();
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Verified(_)
    ));
    e.backend().remove_kill_switch().await.unwrap();
    wait_for(&e, "firewall loss", |state| {
        matches!(state, ConnectionState::Failed { .. })
    })
    .await;
    assert!(e.backend().status().await.unwrap().is_none());
    assert!(matches!(
        e.snapshot().protection.kill_switch,
        Feature::Failed(_)
    ));
}

#[tokio::test]
async fn profile_edit_restores_backend_if_metadata_write_fails() {
    let (e, dir) = engine().await;
    let id = e.import_text("Original", CONF).await.unwrap().meta.id;
    let original = e.backend().load(id).await.unwrap();
    let mut draft = e.draft(id).await.unwrap();
    draft.private_key = crate::draft::KeyAction::Generate;
    draft.name = "Changed".into();
    let path = dir.path().join("profiles.json");
    std::fs::rename(&path, dir.path().join("profiles.backup")).unwrap();
    std::fs::create_dir(path).unwrap();
    assert!(e.update(id, &draft).await.is_err());
    assert_eq!(e.backend().load(id).await.unwrap(), original);
    assert_eq!(e.profiles()[0].meta.name, "Original");
}

#[tokio::test(start_paused = true)]
async fn import_reads_provider_label_into_location() {
    let (e, _d) = engine().await;
    let conf = CONF.replacen("[Peer]", "[Peer]\n# NL-FREE#1", 1);
    let p = e.import_text("file-name", &conf).await.unwrap();
    assert_eq!(p.meta.name, "NL-FREE#1");
    assert_eq!(p.meta.server.location.unwrap().country_code, "NL");
    let plain = e.import_text("Plain", CONF).await.unwrap();
    assert_eq!(plain.meta.name, "Plain");
    assert!(plain.meta.server.location.is_none());
}

async fn two_nl_servers() -> (
    Arc<Engine<MockBackend>>,
    tempfile::TempDir,
    ProfileId,
    ProfileId,
) {
    let dir = tempfile::tempdir().unwrap();
    let e = Engine::start(MockBackend::with_test_kill_switch(), dir.path())
        .await
        .unwrap();
    let mk = |ip: &str, label: &str| {
        CONF.replace("vpn.example.com", ip)
            .replacen("[Peer]", &format!("[Peer]\n# {label}"), 1)
    };
    let a = e
        .import_text("a", &mk("203.0.113.5", "NL-FREE#1"))
        .await
        .unwrap()
        .meta
        .id;
    let b = e
        .import_text("b", &mk("203.0.113.6", "NL-FREE#2"))
        .await
        .unwrap()
        .meta
        .id;
    let mut s = e.settings();
    s.kill_switch = true;
    s.auto_reconnect = false;
    e.set_settings(s).unwrap();
    (e, dir, a, b)
}

#[tokio::test(start_paused = true)]
async fn failed_connect_switches_to_same_country_server() {
    let (e, _d, a, b) = two_nl_servers().await;
    e.backend().fail_next_activate("no handshake");
    e.connect(a).await.unwrap();
    assert!(matches!(
        e.snapshot().state,
        ConnectionState::Connected { profile, .. } if profile == b
    ));
    let (owner, eps, _) = e.backend().blocking().expect("still protected");
    assert_eq!(owner, b);
    assert_eq!(eps, vec!["203.0.113.6:51820".parse().unwrap()]);
    assert!(
        e.events()
            .iter()
            .any(|ev| ev.message.contains("Switched from")),
        "the switch must be visible in Activity"
    );
}

#[tokio::test(start_paused = true)]
async fn switching_off_keeps_the_original_failure() {
    let (e, _d, a, _b) = two_nl_servers().await;
    let mut s = e.settings();
    s.auto_switch_server = false;
    e.set_settings(s).unwrap();
    e.backend().fail_next_activate("no handshake");
    assert!(e.connect(a).await.is_err());
    assert!(!matches!(
        e.snapshot().state,
        ConnectionState::Connected { .. }
    ));
}
