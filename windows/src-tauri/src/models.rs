//! Data models — a 1:1 port of `UsageModels.swift`, serialized with the same
//! snake_case keys the macOS / Python versions emit so the generated
//! `usage.json` stays interchangeable across platforms.

use serde::{Deserialize, Serialize};

/// Token breakdown for a single usage record. Mirrors the Swift
/// `TokenUsageCounts` (private in the original) plus the Python
/// `empty_usage()` shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageCounts {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

impl TokenUsageCounts {
    /// Same total-derivation rule as the original: if no explicit total was
    /// present, sum the parts.
    pub fn finalize_total(&mut self) {
        if self.total_tokens <= 0 {
            self.total_tokens = self.input_tokens
                + self.output_tokens
                + self.cache_creation_input_tokens
                + self.cache_read_input_tokens
                + self.reasoning_output_tokens;
        }
    }

    pub fn add(&mut self, other: &TokenUsageCounts) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }
}

/// Per-source collection status.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records: Option<i64>,
    /// Token accounting revision used to collect this source. Mirrors macOS
    /// `SourceInfo.accountingRevision`; the current revision is 8. Legacy
    /// snapshots (pre-rev8) default to 5.
    #[serde(rename = "accounting_revision", default, skip_serializing_if = "Option::is_none")]
    pub accounting_revision: Option<i64>,
    /// When a recalibration occurred, this holds the *previous* revision the
    /// user's data was collected with. Drives the "Token 已重新校准" notice.
    #[serde(rename = "recalibrated_from_revision", default, skip_serializing_if = "Option::is_none")]
    pub recalibrated_from_revision: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    pub tokens: i64,
    pub cost: f64,
    #[serde(rename = "active_days")]
    pub active_days: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub tools: std::collections::BTreeMap<String, i64>,
    /// Per-model token breakdown for this day (mirrors upstream
    /// `DailyAccumulator.models`). Lets the Today view show today's model
    /// split without recomputing from the global models table.
    #[serde(default)]
    pub models: std::collections::BTreeMap<String, i64>,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    pub cost: f64,
    /// Per-project breakdown for this day (upstream B1-lite). Older
    /// snapshots decode as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ProjectUsage>>,
}

/// A sanitized project aggregate (upstream `ProjectUsage`, B1-lite). The name
/// is the last path segment of the working directory — never a full path.
/// An empty name means "unassigned" (records without a project context).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectUsage {
    pub name: String,
    pub tokens: i64,
    pub cost: f64,
    pub tools: std::collections::BTreeMap<String, i64>,
    pub models: std::collections::BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolUsage {
    pub tool: String,
    pub tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
}

/// A single hour bucket (0-23) with its token total — port of upstream
/// `HourlyTokenBucket`. Drives the 24h rhythm chart.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HourlyTokenBucket {
    pub hour: i64,
    pub tokens: i64,
}

/// The rhythm classification tag assigned to a day based on when tokens were
/// consumed. Port of upstream `RhythmTag` (snake_case raw values).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RhythmTag {
    EarlyStarter,
    MorningPlanner,
    AfternoonBurst,
    EveningSprint,
    NightAgent,
    DoublePeak,
    Fragmented,
    OneShot,
    SteadyCruise,
    QuietDay,
}

impl Default for RhythmTag {
    fn default() -> Self {
        RhythmTag::QuietDay
    }
}

/// A day's rhythm profile: hourly token buckets + derived metrics + a
/// classification tag. Port of upstream `DailyRhythm`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyRhythm {
    pub date: String,
    pub buckets: Vec<HourlyTokenBucket>,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    #[serde(rename = "peak_hour", default, skip_serializing_if = "Option::is_none")]
    pub peak_hour: Option<i64>,
    #[serde(rename = "peak_tokens")]
    pub peak_tokens: i64,
    #[serde(rename = "active_hours")]
    pub active_hours: i64,
    #[serde(rename = "first_active_hour", default, skip_serializing_if = "Option::is_none")]
    pub first_active_hour: Option<i64>,
    #[serde(rename = "last_active_hour", default, skip_serializing_if = "Option::is_none")]
    pub last_active_hour: Option<i64>,
    #[serde(rename = "primary_tag")]
    pub primary_tag: RhythmTag,
    #[serde(rename = "companion_tag")]
    pub companion_tag: RhythmTag,
}

// ── Agent Work data models (port of upstream DailyAgentWork) ─────────────

/// Per-tool, per-hour agent work breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentWorkHourlySource {
    pub source: String,
    pub tokens: i64,
    #[serde(rename = "input_tokens", default)]
    pub input_tokens: i64,
    #[serde(rename = "cached_input_tokens", default)]
    pub cached_input_tokens: i64,
    #[serde(rename = "output_tokens", default)]
    pub output_tokens: i64,
    #[serde(rename = "cache_coverage_complete", default)]
    pub cache_coverage_complete: bool,
}

/// One hour bucket (0-23) in the agent work chart.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentWorkHourBucket {
    pub hour: i64,
    pub sources: Vec<AgentWorkHourlySource>,
}

impl AgentWorkHourBucket {
    pub fn total_tokens(&self) -> i64 {
        self.sources.iter().map(|s| s.tokens).sum()
    }
}

/// Per-tool aggregate for the day.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentWorkSource {
    pub source: String,
    pub tokens: i64,
    #[serde(rename = "model_request_count", default)]
    pub model_request_count: i64,
    #[serde(rename = "tool_call_count", default)]
    pub tool_call_count: i64,
}

/// A day's agent work profile: token/input/output/cache breakdown + hourly
/// distribution + per-source aggregates. Port of upstream `DailyAgentWork`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyAgentWork {
    pub date: String,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    #[serde(rename = "input_tokens", default)]
    pub input_tokens: i64,
    #[serde(rename = "cached_input_tokens", default)]
    pub cached_input_tokens: i64,
    #[serde(rename = "output_tokens", default)]
    pub output_tokens: i64,
    #[serde(rename = "cache_coverage_complete", default)]
    pub cache_coverage_complete: bool,
    #[serde(rename = "active_hours", default)]
    pub active_hours: i64,
    #[serde(rename = "model_request_count", default)]
    pub model_request_count: i64,
    #[serde(rename = "tool_call_count", default)]
    pub tool_call_count: i64,
    #[serde(default)]
    pub sources: Vec<AgentWorkSource>,
    #[serde(default)]
    pub hourly_buckets: Vec<AgentWorkHourBucket>,
    #[serde(rename = "unbucketed_tokens", default)]
    pub unbucketed_tokens: i64,
}

impl DailyAgentWork {
    /// Cache hit rate — only valid when coverage is complete and input > 0.
    pub fn cache_hit_rate(&self) -> Option<f64> {
        if self.cache_coverage_complete && self.input_tokens > 0 && self.cached_input_tokens <= self.input_tokens {
            Some(self.cached_input_tokens as f64 / self.input_tokens as f64)
        } else {
            None
        }
    }
}

/// The full aggregated snapshot, written to `data/usage.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSnapshot {
    #[serde(rename = "generated_at", skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub totals: UsageTotals,
    pub daily: Vec<DailyUsage>,
    #[serde(default)]
    pub rhythms: Vec<DailyRhythm>,
    #[serde(default, rename = "agent_work")]
    pub agent_work: Vec<DailyAgentWork>,
    pub tools: Vec<ToolUsage>,
    pub models: Vec<ModelUsage>,
    pub sources: std::collections::BTreeMap<String, SourceInfo>,
    /// Info about the collection attempt that produced this snapshot
    /// (upstream G-V1 freshness). Older snapshots decode as `None`.
    #[serde(default, rename = "source_attempt", skip_serializing_if = "Option::is_none")]
    pub source_attempt: Option<crate::freshness::RefreshAttemptRecord>,
    /// Project-dimension aggregates (upstream B1-lite, sanitized last path
    /// segment only). Older snapshots decode as empty.
    #[serde(default)]
    pub projects: Vec<ProjectUsage>,
}

impl UsageSnapshot {
    pub fn empty() -> Self {
        UsageSnapshot {
            generated_at: None,
            timezone: Some("Asia/Shanghai".to_string()),
            totals: UsageTotals::default(),
            daily: vec![],
            rhythms: vec![],
            agent_work: vec![],
            tools: vec![],
            models: vec![],
            sources: std::collections::BTreeMap::new(),
            source_attempt: None,
            projects: vec![],
        }
    }
}

/// Settings persisted to `config/settings.json`. Mirrors `TokenStepSettings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStepSettings {
    #[serde(rename = "daily_goal_tokens")]
    pub daily_goal_tokens: i64,
    #[serde(rename = "refresh_interval_seconds")]
    pub refresh_interval_seconds: i64,
    #[serde(rename = "history_days")]
    pub history_days: i64,
    /// When true (default), closing the dashboard window hides it to the tray
    /// instead of quitting the app. Set false to make the X button quit.
    #[serde(rename = "close_to_tray", default = "default_true")]
    pub close_to_tray: bool,
    /// Launch TokenStep on system startup (HKCU Run key).
    #[serde(rename = "autostart", default)]
    pub autostart: bool,
    /// Whether to automatically check for updates on launch (default true).
    #[serde(rename = "auto_update_enabled", default = "default_true")]
    pub auto_update_enabled: bool,
    /// Whether to ask before downloading an update (vs silent download).
    #[serde(rename = "ask_before_downloading_updates", default = "default_true")]
    pub ask_before_downloading_updates: bool,
    /// Whether to only install releases from verified/signed sources.
    #[serde(rename = "require_verified_updates", default = "default_true")]
    pub require_verified_updates: bool,
    /// Color theme: green (default) / ocean / violet / amber / graphite.
    #[serde(rename = "theme", default = "default_theme")]
    pub theme: String,
    /// Directory where screenshots are saved by default (empty = exe dir).
    #[serde(rename = "screenshot_dir", default)]
    pub screenshot_dir: String,
    /// UI language: zhHans (default) / en / zhHant.
    #[serde(rename = "language", default = "default_lang")]
    pub language: String,
    /// Whether the Codex quota card (5h / 7d rate limits) is shown on the
    /// Today view. Mirrors upstream `showCodexQuota`; default off so the card
    /// only appears when the user opts in.
    #[serde(rename = "show_codex_quota", default)]
    pub show_codex_quota: bool,
    /// Agent-work-rank card visibility (upstream `AgentWorkRankVisibility`):
    /// "automatic" (show when a local Token Rank identity is detected),
    /// "visible" (always show), or "hidden" (zero identity reads, zero
    /// network). Default "hidden" — absence must never silently enable.
    #[serde(rename = "agent_work_rank_visibility", default = "default_rank_visibility")]
    pub agent_work_rank_visibility: String,
    /// Per-source experimental agent toggles (source display names). `None`
    /// = master switch alone (legacy ZCode/Hermes/WorkBuddy semantics, and
    /// auto-enroll detected T1 sources); an explicit list decides which T1
    /// sources participate. New sources are never enabled by legacy
    /// boolean migration.
    #[serde(rename = "experimental_agent_sources", default, skip_serializing_if = "Option::is_none")]
    pub experimental_agent_sources: Option<Vec<String>>,
    /// G-S1 device sync — default off, no enablement surface until the
    /// server contract ships (mirrors upstream v0.2.0 dead-code state).
    #[serde(rename = "device_sync_enabled", default)]
    pub device_sync_enabled: bool,
    #[serde(rename = "merge_today_all_devices", default)]
    pub merge_today_all_devices: bool,
    #[serde(rename = "merge_history_all_devices", default)]
    pub merge_history_all_devices: bool,
    #[serde(rename = "hidden_device_ids", default)]
    pub hidden_device_ids: Vec<String>,
    /// A version string the user chose to skip via the update dialog.
    /// When the latest release matches this, the update check reports
    /// `has_update: false` so the user isn't nagged about it again.
    #[serde(rename = "skipped_update_version", default, skip_serializing_if = "Option::is_none")]
    pub skipped_update_version: Option<String>,
    /// Whether experimental agent sources (ZCode / Hermes / WorkBuddy) are
    /// collected and shown. Default off. Mirrors upstream
    /// `showExperimentalAgentSources`.
    #[serde(rename = "show_experimental_agent_sources", default)]
    pub show_experimental_agent_sources: bool,
}

fn default_lang() -> String {
    "zhHans".to_string()
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "green".to_string()
}

fn default_rank_visibility() -> String {
    "hidden".to_string()
}

impl Default for TokenStepSettings {
    fn default() -> Self {
        Self {
            daily_goal_tokens: 100_000_000,
            refresh_interval_seconds: 60,
            history_days: 180,
            close_to_tray: true,
            autostart: false,
            auto_update_enabled: true,
            ask_before_downloading_updates: true,
            require_verified_updates: true,
            theme: "green".to_string(),
            screenshot_dir: String::new(),
            language: "zhHans".to_string(),
            skipped_update_version: None,
            show_codex_quota: false,
            agent_work_rank_visibility: "hidden".to_string(),
            experimental_agent_sources: None,
            device_sync_enabled: false,
            merge_today_all_devices: false,
            merge_history_all_devices: false,
            hidden_device_ids: vec![],
            show_experimental_agent_sources: false,
        }
    }
}
