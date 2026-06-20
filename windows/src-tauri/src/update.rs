//! Update check — a lightweight "is there a newer release on GitHub?" probe.
//!
//! This mirrors the spirit of the macOS `UpdateService.checkForUpdates`: hit the
//! GitHub Releases "latest" endpoint, compare `tag_name` to the running version,
//! and surface the result. Unlike the full Tauri updater plugin, it does **not**
//! download or install anything — it only tells the UI "an update exists, here's
//! the link". The user opens the Releases page in a browser to fetch it.
//!
//! Network is best-effort: any failure (offline, rate-limited, bad JSON) just
//! reports "no update", so a flaky connection never blocks the app.

use serde::Serialize;
use std::time::Duration;

/// Where the Windows port lives on GitHub. Used for both the API URL and the
/// Releases page a user is sent to.
pub const REPO_OWNER: &str = "canyexuanfan";
pub const REPO_NAME: &str = "TokenStep-Windows";

/// Result surfaced to the frontend. All fields are sent over the `update-available`
/// event (or returned from the `check_for_update` command).
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    /// True only when a strictly-newer release was found.
    pub has_update: bool,
    /// The running app's version (e.g. "0.1.0"), read from Cargo at compile time.
    pub current_version: String,
    /// The latest release's version tag (e.g. "0.2.0"), with any `v` prefix stripped.
    pub latest_version: String,
    /// URL of the latest release's page (opened in the browser on click).
    pub release_url: String,
    /// Optional release notes (the release `body`), shown in the settings card.
    pub release_notes: Option<String>,
}

impl Default for UpdateCheck {
    fn default() -> Self {
        UpdateCheck {
            has_update: false,
            current_version: current_version().to_string(),
            latest_version: current_version().to_string(),
            release_url: releases_page_url(),
            release_notes: None,
        }
    }
}

/// The running version, sourced from Cargo.toml (compile-time constant).
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The human-readable Releases page (not the API endpoint).
pub fn releases_page_url() -> String {
    format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest")
}

/// The GitHub API endpoint for the latest (non-prerelease, non-draft) release.
fn latest_release_api_url() -> String {
    format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest")
}

/// Shape of the GitHub releases/latest response. We only pull the few fields we
/// need; serde ignores the rest.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
}

use serde::Deserialize;

/// Check GitHub for a newer release. Never panics / never errors out to the
/// caller — on any failure it returns an `UpdateCheck` with `has_update: false`.
pub fn check() -> UpdateCheck {
    let current = current_version();
    let client = match reqwest::blocking::Client::builder()
        .user_agent(format!("TokenStep/{current}"))
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(_) => return UpdateCheck::default(),
    };

    let resp = match client
        .get(latest_release_api_url())
        .header("Accept", "application/vnd.github+json")
        .send()
    {
        Ok(r) => r,
        Err(_) => return UpdateCheck::default(),
    };
    if !resp.status().is_success() {
        return UpdateCheck::default();
    }
    let release: GitHubRelease = match resp.json() {
        Ok(r) => r,
        Err(_) => return UpdateCheck::default(),
    };

    // Skip draft/prerelease releases — the /latest endpoint already excludes
    // these, but defend in depth in case the endpoint changes.
    if release.draft.unwrap_or(false) || release.prerelease.unwrap_or(false) {
        return UpdateCheck::default();
    }

    let latest = strip_v_prefix(&release.tag_name);
    let has_update = is_newer(&latest, current);

    UpdateCheck {
        has_update,
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        release_url: release.html_url,
        release_notes: release.body,
    }
}

/// Strip a leading `v`/`V` so "v0.2.0" and "0.2.0" compare equal.
fn strip_v_prefix(tag: &str) -> String {
    tag.trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .to_string()
}

/// Strict "is `latest` newer than `current`" by numeric segment comparison.
/// Non-numeric segments (e.g. a "beta" suffix) degrade to 0, mirroring the
/// macOS Version struct's behavior. Missing segments on either side are 0.
fn is_newer(latest: &str, current: &str) -> bool {
    cmp_segments(latest, current) == std::cmp::Ordering::Greater
}

fn cmp_segments(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = a.split('.').map(parse_seg).collect::<Vec<_>>();
    let pb = b.split('.').map(parse_seg).collect::<Vec<_>>();
    let len = pa.len().max(pb.len());
    for i in 0..len {
        let va = pa.get(i).copied().unwrap_or(0);
        let vb = pb.get(i).copied().unwrap_or(0);
        match va.cmp(&vb) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

/// Parse one version segment as a leading integer; "1beta" → 1, "x" → 0.
fn parse_seg(s: &str) -> u64 {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        // v-prefix ignored
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.2.0", "V0.1.0"));
        // uneven lengths
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1", "0.1.1"));
        // non-numeric suffix degrades to 0
        assert!(is_newer("0.2.0-beta", "0.1.0"));
    }

    #[test]
    fn windows_tag_suffix_is_compatible() {
        // Our release tags carry a "-windows" suffix (e.g. v0.1.0-windows) to
        // stay distinct from upstream macOS tags. The suffix must NOT make the
        // tag compare as newer than the running build of the same version:
        // v0.1.0-windows vs running 0.1.0 should be "equal" (no update), while
        // v0.2.0-windows vs 0.1.0 should still report an update.
        assert!(!is_newer("0.1.0-windows", "0.1.0"));
        assert!(is_newer("0.2.0-windows", "0.1.0"));
        assert!(is_newer("0.1.1-windows", "0.1.0"));
    }

    #[test]
    fn strip_prefix() {
        assert_eq!(super::strip_v_prefix("v0.2.0"), "0.2.0");
        assert_eq!(super::strip_v_prefix("V0.2.0"), "0.2.0");
        assert_eq!(super::strip_v_prefix("0.2.0"), "0.2.0");
    }
}
