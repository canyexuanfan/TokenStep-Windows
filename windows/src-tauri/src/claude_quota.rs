//! Claude Code quota reader — a port of macOS `ClaudeQuotaService.swift`.
//!
//! Reads the OAuth access token from `~/.claude/.credentials.json` (written by
//! the Claude Code CLI on `claude login`), then calls Anthropic's usage API to
//! get the 5h / 7d utilization windows. Results are cached for 10 minutes.
//!
//! If the credentials file is missing (user never signed in to Claude Code),
//! we return a friendly "not signed in" snapshot rather than failing — the UI
//! shows a hint instead of an error.

use crate::codex_quota::CodexQuotaSnapshot;
use crate::paths;
use serde::Deserialize;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_TTL_SECS: u64 = 10 * 60;
const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";

/// Read the Claude usage quota (cached for 10 min, then re-fetched).
pub fn read() -> CodexQuotaSnapshot {
    if let Some(cached) = read_fresh_cache() {
        return cached;
    }

    let token = match read_access_token() {
        Some(t) => t,
        None => {
            return CodexQuotaSnapshot {
                available: false,
                error: Some("未登录 Claude Code".to_string()),
                ..Default::default()
            }
        }
    };

    let response = match fetch_usage(&token) {
        Ok(r) => r,
        Err(msg) => {
            return CodexQuotaSnapshot {
                available: false,
                error: Some(msg),
                ..Default::default()
            }
        }
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let snapshot = CodexQuotaSnapshot {
        available: true,
        five_hour_used_percent: window_percent(response.five_hour.as_ref()),
        five_hour_resets_at: response
            .five_hour
            .as_ref()
            .and_then(|w| w.resets_at.clone())
            .map(|s| normalize_iso(&s)),
        seven_day_used_percent: window_percent(response.seven_day.as_ref()),
        seven_day_resets_at: response
            .seven_day
            .as_ref()
            .and_then(|w| w.resets_at.clone())
            .map(|s| normalize_iso(&s)),
        error: None,
    };

    if snapshot.available {
        write_cache(&snapshot, now_secs);
    }
    snapshot
}

/// Normalize the utilization value to a 0–100 percent. The API returns either
/// a fraction (≤1) or an already-percent value. Mirrors upstream
/// `normalizedPercent`.
fn window_percent(w: Option<&ClaudeUsageWindow>) -> Option<f64> {
    let v = w?.utilization?;
    if !v.is_finite() {
        return None;
    }
    if v <= 0.0 {
        return Some(0.0);
    }
    Some(if v <= 1.0 { (v * 100.0).min(100.0) } else { v.min(100.0) })
}

/// Truncate an ISO-8601 timestamp to "YYYY-MM-DD HH:MM" for display.
fn normalize_iso(s: &str) -> String {
    // Accept both "...Z" and offsets; just take the first 16 chars after any
    // leading date+time. The UI only shows minute precision.
    if s.len() >= 16 {
        let head = &s[..16];
        if head.as_bytes()[10] == b'T' {
            return format!("{} {}", &head[..10], &head[11..16]);
        }
        return head.to_string();
    }
    s.to_string()
}

/// Read the OAuth access token from the Claude Code credentials file.
/// Returns None if the file is missing, unreadable, or has no token — the
/// caller treats that as "user not signed in".
fn read_access_token() -> Option<String> {
    let path = paths::claude_credentials_json();
    let text = fs::read_to_string(&path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    let token = root
        .get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()?;
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Fetch the usage payload. Blocking + rustls (matches the rest of the crate's
/// HTTP style; avoids tokio/OpenSSL).
fn fetch_usage(token: &str) -> Result<ClaudeUsageResponse, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(7))
        .build()
        .map_err(|e| format!("Claude API: {}", e))?;
    let resp = client
        .get(ENDPOINT)
        .header("Authorization", format!("Bearer {}", token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("Claude API: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Claude API {}", status.as_u16()));
    }
    resp.json::<ClaudeUsageResponse>()
        .map_err(|e| format!("Claude API: {}", e))
}

// --- Cache (10 min) ---

#[derive(Debug, Deserialize)]
struct ClaudeUsageResponse {
    #[serde(rename = "five_hour")]
    five_hour: Option<ClaudeUsageWindow>,
    #[serde(rename = "seven_day")]
    seven_day: Option<ClaudeUsageWindow>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsageWindow {
    utilization: Option<f64>,
    #[serde(rename = "resets_at")]
    resets_at: Option<String>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
struct ClaudeQuotaCache {
    fetched_at_secs: u64,
    five_hour_used_percent: Option<f64>,
    five_hour_resets_at: Option<String>,
    seven_day_used_percent: Option<f64>,
    seven_day_resets_at: Option<String>,
}

fn read_fresh_cache() -> Option<CodexQuotaSnapshot> {
    let text = fs::read_to_string(paths::claude_quota_cache_json()).ok()?;
    let cache: ClaudeQuotaCache = serde_json::from_str(&text).ok()?;
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if cache.fetched_at_secs + CACHE_TTL_SECS < now_secs {
        return None;
    }
    let snap = CodexQuotaSnapshot {
        available: cache.five_hour_used_percent.is_some()
            || cache.seven_day_used_percent.is_some(),
        five_hour_used_percent: cache.five_hour_used_percent,
        five_hour_resets_at: cache.five_hour_resets_at,
        seven_day_used_percent: cache.seven_day_used_percent,
        seven_day_resets_at: cache.seven_day_resets_at,
        error: None,
    };
    if snap.available { Some(snap) } else { None }
}

fn write_cache(snap: &CodexQuotaSnapshot, now_secs: u64) {
    let cache = ClaudeQuotaCache {
        fetched_at_secs: now_secs,
        five_hour_used_percent: snap.five_hour_used_percent,
        five_hour_resets_at: snap.five_hour_resets_at.clone(),
        seven_day_used_percent: snap.seven_day_used_percent,
        seven_day_resets_at: snap.seven_day_resets_at.clone(),
    };
    let path = paths::claude_quota_cache_json();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(path, text);
    }
}
