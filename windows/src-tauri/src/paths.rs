//! Filesystem locations for TokenStep on Windows.
//!
//! Mirrors the macOS app's layout under `~/Library/Application Support/TokenStep`,
//! but rooted at `%APPDATA%\TokenStep` on Windows (the conventional per-user
//! app-data directory, which is also the roaming profile location).

use std::path::PathBuf;

/// `%APPDATA%\TokenStep` (or a sane fallback if the env var is unset).
pub fn app_support_root() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("home directory"))
        .join("TokenStep")
}

/// `data/usage.json` — the generated token usage snapshot.
#[allow(dead_code)]
pub fn usage_json() -> PathBuf {
    app_support_root().join("data").join("usage.json")
}

/// `cache/collector-cache.json` — per-file parsed-records cache.
pub fn collector_cache_json() -> PathBuf {
    app_support_root().join("cache").join("collector-cache.json")
}

/// `config/settings.json` — daily goal, refresh interval, history days.
pub fn settings_json() -> PathBuf {
    app_support_root().join("config").join("settings.json")
}

/// `logs/` — reserved for future login-item logs.
#[allow(dead_code)]
pub fn logs_dir() -> PathBuf {
    app_support_root().join("logs")
}

/// The user's home directory (`%USERPROFILE%`), where `.codex` / `.claude` live.
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `~/.codex/state_5.sqlite` and the alternate `~/.codex/sqlite/state_5.sqlite`.
pub fn codex_sqlite_candidates() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        home.join(".codex").join("state_5.sqlite"),
        home.join(".codex").join("sqlite").join("state_5.sqlite"),
    ]
}

/// Roots to scan for Codex JSONL rollout files.
pub fn codex_jsonl_roots() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        home.join(".codex").join("sessions"),
        home.join(".codex").join("archived_sessions"),
    ]
}

/// Root to scan for Claude Code project JSONL files.
pub fn claude_projects_root() -> PathBuf {
    home_dir().join(".claude").join("projects")
}

/// `~/.cc-switch/cc-switch.db` — the CC Switch proxy request log (SQLite).
/// CC Switch is a local proxy that routes Claude/Codex/Gemini traffic; its DB
/// holds per-request token + cost rows we aggregate as a usage source.
pub fn ccswitch_db_candidates() -> Vec<PathBuf> {
    vec![home_dir().join(".cc-switch").join("cc-switch.db")]
}

/// `~/.claude/.credentials.json` — Claude Code OAuth credentials (written by
/// the Claude Code CLI on `claude login`). Used to read the access token for
/// the Claude usage-quota API. May be absent if the user never signed in.
pub fn claude_credentials_json() -> PathBuf {
    home_dir().join(".claude").join(".credentials.json")
}

/// `cache/claude-quota-cache.json` — 10-min cache for the Claude usage quota.
pub fn claude_quota_cache_json() -> PathBuf {
    app_support_root()
        .join("cache")
        .join("claude-quota-cache.json")
}

/// `cache/token-rank-cache.json` — 120s cache for the TokenRank leaderboard.
pub fn token_rank_cache_json() -> PathBuf {
    app_support_root()
        .join("cache")
        .join("token-rank-cache.json")
}
