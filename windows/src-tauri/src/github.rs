//! GitHub sidebar integration — repo info + star via the local `gh` CLI
//! (pattern borrowed from the opencodex desktop app's sidebar-github-row).
//!
//! Privacy contract: repo info hits the public GitHub API through the shared
//! proxy-aware client; starring uses the user's OWN `gh` login (no token is
//! ever stored by TokenStep). Without `gh` (or logged out), the frontend falls
//! back to opening the repository page in the browser.

use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

pub const REPO_URL: &str = "https://github.com/canyexuanfan/TokenStep-Windows";
const REPO_API: &str = "https://api.github.com/repos/canyexuanfan/TokenStep-Windows";
const STARRED_API: &str = "https://api.github.com/user/starred/canyexuanfan/TokenStep-Windows";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoInfo {
    pub url: String,
    pub stars: i64,
}

/// Fetch the repo's public star count (public API, proxy-aware).
pub fn repo_info() -> RepoInfo {
    let client = match crate::net::blocking_client()
        .user_agent("TokenStep")
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return RepoInfo { url: REPO_URL.to_string(), stars: 0 },
    };
    let Ok(resp) = client.get(REPO_API).header("Accept", "application/vnd.github+json").send() else {
        return RepoInfo { url: REPO_URL.to_string(), stars: 0 };
    };
    if !resp.status().is_success() {
        return RepoInfo { url: REPO_URL.to_string(), stars: 0 };
    }
    let stars = resp
        .json::<serde_json::Value>()
        .ok()
        .and_then(|v| v.get("stargazers_count").and_then(|s| s.as_i64()))
        .unwrap_or(0);
    RepoInfo { url: REPO_URL.to_string(), stars }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarState {
    Starred,
    NotStarred,
    Unauthenticated,
}

/// Has the locally-logged-in `gh` user starred this repo?
/// 204 = starred, 404 = not starred, anything else (401 / no gh / network)
/// collapses to Unauthenticated — the UI then opens the repo page instead.
pub fn star_state() -> StarState {
    match gh_api(&["api", STARRED_API, "--include"]) {
        GhOutcome::Status(204) => StarState::Starred,
        GhOutcome::Status(404) => StarState::NotStarred,
        _ => StarState::Unauthenticated,
    }
}

/// Star the repo with the local `gh` login. Returns the resulting state.
pub fn star() -> (bool, StarState) {
    match gh_api(&["api", "-X", "PUT", STARRED_API]) {
        GhOutcome::Status(204) => (true, StarState::Starred),
        GhOutcome::Status(404) => (false, StarState::NotStarred),
        _ => (false, StarState::Unauthenticated),
    }
}

enum GhOutcome {
    Status(u16),
    Unauthorized,
    Failed,
}

/// Run `gh ...` and classify the HTTP status from `--include` output (the gh
/// exit code alone cannot distinguish 204/404/401).
fn gh_api(args: &[&str]) -> GhOutcome {
    let Some(gh) = find_gh() else { return GhOutcome::Failed };
    let mut cmd = if gh.ends_with(".cmd") || gh.ends_with(".bat") {
        let mut c = Command::new("cmd.exe");
        c.arg("/c").arg(&gh);
        c
    } else {
        Command::new(&gh)
    };
    // CREATE_NO_WINDOW: GUI app — never flash a console (codex_quota lesson).
    cmd.creation_flags(0x0800_0000);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return GhOutcome::Failed,
    };
    if !output.status.success() && output.status.code() == Some(1) {
        // gh exit 1 covers both 404 and 401 — parse the status line.
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    for line in text.lines() {
        if line.starts_with("HTTP/") {
            let code: u16 = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            return match code {
                204 => GhOutcome::Status(204),
                404 => GhOutcome::Status(404),
                401 => GhOutcome::Status(401),
                _ => GhOutcome::Failed,
            };
        }
    }
    // No status line: gh missing / network failed / not logged in.
    // A PUT that exits 0 with empty output is a success (204).
    if output.status.success() && text.trim().is_empty() && args.iter().any(|a| *a == "-X") {
        return GhOutcome::Status(204);
    }
    GhOutcome::Failed
}

fn find_gh() -> Option<String> {
    for name in &["gh.cmd", "gh.exe", "gh"] {
        if which::which(name).is_ok() {
            return Some(name.to_string());
        }
    }
    None
}
