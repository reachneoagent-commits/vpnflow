//! Pause VPN: disconnect for 5 minutes, 30 minutes or 2 hours, then connect
//! again to the same server. Pausing is a real disconnect, so the kill switch
//! is released too (otherwise nothing would load). Any manual connect or
//! disconnect, and quitting the app, cancels the pending resume.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aegis_core::{ConnectionState, Engine, PeerState, ProfileId};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::backend::AnyBackend;

/// Allowed pause lengths, in minutes. The UI offers exactly these.
pub const CHOICES: [u32; 3] = [5, 30, 120];
const POLL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
struct Paused {
    profile: ProfileId,
    until_ms: u64,
    generation: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct PauseInfo {
    pub until_ms: u64,
}

#[derive(Default)]
pub struct Pause {
    inner: Mutex<(Option<Paused>, u64)>, // (current pause, generation counter)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Pause {
    fn lock(&self) -> std::sync::MutexGuard<'_, (Option<Paused>, u64)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn info(&self) -> Option<PauseInfo> {
        self.lock().0.map(|p| PauseInfo {
            until_ms: p.until_ms,
        })
    }

    /// Forget any pause (a manual connect/disconnect took over).
    pub fn clear(&self, app: &AppHandle) {
        let had = {
            let mut g = self.lock();
            g.1 += 1;
            g.0.take().is_some()
        };
        if had {
            announce(app, None);
        }
    }

    fn take_if(&self, generation: u64) -> Option<Paused> {
        let mut g = self.lock();
        match g.0 {
            Some(p) if p.generation == generation => g.0.take(),
            _ => None,
        }
    }
}

fn announce(app: &AppHandle, info: Option<PauseInfo>) {
    let _ = app.emit("pause", info);
}

fn is_connected(state: &ConnectionState) -> bool {
    matches!(
        state,
        ConnectionState::Connected {
            peer: PeerState::Confirmed | PeerState::Pending | PeerState::NoResponse,
            ..
        }
    )
}

/// Disconnect now and reconnect after `minutes`. Only while connected.
pub async fn start(
    pause: &Arc<Pause>,
    app: &AppHandle,
    engine: &Arc<Engine<AnyBackend>>,
    minutes: u32,
) -> Result<PauseInfo, String> {
    if !CHOICES.contains(&minutes) {
        return Err("choose 5 minutes, 30 minutes or 2 hours".into());
    }
    let state = engine.snapshot().state;
    let profile = match (&state, state.profile()) {
        (s, Some(id)) if is_connected(s) => id,
        _ => return Err("connect first; there is nothing to pause".into()),
    };
    engine.disconnect().await.map_err(|e| e.to_string())?;
    let until_ms = now_ms() + u64::from(minutes) * 60_000;
    let generation = {
        let mut g = pause.lock();
        g.1 += 1;
        g.0 = Some(Paused {
            profile,
            until_ms,
            generation: g.1,
        });
        g.1
    };
    let info = PauseInfo { until_ms };
    announce(app, Some(info));
    crate::tray::refresh(app, engine);

    let (pause, app, engine) = (Arc::clone(pause), app.clone(), Arc::clone(engine));
    tauri::async_runtime::spawn(async move {
        // Wall clock, not a single long sleep: a sleeping Mac must still
        // resume on time once it wakes.
        loop {
            tokio::time::sleep(POLL).await;
            let still = matches!(pause.lock().0, Some(p) if p.generation == generation);
            if !still {
                return;
            }
            if now_ms() >= until_ms {
                break;
            }
        }
        resume_taken(&pause, &app, &engine, generation).await;
    });
    Ok(info)
}

async fn resume_taken(
    pause: &Pause,
    app: &AppHandle,
    engine: &Arc<Engine<AnyBackend>>,
    generation: u64,
) {
    if let Some(p) = pause.take_if(generation) {
        announce(app, None);
        if engine.snapshot().state.is_idle() {
            let _ = engine.connect(p.profile).await; // failures surface as state + events
        }
        crate::tray::refresh(app, engine);
    }
}

/// Connect again now, to the server that was paused.
pub async fn resume_now(
    pause: &Arc<Pause>,
    app: &AppHandle,
    engine: &Arc<Engine<AnyBackend>>,
) -> Result<(), String> {
    let generation = match pause.lock().0 {
        Some(p) => p.generation,
        None => return Ok(()),
    };
    resume_taken(pause, app, engine, generation).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_offered_lengths_are_allowed() {
        assert_eq!(CHOICES, [5, 30, 120]);
        assert!(!CHOICES.contains(&0));
        assert!(!CHOICES.contains(&1_000_000));
    }

    #[test]
    fn a_stale_timer_cannot_resume_a_newer_pause() {
        let p = Pause::default();
        {
            let mut g = p.lock();
            g.1 = 2;
            g.0 = Some(Paused {
                profile: ProfileId::new(),
                until_ms: 1,
                generation: 2,
            });
        }
        assert!(p.take_if(1).is_none());
        assert!(p.info().is_some());
        assert!(p.take_if(2).is_some());
        assert!(p.info().is_none());
    }
}
