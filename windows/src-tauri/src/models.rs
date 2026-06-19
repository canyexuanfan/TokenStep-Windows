//! Data models — a 1:1 port of `UsageModels.swift`, serialized with the same
//! snake_case keys the macOS / Python versions emit so the generated
//! `usage.json` stays interchangeable across platforms.

use serde::{Deserialize, Serialize};

/// Token breakdown for a single usage record. Mirrors the Swift
/// `TokenUsageCounts` (private in the original) plus the Python
/// `empty_usage()` shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    pub cost: f64,
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

/// The full aggregated snapshot, written to `data/usage.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSnapshot {
    #[serde(rename = "generated_at", skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub totals: UsageTotals,
    pub daily: Vec<DailyUsage>,
    pub tools: Vec<ToolUsage>,
    pub models: Vec<ModelUsage>,
    pub sources: std::collections::BTreeMap<String, SourceInfo>,
}

impl UsageSnapshot {
    pub fn empty() -> Self {
        UsageSnapshot {
            generated_at: None,
            timezone: Some("Asia/Shanghai".to_string()),
            totals: UsageTotals::default(),
            daily: vec![],
            tools: vec![],
            models: vec![],
            sources: std::collections::BTreeMap::new(),
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
    /// Color theme: green (default) / ocean / violet / amber / graphite.
    #[serde(rename = "theme", default = "default_theme")]
    pub theme: String,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "green".to_string()
}

impl Default for TokenStepSettings {
    fn default() -> Self {
        Self {
            daily_goal_tokens: 100_000_000,
            refresh_interval_seconds: 60,
            history_days: 180,
            close_to_tray: true,
            theme: "green".to_string(),
        }
    }
}
