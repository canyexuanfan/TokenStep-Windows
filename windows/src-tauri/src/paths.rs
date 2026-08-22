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
/// NOTE: upstream (f93acce) stopped scanning `archived_sessions` to avoid
/// double-counting sessions that were already tallied before being archived.
pub fn codex_jsonl_roots() -> Vec<PathBuf> {
    let home = home_dir();
    vec![home.join(".codex").join("sessions")]
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

// ── Experimental agent source paths (port of upstream v0.1.44) ──────────

/// `~/.zcode/cli/db/db.sqlite` — ZCode agent usage database.
pub fn zcode_db_path() -> PathBuf {
    home_dir().join(".zcode").join("cli").join("db").join("db.sqlite")
}

/// `~/.hermes/state.db` — Hermes agent usage database.
pub fn hermes_db_path() -> PathBuf {
    home_dir().join(".hermes").join("state.db")
}

/// Directories to probe for WorkBuddy presence (no usage extracted yet).
pub fn workbuddy_roots() -> Vec<PathBuf> {
    vec![home_dir().join(".workbuddy")]
}

/// `config/usage-recalibration-v6-pending` — marker file written when a
/// Codex accounting recalibration occurred. Its presence triggers the green
/// "Token 已重新校准" notice banner on the dashboard.
pub fn usage_recalibration_notice_marker() -> PathBuf {
    app_support_root()
        .join("config")
        .join("usage-recalibration-v6-pending")
}

// ── Freshness state (port of upstream v0.2.0 G-V1) ───────────────────────

/// `cache/freshness-state.json` — per-channel refresh-attempt records.
pub fn freshness_state_json() -> PathBuf {
    app_support_root().join("cache").join("freshness-state.json")
}

// ── T1 experimental agent source paths (port of upstream v0.2.0 G-A1) ────

/// `~/.gemini/tmp` — Gemini CLI session dumps (session-*.json / .jsonl).
pub fn gemini_tmp_dir() -> PathBuf {
    home_dir().join(".gemini").join("tmp")
}

/// `~/.qwen/tmp` or `~/.qwen/projects` — Qwen Code usage JSONL roots.
pub fn qwen_roots() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        home.join(".qwen").join("tmp"),
        home.join(".qwen").join("projects"),
    ]
}

/// `~/.kimi-code/sessions` — Kimi Code wire.jsonl sessions (the `.kimi`
/// legacy dir has no usage events, so collection reads only this one).
pub fn kimi_sessions_dir() -> PathBuf {
    home_dir().join(".kimi-code").join("sessions")
}

/// `~/.local/share/opencode/opencode.db` — OpenCode usage database.
pub fn opencode_db_path() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db")
}

/// `~/.local/share/amp/threads` — Amp thread JSONL root.
pub fn amp_threads_dir() -> PathBuf {
    home_dir().join(".local").join("share").join("amp").join("threads")
}

/// `~/.factory/sessions` — Droid session JSONL root.
pub fn droid_sessions_dir() -> PathBuf {
    home_dir().join(".factory").join("sessions")
}

/// `~/.grok/sessions` — Grok Build updates.jsonl root.
pub fn grok_sessions_dir() -> PathBuf {
    home_dir().join(".grok").join("sessions")
}

// ── Agent-work-rank identity (port of upstream v0.2.0) ───────────────────

/// `~/.token-rank/client-state.json` — written by the Token Rank CLI; holds
/// the local identity `{user:{id,...}}` used to locate "my" leaderboard row.
pub fn token_rank_client_state_json() -> PathBuf {
    home_dir().join(".token-rank").join("client-state.json")
}

// ── Cursor (upstream v0.2.1/v0.2.2) ───────────────────────────────────────

/// Cursor's VS Code state DB (holds the auth token in ItemTable), used for
/// the official usage API. Windows: %APPDATA%/Cursor/User/globalStorage/state.vscdb
pub fn cursor_state_vscdb() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
        .join("Cursor")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// Cursor's local code-signal DB: ~/.cursor/ai-tracking/ai-code-tracking.db
pub fn cursor_code_tracking_db() -> PathBuf {
    home_dir().join(".cursor").join("ai-tracking").join("ai-code-tracking.db")
}

/// cache/cursor-usage-cache.json — official usage events cache.
pub fn cursor_usage_cache_json() -> PathBuf {
    app_support_root().join("cache").join("cursor-usage-cache.json")
}
