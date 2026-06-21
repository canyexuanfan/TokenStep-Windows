//! TokenRank leaderboard reader — a port of macOS `TokenRankService.swift`.
//!
//! Fetches the scys.com TokenRank leaderboard (top token users today) and,
//! if the user has set their own user id, locates their entry. Cached for
//! 120 seconds.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_TTL_SECS: u64 = 120;
const ENDPOINT: &str = "https://scys.com/tokenrank/api/subapp/leaderboard";
#[allow(dead_code)]
pub const LEADERBOARD_PAGE_URL: &str = "https://scys.com/tokenrank/";

/// Snapshot returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenRankSnapshot {
    pub available: bool,
    pub board: Option<String>,
    pub range: Option<String>,
    /// Top entry (rank 1), if any.
    pub top: Option<TokenRankEntry>,
    /// The current user's entry, if a user id was set and found.
    pub mine: Option<TokenRankEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenRankEntry {
    pub rank: i64,
    #[serde(rename = "userId")]
    pub user_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub score: i64,
    pub cost: f64,
    #[serde(default)]
    pub by_tool: std::collections::BTreeMap<String, i64>,
}

/// Fetch the leaderboard and locate the user's entry (if `user_id` is set).
/// Caches for 120s keyed by (board, range, user_id).
pub fn read(user_id: &str, board: &str, range: &str) -> TokenRankSnapshot {
    let cleaned = clean_user_id(user_id);
    if let Some(cached) = read_fresh_cache(board, range, &cleaned) {
        return cached;
    }

    let url = format!(
        "{}?board={}&range={}",
        ENDPOINT,
        urlencoding::encode(board),
        urlencoding::encode(range)
    );
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(7))
        .build()
    {
        Ok(c) => c,
        Err(e) => return err_snapshot(&format!("榜单地址不可用: {}", e)),
    };
    let resp = match client.get(&url).header("Accept", "application/json").send() {
        Ok(r) => r,
        Err(_) => return err_snapshot("暂时无法读取榜单"),
    };
    if !resp.status().is_success() {
        return err_snapshot("暂时无法读取榜单");
    }
    let body: Response = match resp.json() {
        Ok(b) => b,
        Err(_) => return err_snapshot("暂时无法读取榜单"),
    };
    // API uses status=0 for success.
    if body.status.unwrap_or(0) != 0 {
        return err_snapshot("暂时无法读取榜单");
    }

    let top = body.entries.first().cloned();
    let mine = if cleaned.is_empty() {
        None
    } else {
        body.entries.iter().find(|e| e.user_id == cleaned).cloned()
    };

    let snap = TokenRankSnapshot {
        available: top.is_some() || mine.is_some(),
        board: Some(body.board.clone()),
        range: Some(body.range.clone()),
        top,
        mine,
        error: None,
    };
    write_cache(&snap, board, range, &cleaned);
    snap
}

/// Build the user's public profile URL (None if user id is empty/invalid).
#[allow(dead_code)]
pub fn user_page_url(user_id: &str) -> Option<String> {
    let cleaned = clean_user_id(user_id);
    if cleaned.is_empty() {
        None
    } else {
        Some(format!("https://scys.com/tokenrank/u/{}", cleaned))
    }
}

/// Mirror upstream `cleanedTokenRankUserID`: trim + keep digits only.
pub fn clean_user_id(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect()
}

fn err_snapshot(msg: &str) -> TokenRankSnapshot {
    TokenRankSnapshot {
        available: false,
        error: Some(msg.to_string()),
        ..Default::default()
    }
}

// --- Cache (120s), keyed by board+range+userId ---

#[derive(Debug, Deserialize)]
struct Response {
    status: Option<i64>,
    board: String,
    range: String,
    entries: Vec<TokenRankEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    fetched_at_secs: u64,
    board: String,
    range: String,
    user_id: String,
    snapshot: TokenRankSnapshot,
}

fn cache_path() -> std::path::PathBuf {
    paths::token_rank_cache_json()
}

fn read_fresh_cache(board: &str, range: &str, user_id: &str) -> Option<TokenRankSnapshot> {
    let text = fs::read_to_string(cache_path()).ok()?;
    let cache: CacheFile = serde_json::from_str(&text).ok()?;
    if cache.board != board || cache.range != range || cache.user_id != user_id {
        return None;
    }
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if cache.fetched_at_secs + CACHE_TTL_SECS < now_secs {
        return None;
    }
    Some(cache.snapshot)
}

fn write_cache(snap: &TokenRankSnapshot, board: &str, range: &str, user_id: &str) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = CacheFile {
        fetched_at_secs: now_secs,
        board: board.to_string(),
        range: range.to_string(),
        user_id: user_id.to_string(),
        snapshot: snap.clone(),
    };
    let path = cache_path();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(path, text);
    }
}

// Minimal percent-encoding for the board/range query params (we control these
// values, but encode defensively in case a future caller passes user input).
mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                    c.to_string()
                } else {
                    format!("%{:02X}", c as u32)
                }
            })
            .collect()
    }
}
