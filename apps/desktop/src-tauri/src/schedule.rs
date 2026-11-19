//! Scheduled connect: connect and disconnect at set times of day on chosen
//! days. It acts only at the scheduled minute, so a manual choice made at any
//! other time is never overridden. A time missed while the computer slept is
//! not replayed.

use std::sync::Arc;
use std::time::Duration;

use aegis_core::{Engine, Settings, parse_hhmm};

use crate::backend::AnyBackend;

const POLL: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Nothing,
    Connect,
    Disconnect,
}

/// What the schedule calls for at `weekday` (1 = Monday) and `minute` of day.
pub fn due(s: &Settings, weekday: u8, minute: u32, idle: bool) -> Action {
    if !s.schedule_enabled || !s.schedule_days.contains(&weekday) {
        return Action::Nothing;
    }
    if parse_hhmm(&s.schedule_connect) == Some(minute) && idle {
        Action::Connect
    } else if parse_hhmm(&s.schedule_disconnect) == Some(minute) && !idle {
        Action::Disconnect
    } else {
        Action::Nothing
    }
}

/// Local weekday (1 = Monday) and minute of day, from `date`.
fn local_now() -> Option<(u8, u32)> {
    let out = std::process::Command::new("date")
        .arg("+%u %H:%M")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (day, time) = text.trim().split_once(' ')?;
    Some((day.parse().ok()?, parse_hhmm(time)?))
}

pub fn spawn(engine: Arc<Engine<AnyBackend>>) {
    tauri::async_runtime::spawn(async move {
        let mut last_fired: Option<(u8, u32)> = None;
        loop {
            tokio::time::sleep(POLL).await;
            let Some(now) = tokio::task::spawn_blocking(local_now).await.unwrap_or(None) else {
                continue;
            };
            if last_fired == Some(now) {
                continue;
            }
            let s = engine.settings();
            match due(&s, now.0, now.1, engine.snapshot().state.is_idle()) {
                Action::Nothing => {}
                Action::Connect => {
                    last_fired = Some(now);
                    let _ = match s.preferred_profile {
                        Some(id) => engine.connect(id).await,
                        None => engine.quick_connect().await.map(|_| ()),
                    };
                }
                Action::Disconnect => {
                    last_fired = Some(now);
                    let _ = engine.disconnect().await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Settings {
        Settings {
            schedule_enabled: true,
            ..Settings::default()
        }
    }

    #[test]
    fn connects_and_disconnects_at_the_set_minute_on_chosen_days() {
        assert_eq!(due(&s(), 1, 9 * 60, true), Action::Connect);
        assert_eq!(due(&s(), 1, 17 * 60, false), Action::Disconnect);
        assert_eq!(due(&s(), 1, 12 * 60, true), Action::Nothing);
    }

    #[test]
    fn skips_other_days_and_states() {
        assert_eq!(due(&s(), 6, 9 * 60, true), Action::Nothing); // Saturday
        assert_eq!(due(&s(), 1, 9 * 60, false), Action::Nothing); // already on
        assert_eq!(due(&s(), 1, 17 * 60, true), Action::Nothing); // already off
        assert_eq!(due(&Settings::default(), 1, 9 * 60, true), Action::Nothing);
    }
}
