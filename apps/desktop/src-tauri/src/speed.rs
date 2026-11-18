//! On-demand connection speed test, run only when the user presses the button.
//! It talks to Cloudflare's public speed endpoint (the one used by
//! speed.cloudflare.com) and sends no identifiers. Traffic follows the normal
//! routing table, so while connected it goes through the tunnel.

use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde::Serialize;

const PING_TARGET: &str = "1.1.1.1:443";
const DOWN_URL: &str = "https://speed.cloudflare.com/__down?bytes=25000000";
const UP_URL: &str = "https://speed.cloudflare.com/__up";
const DOWN_LIMIT: Duration = Duration::from_secs(10);
const UP_BYTES: usize = 6_000_000;
const PINGS: usize = 6;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpeedResult {
    pub latency_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub download_mbps: Option<f64>,
    pub upload_mbps: Option<f64>,
    pub provider: &'static str,
    /// Per-part problems, so one failed step does not hide the others.
    pub notes: Vec<String>,
}

pub fn mbps(bytes: u64, elapsed: Duration) -> Option<f64> {
    let s = elapsed.as_secs_f64();
    (bytes > 0 && s > 0.0).then(|| bytes as f64 * 8.0 / s / 1_000_000.0)
}

/// Median latency and jitter (mean gap between consecutive samples).
pub fn latency_stats(samples: &[f64]) -> Option<(f64, f64)> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    };
    let jitter = if samples.len() > 1 {
        samples.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (samples.len() - 1) as f64
    } else {
        0.0
    };
    Some((median, jitter))
}

fn latency() -> Option<(f64, f64)> {
    let addr: SocketAddr = PING_TARGET.parse().ok()?;
    let samples: Vec<f64> = (0..PINGS)
        .filter_map(|_| {
            let t = Instant::now();
            TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()?;
            Some(t.elapsed().as_secs_f64() * 1000.0)
        })
        .collect();
    latency_stats(&samples)
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .build()
        .into()
}

fn download() -> Result<f64, String> {
    let resp = agent(20).get(DOWN_URL).call().map_err(|e| e.to_string())?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = [0u8; 64 * 1024];
    let (start, mut total) = (Instant::now(), 0u64);
    while start.elapsed() < DOWN_LIMIT {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => total += n as u64,
            Err(e) => {
                if total == 0 {
                    return Err(e.to_string());
                }
                break;
            }
        }
    }
    mbps(total, start.elapsed()).ok_or_else(|| "no data received".into())
}

fn upload() -> Result<f64, String> {
    let body = vec![0u8; UP_BYTES];
    let start = Instant::now();
    agent(20)
        .post(UP_URL)
        .send(&body[..])
        .map_err(|e| e.to_string())?;
    mbps(UP_BYTES as u64, start.elapsed()).ok_or_else(|| "upload finished instantly".into())
}

/// Blocking; run on a worker thread.
pub fn run() -> SpeedResult {
    let mut notes = Vec::new();
    let (latency_ms, jitter_ms) = match latency() {
        Some((l, j)) => (Some(l), Some(j)),
        None => {
            notes.push("Latency: no reply from the test server.".into());
            (None, None)
        }
    };
    let download_mbps = download()
        .map_err(|e| notes.push(format!("Download: {e}")))
        .ok();
    let upload_mbps = upload()
        .map_err(|e| notes.push(format!("Upload: {e}")))
        .ok();
    SpeedResult {
        latency_ms,
        jitter_ms,
        download_mbps,
        upload_mbps,
        provider: "Cloudflare",
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_math() {
        assert_eq!(mbps(1_000_000, Duration::from_secs(1)), Some(8.0));
        assert_eq!(mbps(0, Duration::from_secs(1)), None);
        assert_eq!(mbps(10, Duration::ZERO), None);
        assert_eq!(latency_stats(&[]), None);
        assert_eq!(latency_stats(&[10.0]), Some((10.0, 0.0)));
        assert_eq!(latency_stats(&[30.0, 10.0, 20.0]), Some((20.0, 15.0)));
        assert_eq!(latency_stats(&[10.0, 20.0]), Some((15.0, 10.0)));
    }
}
