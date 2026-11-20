//! Update check: asks GitHub for the latest public release and compares it with
//! the running version. Sends nothing about the user (a plain anonymous GET) and
//! never installs anything; the UI only offers a link to the release page.

use std::time::Duration;

use serde::Serialize;

const LATEST_URL: &str =
    "https://api.github.com/repos/reachneoagent-commits/vpnflow/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/reachneoagent-commits/vpnflow/releases/latest";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub url: &'static str,
}

/// Parse "v0.1.12" or "0.1.12" into numbers. Anything unusual is rejected.
fn parse(v: &str) -> Option<Vec<u64>> {
    let parts: Vec<u64> = v
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    (!parts.is_empty() && parts.len() <= 4).then_some(parts)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(mut l), Some(mut c)) => {
            let n = l.len().max(c.len());
            l.resize(n, 0);
            c.resize(n, 0);
            l > c
        }
        _ => false,
    }
}

fn fetch_latest_tag() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut resp = agent
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "VPNFlow-update-check")
        .call()
        .map_err(|_| "Could not reach GitHub. Check your connection and try again.".to_string())?;
    let v: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|_| "GitHub answered in a way we could not read.".to_string())?;
    v.get("tag_name")
        .and_then(|t| t.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "No release found yet.".to_string())
}

pub async fn check() -> Result<UpdateInfo, String> {
    let latest = tauri::async_runtime::spawn_blocking(fetch_latest_tag)
        .await
        .map_err(|e| e.to_string())??;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let newer = is_newer(&latest, &current);
    Ok(UpdateInfo {
        latest: latest.trim_start_matches('v').to_string(),
        current,
        newer,
        url: RELEASES_PAGE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("v0.1.12", "0.1.11"));
        assert!(is_newer("0.2.0", "0.1.99"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.11", "0.1.11"));
        assert!(!is_newer("0.1.10", "0.1.11"));
    }

    #[test]
    fn odd_tags_never_claim_an_update() {
        assert!(!is_newer("nightly", "0.1.11"));
        assert!(!is_newer("0.1.12-beta", "0.1.11"));
        assert!(!is_newer("", "0.1.11"));
    }
}
