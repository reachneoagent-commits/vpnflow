//! `vpnflow-daemon run` — started by launchd as root.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("vpnflow-daemon only runs on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() {
    use std::path::Path;
    use std::sync::Arc;

    use tokio::signal::unix::{SignalKind, signal};
    use tokio_util::sync::CancellationToken;
    use vpnflow_daemon::handler::Daemon;
    use vpnflow_daemon::server::{Server, console_user_policy};
    use vpnflow_daemon::store::ConfigStore;

    if std::env::args().nth(1).as_deref() != Some("run") {
        eprintln!("usage: vpnflow-daemon run");
        std::process::exit(2);
    }
    tracing_subscriber::fmt().with_target(false).init();

    let store = match ConfigStore::production() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("cannot open the profile store: {e}");
            std::process::exit(1);
        }
    };
    // Undo DNS left behind by a crash of a previous run.
    vpnflow_daemon::tunnel::TunnelManager::restore_dns().await;

    let server = match Server::bind(Path::new(aegis_macos::SOCKET_PATH), console_user_policy) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("cannot bind {}: {e}", aegis_macos::SOCKET_PATH);
            std::process::exit(1);
        }
    };
    let daemon = Arc::new(Daemon::new(store));
    let stop = CancellationToken::new();
    let run = tokio::spawn(server.run(Arc::clone(&daemon), stop.clone()));

    let mut term = signal(SignalKind::terminate()).expect("signal handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    stop.cancel();
    let _ = run.await;
    daemon.tunnel().shutdown().await; // the kill switch deliberately stays
}
