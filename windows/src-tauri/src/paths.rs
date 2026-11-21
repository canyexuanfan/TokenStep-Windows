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
    app_support_root()
        .join("cache")
        .join("collector-cache.json")
}

/// `cache/codex-dirs.json` — per-directory walk cache for the Codex
/// sessions tree (own mtime + direct jsonl files + subdir names), letting
/// steady-state walks skip re-reading unchanged directories on slow volumes.
pub fn codex_dir_cache_json() -> PathBuf {
    app_support_root().join("cache").join("codex-dirs.json")
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
    home_dir()
        .join(".zcode")
        .join("cli")
        .join("db")
        .join("db.sqlite")
}

/// `~/.hermes/state.db` — Hermes agent usage database.
pub fn hermes_db_path() -> PathBuf {
    home_dir().join(".hermes").join("state.db")
}

/// Directories to probe for WorkBuddy presence (no usage extracted yet).
pub fn workbuddy_roots() -> Vec<PathBuf> {
    vec![home_dir().join(".workbuddy")]
}

/// Per-project JSONL event streams (~/.workbuddy/projects/): each
/// function_call line carries timestamp/model/message.usage — the same
/// event_usage source the Token Rank client scans.
/// ~/.cursor/projects — Cursor agent session transcripts (JSONL).
pub fn cursor_transcript_roots() -> Vec<PathBuf> {
    vec![home_dir().join(".cursor").join("projects")]
}

/// `~/.minimax/sqlite.db` — MiniMax desktop runtime usage store (legacy,
/// May–Jul 2026 era; table `token_usage`).
pub fn minimax_db_path() -> PathBuf {
    home_dir().join(".minimax").join("sqlite.db")
}

/// `~/.dsh/.dshw-usage.json` — DeepSeek Harness per-call billing journal
/// (events: [{ts, day, model, cost, tokens}]; the dir is usually a symlink
/// to the harness install's data root). FALLBACK source only: it was rebuilt
/// on 2026-11-16 (recharge-fix), so it misses older history.
pub fn dsh_usage_json() -> PathBuf {
    home_dir().join(".dsh").join(".dshw-usage.json")
}

/// `~/.dsh/dsh-tokenlens/usage-cache.json` — the harness's own TokenLens
/// statistics cache (the exact source its stats panel renders). Per session
/// file: {mtimeMs, size, frames, days, providers, models, hours, ...} where
/// `models` maps "providerId/modelId" → {model, today:{day:counts},
/// total:counts} with counts {input, output, cacheRead, cacheWrite, total,
/// messages}. Authoritative full history (matches the panel's 1.5B).
pub fn dsh_tokenlens_cache() -> PathBuf {
    home_dir()
        .join(".dsh")
        .join("dsh-tokenlens")
        .join("usage-cache.json")
}

/// Antigravity brain transcript roots: `~/.gemini/antigravity{,-cli,-ide}/
/// brain/<session>/.system_generated/logs/chunks/transcript/*.jsonl`. The
/// IDE/CLI keep per-session agent transcripts (with per-turn token counts)
/// here, alongside the legacy Gemini CLI tree the same home already hosts.
pub fn antigravity_brain_roots() -> Vec<PathBuf> {
    let gemini = home_dir().join(".gemini");
    vec![
        gemini.join("antigravity"),
        gemini.join("antigravity-cli"),
        gemini.join("antigravity-ide"),
    ]
}

/// `Roaming/Antigravity/User/globalStorage/state.vscdb` — the Antigravity
/// app's VS Code-style state store; its ItemTable holds the cockpit
/// extension's sealed account credentials (secret:// keys).
pub fn antigravity_secret_db() -> PathBuf {
    let roaming = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(roaming)
        .join("Antigravity")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// `~/.minimax/v2/sqlite/runtime-state.sqlite` — the live MiniMax runtime
/// store (table `local_runtime_token_usage`, same schema as the legacy one).
pub fn minimax_runtime_db_path() -> PathBuf {
    home_dir()
        .join(".minimax")
        .join("v2")
        .join("sqlite")
        .join("runtime-state.sqlite")
}

/// `~/.minimax/v2/sessions` — MiniMax per-session detail directories
/// (`YYYY/MM/DD/<time>-session_<base64(sessionId)>/`), each carrying
/// `llm-call.json` (session model config) and `messages.jsonl` (per-message
/// model + usage). These are the attribution source for token-usage rows the
/// client left without a model.
pub fn minimax_sessions_root() -> PathBuf {
    home_dir().join(".minimax").join("v2").join("sessions")
}

pub fn workbuddy_project_roots() -> Vec<PathBuf> {
    vec![home_dir().join(".workbuddy").join("projects")]
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
    app_support_root()
        .join("cache")
        .join("freshness-state.json")
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
    home_dir()
        .join(".local")
        .join("share")
        .join("amp")
        .join("threads")
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
    home_dir()
        .join(".cursor")
        .join("ai-tracking")
        .join("ai-code-tracking.db")
}

/// cache/cursor-usage-cache.json — official usage events cache.
pub fn cursor_usage_cache_json() -> PathBuf {
    app_support_root()
        .join("cache")
        .join("cursor-usage-cache.json")
}
