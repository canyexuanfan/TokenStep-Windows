//! Agent-work-rank leaderboard — a port of upstream macOS
//! `AgentWorkRankService.swift` (v0.2.0; the former scys.com TokenRank was
//! retired in v0.1.46 and replaced by the zhenganhuo.com Agent usage rank).
//!
//! Fetches the public leaderboard (`client=all&range=today&usage_mode=all`),
//! and locates the local user's row via the identity the Token Rank CLI
//! persists at `~/.token-rank/client-state.json`. Rank is fully
//! server-assigned; the client never computes placements. Cache TTL 30 min.
//!
//! Privacy contract (docs/PRIVACY.md): the card defaults to HIDDEN — hidden
//! mode performs zero local-identity reads and zero network requests.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_TTL_SECS: u64 = 30 * 60;
const ENDPOINT: &str = "https://www.zhenganhuo.com/api/token-rank/leaderboard.php";
#[allow(dead_code)]
pub const LEADERBOARD_PAGE_URL: &str = "https://www.zhenganhuo.com/token-rank";
#[allow(dead_code)]
pub const MY_PAGE_URL: &str = "https://www.zhenganhuo.com/token-rank/me";

/// Visibility tri-state (upstream `AgentWorkRankVisibility`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankVisibility {
    Automatic,
    Visible,
    Hidden,
}

impl RankVisibility {
    pub fn from_str(v: &str) -> RankVisibility {
        match v {
            "automatic" => RankVisibility::Automatic,
            "visible" => RankVisibility::Visible,
            _ => RankVisibility::Hidden,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RankVisibility::Automatic => "automatic",
            RankVisibility::Visible => "visible",
            RankVisibility::Hidden => "hidden",
        }
    }

    /// Hidden performs no identity reads at all.
    pub fn reads_local_identity(&self) -> bool {
        *self != RankVisibility::Hidden
    }

    pub fn should_show(&self, has_local_identity: bool) -> bool {
        match self {
            RankVisibility::Automatic => has_local_identity,
            RankVisibility::Visible => true,
            RankVisibility::Hidden => false,
        }
    }
}

/// Local identity from the Token Rank CLI (`~/.token-rank/client-state.json`).
/// `None` when the file is absent or holds no positive user id — "not
/// linked" (尚未关联).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalIdentity {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

pub fn load_local_identity() -> Option<LocalIdentity> {
    let text = fs::read_to_string(paths::token_rank_client_state_json()).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let user = doc.get("user")?;
    let id = user.get("id")?.as_i64()?;
    if id <= 0 {
        return None;
    }
    Some(LocalIdentity {
        id,
        name: user
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("匿名用户")
            .to_string(),
        avatar_url: user.get("avatar_url").and_then(|v| v.as_str()).map(String::from),
    })
}

/// Snapshot returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenRankSnapshot {
    pub available: bool,
    pub identity: Option<LocalIdentity>,
    /// Top entry (rank 1), if any.
    pub top: Option<TokenRankEntry>,
    /// The current user's entry (server-assigned rank).
    pub mine: Option<TokenRankEntry>,
    /// Whole-board tokens today (data.total_tokens).
    pub total_tokens: i64,
    /// Number of ranked users.
    pub total_ranked_users: i64,
    /// When this data was fetched (epoch secs) — drives the "N 人 · X 分钟前"
    /// header (cache hits re-stamp from the cache file).
    #[serde(default)]
    pub fetched_at_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenRankEntry {
    pub rank: i64,
    #[serde(default)]
    pub user_id: i64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub call_count: i64,
    #[serde(default)]
    pub session_count: i64,
    #[serde(default)]
    pub clients: std::collections::BTreeMap<String, i64>,
    /// model id -> tokens (the API ships a map, not a list).
    #[serde(default)]
    pub models: std::collections::BTreeMap<String, i64>,
}

impl TokenRankEntry {
    /// Primary client = the max-tokens key of `clients` (upstream maps the
    /// raw client ids to display names at the UI layer).
    #[allow(dead_code)]
    pub fn primary_client(&self) -> Option<(&String, &i64)> {
        self.clients.iter().max_by_key(|(_, v)| *v)
    }
}

/// Fetch the leaderboard and locate the user's row via `identity`.
/// Cached for 30 minutes. When `identity` is None the board is still fetched
/// (the card may show the top entry) but `mine` stays None.
pub fn read(identity: Option<&LocalIdentity>) -> TokenRankSnapshot {
    if let Some(mut cached) = read_fresh_cache(identity.map(|i| i.id)) {
        cached.fetched_at_secs = cached_fetch_stamp();
        return cached;
    }
    let url = format!("{}?client=all&range=today&usage_mode=all", ENDPOINT);
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(_) => return err_snapshot("榜单地址不可用"),
    };
    let resp = match client
        .get(&url)
        .header("Accept", "application/json")
        .header("Cache-Control", "no-cache")
        .send()
    {
        Ok(r) => r,
        Err(_) => return err_snapshot("暂时无法读取榜单"),
    };
    if !resp.status().is_success() {
        return err_snapshot("暂时无法读取榜单");
    }
    let body: serde_json::Value = match resp.json() {
        Ok(v) => v,
        Err(_) => return err_snapshot("暂时无法读取榜单"),
    };
    // API uses success=true; rows live under data.rows.
    if body.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return err_snapshot("暂时无法读取榜单");
    }
    let Some(data) = body.get("data") else {
        return err_snapshot("暂时无法读取榜单");
    };
    let total_tokens = data.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let total_ranked = data.get("total_ranked_users").and_then(|v| v.as_i64()).unwrap_or(0);
    let mut entries: Vec<TokenRankEntry> = vec![];
    if let Some(rows) = data.get("rows").and_then(|v| v.as_array()) {
        for row in rows {
            if let Some(e) = parse_entry(row) {
                entries.push(e);
            }
        }
    }
    let top = entries.first().cloned();
    let mine = identity.and_then(|me| entries.iter().find(|e| e.user_id == me.id).cloned());
    let snap = TokenRankSnapshot {
        available: top.is_some(),
        identity: identity.cloned(),
        top,
        mine,
        total_tokens,
        total_ranked_users: total_ranked,
        fetched_at_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        error: None,
    };
    write_cache(&snap, identity.map(|i| i.id));
    snap
}

fn parse_entry(row: &serde_json::Value) -> Option<TokenRankEntry> {
    // Row fields may sit at top level or under a `user` wrapper.
    let src = row.get("user").unwrap_or(row);
    Some(TokenRankEntry {
        rank: row.get("rank").and_then(|v| v.as_i64()).unwrap_or(0),
        user_id: src.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
        name: src
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("匿名用户")
            .to_string(),
        avatar_url: src
            .get("avatar_url")
            .or_else(|| src.get("avatarUrl"))
            .and_then(|v| v.as_str())
            .map(String::from),
        total_tokens: row.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
        call_count: row.get("call_count").and_then(|v| v.as_i64()).unwrap_or(0),
        session_count: row.get("session_count").and_then(|v| v.as_i64()).unwrap_or(0),
        clients: row
            .get("clients")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n)))
                    .collect()
            })
            .unwrap_or_default(),
        models: row
            .get("models")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n)))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn cached_fetch_stamp() -> u64 {
    fs::read_to_string(paths::token_rank_cache_json())
        .ok()
        .and_then(|text| serde_json::from_str::<CacheFile>(&text).ok())
        .map(|c| c.fetched_at_secs)
        .unwrap_or(0)
}

fn err_snapshot(msg: &str) -> TokenRankSnapshot {
    TokenRankSnapshot {
        available: false,
        error: Some(msg.to_string()),
        ..Default::default()
    }
}

// --- Cache (30 min), keyed by identity id ---

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    fetched_at_secs: u64,
    identity_id: Option<i64>,
    snapshot: TokenRankSnapshot,
}

fn read_fresh_cache(identity_id: Option<i64>) -> Option<TokenRankSnapshot> {
    let text = fs::read_to_string(paths::token_rank_cache_json()).ok()?;
    let cache: CacheFile = serde_json::from_str(&text).ok()?;
    if cache.identity_id != identity_id {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if cache.fetched_at_secs + CACHE_TTL_SECS < now {
        return None;
    }
    Some(cache.snapshot)
}

fn write_cache(snap: &TokenRankSnapshot, identity_id: Option<i64>) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = CacheFile {
        fetched_at_secs: now,
        identity_id,
        snapshot: snap.clone(),
    };
    let path = paths::token_rank_cache_json();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        let _ = fs::write(path, text);
    }
}
