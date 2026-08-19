//! Freshness model — a port of upstream macOS `FreshnessPolicy.swift` (v0.2.0
//! G-V1 / V1-T01). A unified six-state model describing how fresh each data
//! channel (collection, Codex quota, Claude quota) is, persisted per channel
//! in `cache/freshness-state.json`.
//!
//! Only classified error kinds are persisted — never raw error text
//! (upstream privacy contract).

use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;

/// Classified error kinds (upstream `UsageErrorKind`). Raw error strings are
/// mapped into one of these before persistence/display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageErrorKind {
    CollectionFailed,
    NetworkFailed,
    Unauthorized,
    Timeout,
    ParseFailed,
    Unknown,
}

impl UsageErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UsageErrorKind::CollectionFailed => "collection_failed",
            UsageErrorKind::NetworkFailed => "network_failed",
            UsageErrorKind::Unauthorized => "unauthorized",
            UsageErrorKind::Timeout => "timeout",
            UsageErrorKind::ParseFailed => "parse_failed",
            UsageErrorKind::Unknown => "unknown",
        }
    }

    /// Map a raw error message into a classified kind (keyword fallback —
    /// port of upstream `FreshnessPolicy.classify(error:)` text heuristics;
    /// the URL-layer codes map at the call site).
    pub fn from_error_text(text: &str) -> UsageErrorKind {
        let t = text.to_lowercase();
        for kw in ["unauthorized", "forbidden", "401", "403", "未授权", "登录", "凭据", "额度"] {
            if t.contains(kw) {
                return UsageErrorKind::Unauthorized;
            }
        }
        for kw in ["timed out", "timeout", "超时"] {
            if t.contains(kw) {
                return UsageErrorKind::Timeout;
            }
        }
        for kw in ["parse", "decode", "解析"] {
            if t.contains(kw) {
                return UsageErrorKind::ParseFailed;
            }
        }
        for kw in [
            "connect", "network", "dns", "connection", "offline", "网络", "无法连接",
        ] {
            if t.contains(kw) {
                return UsageErrorKind::NetworkFailed;
            }
        }
        UsageErrorKind::Unknown
    }
}

/// One channel's refresh-attempt history (upstream `RefreshAttemptRecord`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RefreshAttemptRecord {
    #[serde(rename = "last_attempted_at", default, skip_serializing_if = "Option::is_none")]
    pub last_attempted_at: Option<String>,
    #[serde(rename = "last_succeeded_at", default, skip_serializing_if = "Option::is_none")]
    pub last_succeeded_at: Option<String>,
    #[serde(rename = "last_failed_at", default, skip_serializing_if = "Option::is_none")]
    pub last_failed_at: Option<String>,
    #[serde(rename = "last_error_kind", default, skip_serializing_if = "Option::is_none")]
    pub last_error_kind: Option<UsageErrorKind>,
}

impl RefreshAttemptRecord {
    /// The most recent attempt time (attempted or failed, whichever is newer).
    fn latest_attempt(&self) -> Option<&str> {
        match (&self.last_attempted_at, &self.last_failed_at) {
            (Some(a), Some(b)) => Some(if a >= b { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Whether the latest attempt explicitly failed (upstream
    /// `lastAttemptFailed`).
    pub fn last_attempt_failed(&self) -> bool {
        match (&self.last_failed_at, self.latest_attempt()) {
            (Some(f), Some(l)) => f == l,
            _ => false,
        }
    }

    pub fn attempting(&mut self, now_iso: String) {
        self.last_attempted_at = Some(now_iso);
    }

    pub fn succeeding(&mut self, now_iso: String) {
        self.last_succeeded_at = Some(now_iso);
        self.last_failed_at = None;
        self.last_error_kind = None;
    }

    pub fn failing(&mut self, kind: UsageErrorKind, now_iso: String) {
        self.last_attempted_at = Some(now_iso.clone());
        self.last_failed_at = Some(now_iso);
        self.last_error_kind = Some(kind);
    }
}

/// The six freshness states (upstream `UsageFreshnessKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageFreshnessKind {
    NeverSucceeded,
    Fresh,
    Aging,
    Stale,
    Partial,
    Disabled,
}

impl UsageFreshnessKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UsageFreshnessKind::NeverSucceeded => "never_succeeded",
            UsageFreshnessKind::Fresh => "fresh",
            UsageFreshnessKind::Aging => "aging",
            UsageFreshnessKind::Stale => "stale",
            UsageFreshnessKind::Partial => "partial",
            UsageFreshnessKind::Disabled => "disabled",
        }
    }
}

/// Upstream `staleTTLMultiple`.
const STALE_TTL_MULTIPLE: f64 = 2.0;

/// Classify one channel. Port of upstream `FreshnessPolicy.classify(...)`
/// with the same short-circuit priority:
/// 1. disabled → 2. never succeeded → 3. last attempt failed → stale
/// 4. age > 2×TTL → stale → 5. some sources failed this cycle → partial
/// 6. age > TTL → aging → else fresh.
#[allow(clippy::too_many_arguments)]
pub fn classify(
    enabled: bool,
    record: &RefreshAttemptRecord,
    source_statuses: &[(String, String)],
    normal_ttl_secs: i64,
    now_epoch: i64,
) -> UsageFreshnessKind {
    if !enabled {
        return UsageFreshnessKind::Disabled;
    }
    let Some(last_succeeded) = &record.last_succeeded_at else {
        return UsageFreshnessKind::NeverSucceeded;
    };
    if record.last_attempt_failed() {
        return UsageFreshnessKind::Stale;
    }
    let succeeded_epoch = iso_to_epoch(last_succeeded).unwrap_or(now_epoch);
    let age = now_epoch - succeeded_epoch;
    if age > (normal_ttl_secs as f64 * STALE_TTL_MULTIPLE) as i64 {
        return UsageFreshnessKind::Stale;
    }
    let mut has_success = false;
    let mut has_failure = false;
    for (_, status) in source_statuses {
        match source_status_class(status) {
            SourceStatusClass::Success => has_success = true,
            SourceStatusClass::Absent => {}
            SourceStatusClass::Failure => has_failure = true,
        }
    }
    if has_success && has_failure {
        return UsageFreshnessKind::Partial;
    }
    if age > normal_ttl_secs {
        return UsageFreshnessKind::Aging;
    }
    UsageFreshnessKind::Fresh
}

/// Three-way classification of a source status string (upstream
/// `FreshnessPolicy.sourceStatusClass`): absent sources are NOT failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatusClass {
    Success,
    Absent,
    Failure,
}

pub fn source_status_class(status: &str) -> SourceStatusClass {
    match status {
        "ok" | "ok_sqlite" => SourceStatusClass::Success,
        // Absent ≠ failed: a missing tool must not flag the channel red.
        "disabled" | "missing" | "missing_db" | "missing_valid_rows" => SourceStatusClass::Absent,
        _ => SourceStatusClass::Failure,
    }
}

/// Persisted per-channel freshness state (upstream `FreshnessState`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FreshnessState {
    #[serde(default)]
    pub collection: RefreshAttemptRecord,
    #[serde(default)]
    pub codex_quota: RefreshAttemptRecord,
    #[serde(default)]
    pub claude_quota: RefreshAttemptRecord,
    #[serde(default = "default_state_version")]
    pub state_version: i64,
}

fn default_state_version() -> i64 {
    1
}

impl FreshnessState {
    pub fn load() -> FreshnessState {
        let path = paths::freshness_state_json();
        let Some(text) = fs::read_to_string(path).ok() else {
            return FreshnessState::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = paths::freshness_state_json();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}

// ── time helpers ──────────────────────────────────────────────────────────

/// Current time as an RFC3339-ish local ISO string (matches the format the
/// collector stamps on snapshots: `YYYY-MM-DDTHH:MM:SS+08:00`).
pub fn now_iso() -> String {
    crate::collector::now_iso()
}

/// Parse our ISO strings back to epoch seconds (tolerant: only the leading
/// `YYYY-MM-DDTHH:MM:SS` matters; timezone is always Asia/Shanghai +08:00).
pub fn iso_to_epoch(iso: &str) -> Option<i64> {
    let bytes = iso.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = iso.get(0..4)?.parse().ok()?;
    let month: i64 = iso.get(5..7)?.parse().ok()?;
    let day: i64 = iso.get(8..10)?.parse().ok()?;
    let hour: i64 = iso.get(11..13)?.parse().ok()?;
    let minute: i64 = iso.get(14..16)?.parse().ok()?;
    let second: i64 = iso.get(17..19)?.parse().ok()?;
    // Days since epoch via civil-date algorithm (Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    // Fixed +08:00 offset (Asia/Shanghai, no DST).
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) - 8 * 3_600)
}

/// Relative-time label port (upstream `FreshnessPolicy.relativeTime`):
/// 刚刚 (<60s) / N 分钟前 (<1h) / N 小时前 (<1d) / N 天前.
#[allow(dead_code)]
pub fn relative_time_label(iso: &str, now_epoch: i64) -> String {
    let Some(epoch) = iso_to_epoch(iso) else {
        return String::new();
    };
    let secs = (now_epoch - epoch).max(0);
    if secs < 60 {
        return "刚刚".to_string();
    }
    if secs < 3600 {
        return format!("{} 分钟前", secs / 60);
    }
    if secs < 86_400 {
        return format!("{} 小时前", secs / 3600);
    }
    format!("{} 天前", secs / 86_400)
}
