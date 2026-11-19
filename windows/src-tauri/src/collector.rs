//! Usage collector — a Rust port of `UsageCollector.swift` and the Python
//! `token_usage_monitor.py` collectors.
//!
//! Data sources (Windows paths, equivalent to the macOS `~/.codex` / `~/.claude`):
//!   - Codex: SQLite `state_5.sqlite` (primary) or session JSONL rollouts (fallback)
//!   - Claude Code: `~/.claude/projects/**/*.jsonl`
//!
//! SQLite is read in-process via `rusqlite` (bundled) — this replaces the
//! macOS approach of shelling out to `/usr/bin/sqlite3`, which does not exist
//! on Windows.

use crate::models::{
    DailyAgentWork, DailyRhythm, DailyUsage, HourlyTokenBucket, ModelUsage, RhythmTag, SourceInfo,
    TokenUsageCounts, ToolUsage, UsageSnapshot, UsageTotals,
};
use crate::paths;
use crate::pricing;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Timelike, Utc};
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
mod antigravity;
mod ccswitch;
mod claude;
mod codex;
mod cursor;
mod dsh;
mod minimax;
mod workbuddy;
mod zcode_hermes;

use self::antigravity::*;
use self::ccswitch::*;
use self::claude::*;
use self::codex::*;
use self::cursor::*;
use self::dsh::*;
use self::minimax::*;
use self::workbuddy::*;
use self::zcode_hermes::*;

/// Asia/Shanghai offset (+08:00), used to bucket timestamps into local days.
fn local_tz() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid offset")
}

/// A single parsed usage record before aggregation.
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub date: String,
    /// Original ISO timestamp (when available). Used to bucket tokens by hour
    /// for the rhythm feature. `None` for sources without sub-day resolution
    /// (e.g. Codex SQLite `threads` table) — those records skip rhythm.
    pub timestamp: Option<String>,
    pub tool: String,
    pub model: String,
    pub usage: TokenUsageCounts,
    /// Direct USD cost from sources that report it (e.g. CC Switch proxy logs),
    /// bypassing the pricing table. `None` → estimate from usage via pricing.
    #[allow(dead_code)]
    pub cost_usd: Option<f64>,
}

/// Public entry point: collect from all sources, aggregate, return a snapshot.
/// Mirrors upstream macOS `UsageCollector.collect()` (v0.2.0): Codex + Claude
/// Code + CC Switch records are summed; legacy experimental sources
/// (ZCode / Hermes / WorkBuddy) key off the master switch alone; T1 agent
/// sources (Gemini CLI / Qwen / Kimi / OpenCode / Amp / Droid / Grok) follow
/// the per-source list semantics of `agent_sources::enabled_ids`.
pub fn collect(
    include_experimental: bool,
    experimental_source_ids: Option<&Vec<String>>,
) -> UsageSnapshot {
    let pricing_data = pricing::load();

    // One shared collector cache for the whole pass: each collector only
    // prunes its own entries (single save AFTER all collectors — placing it
    // mid-pass once stranded the later collectors' fresh entries in memory,
    // forcing full rescans of WorkBuddy/Cursor/MiniMax/Antigravity every pass).
    let mut cache = load_cache();
    let (codex_records, mut codex_source) = collect_codex(&mut cache);
    let (claude_records, mut claude_source) = collect_claude_code(&mut cache);
    let (ccswitch_records, mut ccswitch_source) = collect_ccswitch();

    // Stamp accounting revision on Codex source (forward compat with macOS).
    codex_source.accounting_revision = Some(CODEX_ACCOUNTING_REVISION);

    let mut records: Vec<UsageRecord> = codex_records;
    records.extend(claude_records.iter().cloned());
    records.extend(ccswitch_records.iter().cloned());

    // Experimental agent sources (only when enabled in settings).
    let (zcode_records, zcode_source) = if include_experimental {
        collect_zcode()
    } else {
        (
            vec![],
            SourceInfo {
                status: Some("disabled".into()),
                ..Default::default()
            },
        )
    };
    let (hermes_records, hermes_source) = if include_experimental {
        collect_hermes()
    } else {
        (
            vec![],
            SourceInfo {
                status: Some("disabled".into()),
                ..Default::default()
            },
        )
    };
    let (workbuddy_records, workbuddy_source) = if include_experimental {
        collect_workbuddy(&mut cache)
    } else {
        (
            vec![],
            SourceInfo {
                status: Some("disabled".into()),
                ..Default::default()
            },
        )
    };
    records.extend(zcode_records.iter().cloned());
    records.extend(hermes_records.iter().cloned());
    let (cursor_transcript_records, cursor_transcript_source) =
        collect_cursor_transcripts(&mut cache);
    records.extend(cursor_transcript_records.iter().cloned());
    records.extend(workbuddy_records.iter().cloned());
    let (minimax_records, minimax_source) = collect_minimax();
    records.extend(minimax_records.iter().cloned());
    let (dsh_records, dsh_source) = collect_dsh();
    records.extend(dsh_records.iter().cloned());
    let (antigravity_records, antigravity_source) = collect_antigravity(&mut cache);
    records.extend(antigravity_records.iter().cloned());

    // Stamp per-source record counts (recount precisely per tool, since the
    // CC Switch source name differs from its record `tool` strings — e.g.
    // "Claude Code via CC Switch" — we count by source prefix instead).
    let mut counts: HashMap<String, i64> = HashMap::new();
    for r in &records {
        *counts.entry(r.tool.clone()).or_insert(0) += 1;
    }
    codex_source.records = counts.get("Codex").copied();
    claude_source.records = counts.get("Claude Code").copied();
    // CC Switch groups several tool names ("X via CC Switch"); sum them.
    let ccswitch_count: i64 = counts
        .iter()
        .filter(|(k, _)| {
            k.ends_with("via CC Switch") || k.ends_with("via CC Switch (experimental)")
        })
        .map(|(_, v)| *v)
        .sum();
    ccswitch_source.records = Some(ccswitch_count);

    let mut sources = BTreeMap::new();
    sources.insert("Codex".to_string(), codex_source);
    sources.insert("Claude Code".to_string(), claude_source);
    sources.insert("CC Switch Proxy".to_string(), ccswitch_source);
    sources.insert("ZCode".to_string(), zcode_source);
    sources.insert("Hermes Agent".to_string(), hermes_source);
    sources.insert("WorkBuddy".to_string(), workbuddy_source);
    sources.insert("Cursor".to_string(), cursor_transcript_source);
    sources.insert("MiniMax".to_string(), minimax_source);
    sources.insert("DSH".to_string(), dsh_source);
    sources.insert("Antigravity".to_string(), antigravity_source);

    // ── T1 secondary sources (0.1.13 D7 fix) ─────────────────────────────
    // Gemini CLI / Qwen Code / Kimi Code / OpenCode / Amp / Droid / Grok
    // Build: agent_sources::collect() existed since v0.1.44 but had NO
    // caller, so these seven never reached any panel. Same gating as the
    // settings per-source list (master switch + explicit list defaulting to
    // auto-enroll for detected sources).
    let agent_enabled: Vec<String> = match experimental_source_ids {
        Some(list) => list.clone(),
        None => crate::agent_sources::ALL_SOURCE_IDS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let mut agent_enabled_ref = agent_enabled;
    if !include_experimental {
        agent_enabled_ref.clear();
    }
    for (name, result) in crate::agent_sources::collect(&agent_enabled_ref) {
        records.extend(result.records.iter().cloned());
        sources.insert(name, result.source);
    }

    save_cache(&cache);
    aggregate(records, &pricing_data, sources)
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// How long the collector waits for a Codex JSONL scan before falling back to
/// the last completed scan. The sessions root can be a junction onto a
/// pathologically slow volume where a single read_dir/stat blocks for
/// minutes — nothing an in-loop deadline can cancel — so the scan runs on a
/// throwaway thread and a hung volume degrades Codex freshness instead of
/// freezing every refresh pass.
const CODEX_SCAN_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// Result of one Codex JSONL scan, kept so later passes can serve the last
/// good data while a scan thread is still stuck on slow storage.
#[derive(Debug, Clone)]
struct CodexScanOutcome {
    records: Vec<UsageRecord>,
    files: i64,
    timed_out: bool,
    live_paths: BTreeSet<String>,
    codex_entries: BTreeMap<String, CachedUsageFile>,
}

static CODEX_SCAN_MEMO: std::sync::Mutex<Option<CodexScanOutcome>> = std::sync::Mutex::new(None);

/// Set while a codex-scan thread is alive. While it stays set (thread stuck
/// on slow storage), later passes serve the memo instead of spawning more
/// threads — one detached thread max, not one per refresh interval.
static CODEX_SCAN_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn aggregate(
    records: Vec<UsageRecord>,
    pricing_data: &pricing::PricingFile,
    sources: BTreeMap<String, SourceInfo>,
) -> UsageSnapshot {
    let mut daily: BTreeMap<String, DailyAccumulator> = BTreeMap::new();
    let mut rhythms: BTreeMap<String, RhythmAccumulator> = BTreeMap::new();
    let mut tools: BTreeMap<String, UsageAccumulator> = BTreeMap::new();
    let mut models: BTreeMap<(String, String), UsageAccumulator> = BTreeMap::new();

    for record in &records {
        // Sources that report a direct USD cost (CC Switch proxy logs) bypass
        // the pricing table; others are estimated from token counts.
        let cost = record
            .cost_usd
            .filter(|c| c.is_finite())
            .unwrap_or_else(|| {
                pricing::estimate_cost(&record.usage, &record.tool, &record.model, pricing_data)
            });

        let daily_entry = daily
            .entry(record.date.clone())
            .or_insert_with(|| DailyAccumulator {
                date: record.date.clone(),
                ..Default::default()
            });
        *daily_entry.tools.entry(record.tool.clone()).or_insert(0) += record.usage.total_tokens;
        *daily_entry.models.entry(record.model.clone()).or_insert(0) += record.usage.total_tokens;
        daily_entry.total_tokens += record.usage.total_tokens;
        daily_entry.cost += cost;

        // Rhythm: bucket this record's tokens into its hour-of-day (0-23),
        // but only when we have a sub-day timestamp. Mirrors upstream
        // `RhythmAccumulator.add(tokens:hour:)`.
        if let Some(ref ts) = record.timestamp {
            if let Some(hour) = hour_from_iso(ts) {
                rhythms
                    .entry(record.date.clone())
                    .or_insert_with(|| RhythmAccumulator {
                        date: record.date.clone(),
                        ..Default::default()
                    })
                    .add(record.usage.total_tokens, hour);
            }
        }

        let tool_entry = tools.entry(record.tool.clone()).or_default();
        tool_entry.usage.add(&record.usage);
        tool_entry.cost += cost;

        let model_entry = models
            .entry((record.tool.clone(), record.model.clone()))
            .or_default();
        model_entry.usage.add(&record.usage);
        model_entry.cost += cost;
    }

    let total_tokens: i64 = tools.values().map(|a| a.usage.total_tokens).sum();
    let total_cost: f64 = tools.values().map(|a| a.cost).sum();

    let mut daily_rows: Vec<DailyUsage> = daily
        .into_values()
        .map(|d| DailyUsage {
            date: d.date,
            tools: d.tools,
            models: d.models,
            total_tokens: d.total_tokens,
            cost: round(d.cost, 4),
        })
        .collect();
    daily_rows.sort_by_key(|r| r.date.clone());

    let mut rhythm_rows: Vec<DailyRhythm> = rhythms
        .into_values()
        .map(|r| r.into_daily_rhythm())
        .filter(|r| r.total_tokens > 0)
        .collect();
    rhythm_rows.sort_by_key(|r| r.date.clone());

    let mut tool_rows: Vec<ToolUsage> = tools
        .into_iter()
        .map(|(tool, acc)| ToolUsage {
            tool,
            tokens: acc.usage.total_tokens,
            percent: Some(percent(acc.usage.total_tokens, total_tokens)),
        })
        .collect();
    tool_rows.sort_by_key(|r| std::cmp::Reverse(r.tokens));

    let mut model_rows: Vec<ModelUsage> = models
        .into_iter()
        .map(|((tool, model), acc)| ModelUsage {
            model,
            tool: Some(tool),
            tokens: acc.usage.total_tokens,
            percent: Some(percent(acc.usage.total_tokens, total_tokens)),
        })
        .collect();
    model_rows.sort_by_key(|r| std::cmp::Reverse(r.tokens));

    let active_days = daily_rows.iter().filter(|d| d.total_tokens > 0).count() as i64;

    UsageSnapshot {
        generated_at: Some(now_iso()),
        timezone: Some("Asia/Shanghai".to_string()),
        totals: UsageTotals {
            tokens: total_tokens,
            cost: round(total_cost, 2),
            active_days,
        },
        daily: daily_rows,
        rhythms: rhythm_rows,
        agent_work: aggregate_agent_work(&records),
        tools: tool_rows,
        models: model_rows,
        sources,
    }
}

#[derive(Default)]
struct DailyAccumulator {
    date: String,
    tools: BTreeMap<String, i64>,
    /// Per-model token breakdown for this day (mirrors upstream
    /// DailyAccumulator.models). Drives the Today view's model split.
    models: BTreeMap<String, i64>,
    total_tokens: i64,
    cost: f64,
}

#[derive(Default)]
struct UsageAccumulator {
    usage: TokenUsageCounts,
    cost: f64,
}

// ---------------------------------------------------------------------------
// Rhythm: per-day hourly token buckets + classification.
// Port of upstream `RhythmAccumulator` + `classify` + `companionTag`.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RhythmAccumulator {
    date: String,
    hourly_tokens: [i64; 24],
}

impl RhythmAccumulator {
    fn add(&mut self, tokens: i64, hour: u32) {
        if tokens > 0 && (hour as usize) < 24 {
            self.hourly_tokens[hour as usize] += tokens;
        }
    }

    /// Fold the hourly buckets into a `DailyRhythm`, computing peak/active-hour
    /// metrics and the classification tag. Mirrors upstream `dailyRhythm`.
    fn into_daily_rhythm(self) -> DailyRhythm {
        let buckets: Vec<HourlyTokenBucket> = self
            .hourly_tokens
            .iter()
            .enumerate()
            .map(|(hour, &tokens)| HourlyTokenBucket {
                hour: hour as i64,
                tokens,
            })
            .collect();

        let total_tokens: i64 = self.hourly_tokens.iter().sum();
        let (peak_hour, peak_tokens) = self
            .hourly_tokens
            .iter()
            .enumerate()
            .max_by(|(ha, ta), (hb, tb)| {
                if ta == tb {
                    ha.cmp(hb).reverse() // earlier hour wins ties (matches Swift max behavior)
                } else {
                    ta.cmp(tb)
                }
            })
            .filter(|(_, &t)| t > 0)
            .map(|(h, &t)| (h as i64, t))
            .unwrap_or((-1, 0));

        let active_threshold = Self::significant_threshold(total_tokens, peak_tokens);
        let significant: Vec<i64> = self
            .hourly_tokens
            .iter()
            .map(|&t| if t >= active_threshold { t } else { 0 })
            .collect();
        let active_hours = significant.iter().filter(|&&t| t > 0).count() as i64;
        let first_active_hour = significant.iter().position(|&t| t > 0).map(|h| h as i64);
        let last_active_hour = significant.iter().rposition(|&t| t > 0).map(|h| h as i64);

        let primary_tag = Self::classify(
            &self.hourly_tokens,
            &significant,
            total_tokens,
            if peak_hour >= 0 {
                Some(peak_hour)
            } else {
                None
            },
            peak_tokens,
            active_hours,
            first_active_hour,
        );

        DailyRhythm {
            date: self.date,
            buckets,
            total_tokens,
            peak_hour: if peak_hour >= 0 {
                Some(peak_hour)
            } else {
                None
            },
            peak_tokens,
            active_hours,
            first_active_hour,
            last_active_hour,
            primary_tag,
            companion_tag: Self::companion_tag(primary_tag),
        }
    }

    /// Threshold below which an hour's tokens are considered noise. Mirrors
    /// upstream `significantTokenThreshold`.
    fn significant_threshold(total_tokens: i64, peak_tokens: i64) -> i64 {
        let baseline = peak_tokens.max(total_tokens / 24);
        let scaled = ((baseline as f64) * 0.12).ceil() as i64;
        scaled.max(4)
    }

    /// Classify a day's usage into one of 10 rhythm tags. Port of upstream
    /// `classify` — pure numeric thresholds, no platform deps.
    #[allow(clippy::too_many_arguments)]
    fn classify(
        _hourly: &[i64; 24],
        significant: &[i64],
        total_tokens: i64,
        peak_hour: Option<i64>,
        peak_tokens: i64,
        active_hours: i64,
        first_active_hour: Option<i64>,
    ) -> RhythmTag {
        if total_tokens <= 0 {
            return RhythmTag::QuietDay;
        }
        let peak_share = share(peak_tokens, total_tokens);
        if is_double_peak(significant, peak_tokens) {
            return RhythmTag::DoublePeak;
        }
        if peak_share >= 0.50 {
            return RhythmTag::OneShot;
        }
        let night_share = share(
            tokens_in_hours(&[21, 22, 23, 0, 1, 2], significant),
            total_tokens,
        );
        if night_share >= 0.35
            || (peak_hour.map(|h| h >= 21 || h <= 2).unwrap_or(false) && night_share >= 0.25)
        {
            return RhythmTag::NightAgent;
        }
        let evening_share = share(tokens_in_hours(&[19, 20], significant), total_tokens);
        if peak_hour.map(|h| (19..=20).contains(&h)).unwrap_or(false) || evening_share >= 0.30 {
            return RhythmTag::EveningSprint;
        }
        let afternoon_share = share(tokens_in_hours_range(14, 18, significant), total_tokens);
        if afternoon_share >= 0.35
            || peak_hour.map(|h| (14..=18).contains(&h)).unwrap_or(false) && afternoon_share >= 0.25
        {
            return RhythmTag::AfternoonBurst;
        }
        let early_share = share(tokens_in_hours_range(5, 9, significant), total_tokens);
        if first_active_hour.map(|h| h <= 8).unwrap_or(false) && early_share >= 0.25 {
            return RhythmTag::EarlyStarter;
        }
        let morning_share = share(tokens_in_hours_range(8, 12, significant), total_tokens);
        if morning_share >= 0.35
            || peak_hour.map(|h| (8..=12).contains(&h)).unwrap_or(false) && morning_share >= 0.25
        {
            return RhythmTag::MorningPlanner;
        }
        if active_hours >= 6 && peak_share < 0.35 {
            return RhythmTag::Fragmented;
        }
        if active_hours >= 4 {
            return RhythmTag::SteadyCruise;
        }
        RhythmTag::QuietDay
    }

    /// The "opposite" rhythm, used as a companion suggestion. Port of
    /// upstream `companionTag`.
    fn companion_tag(tag: RhythmTag) -> RhythmTag {
        match tag {
            RhythmTag::EarlyStarter => RhythmTag::NightAgent,
            RhythmTag::MorningPlanner => RhythmTag::AfternoonBurst,
            RhythmTag::AfternoonBurst => RhythmTag::MorningPlanner,
            RhythmTag::EveningSprint => RhythmTag::SteadyCruise,
            RhythmTag::NightAgent => RhythmTag::EarlyStarter,
            RhythmTag::DoublePeak => RhythmTag::SteadyCruise,
            RhythmTag::Fragmented => RhythmTag::OneShot,
            RhythmTag::OneShot => RhythmTag::Fragmented,
            RhythmTag::SteadyCruise => RhythmTag::DoublePeak,
            RhythmTag::QuietDay => RhythmTag::MorningPlanner,
        }
    }
}

/// Whether the day has two distinct peaks (port of `isDoublePeak`).
fn is_double_peak(significant: &[i64], peak_tokens: i64) -> bool {
    if peak_tokens <= 0 {
        return false;
    }
    // Count hours whose tokens are at least 60% of the peak, excluding
    // adjacent hours (so a single fat peak isn't double-counted).
    let threshold = ((peak_tokens as f64) * 0.60).ceil() as i64;
    let mut peaks = 0;
    let mut prev_peak = false;
    for &t in significant {
        let is_peak = t >= threshold;
        if is_peak && !prev_peak {
            peaks += 1;
        }
        prev_peak = is_peak;
    }
    peaks >= 2
}

/// Sum tokens across a set of specific hours (port of `tokens(in:)`).
fn tokens_in_hours(hours: &[i32], significant: &[i64]) -> i64 {
    hours
        .iter()
        .filter_map(|&h| significant.get(h as usize))
        .sum()
}

/// Sum tokens across an inclusive range of hours.
fn tokens_in_hours_range(start: i32, end: i32, significant: &[i64]) -> i64 {
    (start..=end)
        .filter_map(|h| significant.get(h as usize))
        .sum()
}

/// Ratio of `part` to `whole`, as an f64 in [0,1]. Guards divide-by-zero.
fn share(part: i64, whole: i64) -> f64 {
    if whole <= 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// Extract the local hour-of-day (0-23) from an ISO timestamp. Port of
/// Current collector cache version. Bump on schema changes to force a full
/// rebuild from source. v5 = agent work aggregation + accounting revision tracking.
/// v11 = anchors thinned to CODEX_MAX_PERSISTED_ANCHORS per file; v10 caches
/// wrote every cumulative snapshot and could reach gigabytes, so they are
/// discarded instead of parsed.
const COLLECTOR_CACHE_VERSION: i32 = 12;
// v12: v11 entries written during the (rolled back) incremental-append
// experiment may hold truncated Codex record sets with an advanced
// scan_offset; bumping forces a clean rebuild.

/// Current Codex accounting revision (mirrors macOS `codexAccountingRevision`).
/// The Win port does not yet implement the rev6-8 incremental accounting, but
/// stamps this value on the Codex SourceInfo for forward compatibility.
pub const CODEX_ACCOUNTING_REVISION: i64 = 8;
/// Legacy revision assumed for old snapshots lacking an explicit value.
pub const LEGACY_CODEX_ACCOUNTING_REVISION: i64 = 5;
/// `day_string_from_iso`.
fn hour_from_iso(ts: &str) -> Option<u32> {
    parse_iso(ts).map(|dt| dt.hour())
}

/// Convert a CC Switch `created_at` epoch value (seconds OR milliseconds) into
/// an ISO-8601 string in local time, so rhythm can bucket it by hour.
fn iso_from_epoch(v: &rusqlite::types::Value) -> Option<String> {
    use rusqlite::types::Value;
    let raw: f64 = match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) => *r,
        Value::Text(s) => s.parse().ok()?,
        Value::Null => return None,
        Value::Blob(b) => String::from_utf8_lossy(b).parse().ok()?,
    };
    let secs: i64 = if raw > 1e12 {
        (raw / 1000.0) as i64
    } else {
        raw as i64
    };
    let tz = local_tz();
    let dt = tz.timestamp_opt(secs, 0).single()?;
    Some(dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

// ---------------------------------------------------------------------------
// Helpers: parsing, normalization, file enumeration, cache
// ---------------------------------------------------------------------------

fn model_key(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Mirror the Python `aliases` table used by `normalize_usage`.
fn normalize_usage(raw: Option<&serde_json::Value>) -> TokenUsageCounts {
    let mut usage = TokenUsageCounts::default();
    let Some(obj) = raw.and_then(|v| v.as_object()) else {
        return usage;
    };
    let alias = |key: &str| -> Option<&str> {
        match key {
            "input" | "input_tokens" => Some("input_tokens"),
            "output" | "output_tokens" => Some("output_tokens"),
            "cached" | "cache_read_input_tokens" | "cached_input_tokens" => {
                Some("cache_read_input_tokens")
            }
            "cache_creation_input_tokens" => Some("cache_creation_input_tokens"),
            "thoughts" | "reasoning_output_tokens" => Some("reasoning_output_tokens"),
            "total" | "total_tokens" => Some("total_tokens"),
            _ => None,
        }
    };
    for (key, value) in obj {
        let Some(target) = alias(key) else {
            continue;
        };
        let n = value_as_i64(value);
        match target {
            "input_tokens" => usage.input_tokens += n,
            "output_tokens" => usage.output_tokens += n,
            "cache_creation_input_tokens" => usage.cache_creation_input_tokens += n,
            "cache_read_input_tokens" => usage.cache_read_input_tokens += n,
            "reasoning_output_tokens" => usage.reasoning_output_tokens += n,
            "total_tokens" => usage.total_tokens += n,
            _ => {}
        }
    }
    usage
}

fn value_as_i64(v: &serde_json::Value) -> i64 {
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

// --- rusqlite Value converters (the `threads` columns are integers/text/null) ---

fn sqlite_value_as_f64(v: &rusqlite::types::Value) -> f64 {
    use rusqlite::types::Value;
    match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) => *r,
        Value::Text(s) => s.parse().unwrap_or(0.0),
        Value::Null => 0.0,
        Value::Blob(b) => String::from_utf8_lossy(b).parse().unwrap_or(0.0),
    }
}

fn sqlite_value_as_string(v: &rusqlite::types::Value) -> String {
    use rusqlite::types::Value;
    match v {
        Value::Text(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Null => String::new(),
        Value::Blob(b) => String::from_utf8_lossy(b).into_owned(),
    }
}

/// Codex `created_at` is stored as a Unix epoch (seconds) integer.
fn sqlite_value_as_epoch_day(v: &rusqlite::types::Value) -> Option<String> {
    use rusqlite::types::Value;
    let secs: f64 = match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) => *r,
        Value::Text(s) => s.parse().ok()?,
        Value::Null => return None,
        Value::Blob(b) => String::from_utf8_lossy(b).parse().ok()?,
    };
    let tz = local_tz();
    let dt = tz.timestamp_opt(secs as i64, 0).single()?;
    Some(dt.format("%Y-%m-%d").to_string())
}

/// Parse an ISO-8601 timestamp (optional fractional seconds / trailing 'Z')
/// and bucket it into a local day string.
pub fn day_string_from_iso(ts: &str) -> Option<String> {
    parse_iso(ts).map(|dt| dt.format("%Y-%m-%d").to_string())
}

pub fn parse_iso(ts: &str) -> Option<DateTime<FixedOffset>> {
    let tz = local_tz();
    // RFC3339 covers "+08:00" and "Z" offsets.
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&tz));
    }
    // Fall back to a naive UTC datetime for "YYYY-MM-DDTHH:MM:SS[.f]" (with or
    // without trailing Z), as emitted by both Codex and Claude Code logs.
    let cleaned = ts.trim_end_matches('Z');
    let parse_and_convert = |ndt: NaiveDateTime| {
        let utc = Utc.from_utc_datetime(&ndt);
        utc.with_timezone(&tz)
    };
    if let Ok(ndt) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(parse_and_convert(ndt));
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%dT%H:%M:%S") {
        return Some(parse_and_convert(ndt));
    }
    None
}

pub fn now_iso() -> String {
    let tz = local_tz();
    tz.from_utc_datetime(&Utc::now().naive_utc())
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

fn jsonl_files_under(root: &Path) -> Vec<PathBuf> {
    let pattern = root.join("**").join("*.jsonl");
    let Ok(entries) = glob::glob(&pattern.to_string_lossy()) else {
        return Vec::new();
    };
    entries.flatten().filter(|p| p.is_file()).collect()
}

/// Newest-first, deadline-bounded DFS for JSONL rollouts (Codex).
///
/// The Codex sessions root can be a junction onto a slow volume where even a
/// metadata walk takes minutes (41 GB / cloud-synced backups). The glob walk
/// cannot be cancelled and enumerates oldest-first, so on such roots the
/// collector would hang every pass AND never reach the newest sessions. This
/// walker descends directory names in DESCENDING order (date-structured
/// trees hit the newest days first) and stops at `deadline`.
/// Returns (files, timed_out); `timed_out` means the tree was NOT fully
/// enumerated and the result must not be treated as exhaustive.
fn jsonl_files_under_budget(root: &Path, deadline: std::time::Instant) -> (Vec<PathBuf>, bool) {
    let mut dir_cache = load_codex_dir_cache();
    let mut out: Vec<PathBuf> = Vec::new();
    // Descending sort: pop() takes the "largest" name first → newest first
    // for year/month/day style layouts.
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut timed_out = false;
    while let Some(dir) = stack.pop() {
        if std::time::Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let dir_key = dir.to_string_lossy().to_string();
        let dir_mtime = fs::metadata(&dir).ok().and_then(|m| {
            m.modified().ok().and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs_f64())
            })
        });
        // Short-circuit: directory unchanged since the last walk — reuse its
        // cached direct file list and only descend into its subdirectories
        // (their own mtimes are checked at their level, so any new session
        // file still gets discovered: appending bumps the day dir's mtime).
        if let (Some(mtime), Some(entry)) = (dir_mtime, dir_cache.dirs.get(&dir_key)) {
            if entry.mtime == mtime {
                for f in &entry.files {
                    out.push(PathBuf::from(f));
                }
                for sd in &entry.subdirs {
                    stack.push(PathBuf::from(sd));
                }
                continue;
            }
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => dirs.push(path),
                Ok(ft)
                    if ft.is_file()
                        && path.extension().and_then(|e| e.to_str()) == Some("jsonl") =>
                {
                    out.push(path.clone());
                    files.push(path.to_string_lossy().to_string());
                }
                _ => {}
            }
        }
        if let Some(mtime) = dir_mtime {
            dir_cache.dirs.insert(
                dir_key,
                CodexDirEntry {
                    mtime,
                    files,
                    subdirs: dirs
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect(),
                },
            );
        }
        dirs.sort_unstable();
        dirs.reverse();
        stack.extend(dirs);
    }
    save_codex_dir_cache(&dir_cache);
    (out, timed_out)
}

/// Per-directory walk cache for the Codex sessions tree (see
/// `jsonl_files_under_budget`): own mtime + direct jsonl files + subdir
/// names. Persisted so steady-state walks skip re-reading unchanged
/// directories on the slow volume.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CodexDirCache {
    version: i32,
    dirs: BTreeMap<String, CodexDirEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodexDirEntry {
    mtime: f64,
    files: Vec<String>,
    subdirs: Vec<String>,
}

fn load_codex_dir_cache() -> CodexDirCache {
    let path = paths::codex_dir_cache_json();
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<CodexDirCache>(&text).ok())
        .filter(|c| c.version == 1)
        .unwrap_or_default()
}

fn save_codex_dir_cache(cache: &CodexDirCache) {
    let path = paths::codex_dir_cache_json();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut out = serde_json::to_string(cache).unwrap_or_default();
    out.push('\n');
    let _ = fs::write(path, out);
}

fn percent(value: i64, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        round(value as f64 / total as f64 * 100.0, 2)
    }
}

fn round(value: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (value * m).round() / m
}

// ---------------------------------------------------------------------------
// File-level cache (mirrors Swift CollectorCache)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedUsageFile {
    tool: String,
    size: u64,
    mtime: f64,
    records: Vec<CachedRecord>,
    /// Codex fork metadata (rev8): the session this file forked from, its
    /// creation time, and the file's cumulative anchors — used so freshly
    /// scanned fork files can inherit the parent's counter instead of
    /// re-counting the inherited history.
    #[serde(
        default,
        rename = "codex_session",
        skip_serializing_if = "Option::is_none"
    )]
    codex_session: Option<String>,
    #[serde(
        default,
        rename = "codex_parent_session",
        skip_serializing_if = "Option::is_none"
    )]
    codex_parent_session: Option<String>,
    #[serde(
        default,
        rename = "codex_created_at",
        skip_serializing_if = "Option::is_none"
    )]
    codex_created_at: Option<f64>,
    /// Incremental append cursor: how many bytes of this file were already
    /// parsed into `records` (v12). Codex rollouts are append-only; a grown
    /// file only needs its tail re-parsed. Files rewritten in place (smaller
    /// size) fall back to a full rescan with offset 0.
    #[serde(default)]
    scan_offset: u64,
    #[serde(default)]
    codex_anchors: Vec<CachedAnchor>,
}

/// One cumulative anchor point: (epoch secs, usage snapshot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CachedAnchor {
    ts: f64,
    total: i64,
    input: i64,
    output: i64,
    cache_creation: i64,
    cache_read: i64,
    reasoning: i64,
}

impl CachedAnchor {
    fn usage(&self) -> TokenUsageCounts {
        TokenUsageCounts {
            input_tokens: self.input,
            output_tokens: self.output,
            cache_creation_input_tokens: self.cache_creation,
            cache_read_input_tokens: self.cache_read,
            reasoning_output_tokens: self.reasoning,
            total_tokens: self.total,
        }
    }

    fn from_usage(ts: f64, u: &TokenUsageCounts) -> CachedAnchor {
        CachedAnchor {
            ts,
            total: u.total_tokens,
            input: u.input_tokens,
            output: u.output_tokens,
            cache_creation: u.cache_creation_input_tokens,
            cache_read: u.cache_read_input_tokens,
            reasoning: u.reasoning_output_tokens,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRecord {
    date: String,
    #[serde(default)]
    timestamp: Option<String>,
    tool: String,
    model: String,
    usage: TokenUsageCounts,
    #[serde(default)]
    cost_usd: Option<f64>,
}

impl From<&UsageRecord> for CachedRecord {
    fn from(r: &UsageRecord) -> Self {
        CachedRecord {
            date: r.date.clone(),
            timestamp: r.timestamp.clone(),
            tool: r.tool.clone(),
            model: r.model.clone(),
            usage: r.usage.clone(),
            cost_usd: r.cost_usd,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CollectorCache {
    version: i32,
    files: BTreeMap<String, CachedUsageFile>,
}

impl Default for CollectorCache {
    fn default() -> Self {
        CollectorCache {
            version: COLLECTOR_CACHE_VERSION,
            files: BTreeMap::new(),
        }
    }
}

fn load_cache() -> CollectorCache {
    let path = paths::collector_cache_json();
    let Ok(mut file) = fs::File::open(&path) else {
        return CollectorCache::default();
    };
    // Cheap head probe: a bloated legacy cache can be gigabytes; a full serde
    // parse would cost GBs of RAM before the version check discards it. The
    // version key is always the first field both pretty and compact.
    use std::io::Read;
    let mut head = [0u8; 256];
    let n = file.read(&mut head).unwrap_or(0);
    let head = String::from_utf8_lossy(&head[..n]);
    if let Some(pos) = head.find("\"version\"") {
        let rest = head[pos + 9..].trim_start_matches([' ', ':', '\n', '\r', '\t']);
        let digits: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        if let Ok(v) = digits.parse::<i32>() {
            if v != COLLECTOR_CACHE_VERSION {
                return CollectorCache::default();
            }
        }
    }
    let Ok(text) = fs::read_to_string(&path) else {
        return CollectorCache::default();
    };
    match serde_json::from_str::<CollectorCache>(&text) {
        // Bump on schema change; old caches are rebuilt from source.
        Ok(c) if c.version == COLLECTOR_CACHE_VERSION => c,
        _ => CollectorCache::default(),
    }
}

fn save_cache(cache: &CollectorCache) {
    let path = paths::collector_cache_json();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    // Compact form: the cache is machine-only, and pretty printing a
    // multi-hundred-MB file inflates size and write time for nothing.
    if let Ok(text) = serde_json::to_string(cache) {
        let _ = fs::write(path, text);
    }
}

fn file_meta(path: &Path) -> Option<(u64, f64)> {
    let md = fs::metadata(path).ok()?;
    let size = md.len();
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    Some((size, mtime))
}

fn cached_records(
    cache: &CollectorCache,
    key: &str,
    tool: &str,
    path: &Path,
) -> Option<Vec<UsageRecord>> {
    let meta = file_meta(path)?;
    let entry = cache.files.get(key)?;
    if entry.tool != tool || entry.size != meta.0 || (entry.mtime - meta.1).abs() >= 0.001 {
        return None;
    }
    Some(
        entry
            .records
            .iter()
            .map(|c| UsageRecord {
                date: c.date.clone(),
                timestamp: c.timestamp.clone(),
                tool: c.tool.clone(),
                model: c.model.clone(),
                usage: c.usage.clone(),
                cost_usd: c.cost_usd,
            })
            .collect(),
    )
}

fn update_cache(
    cache: &mut CollectorCache,
    key: &str,
    tool: &str,
    path: &Path,
    records: &[UsageRecord],
) {
    let Some((size, mtime)) = file_meta(path) else {
        return;
    };
    cache.files.insert(
        key.to_string(),
        CachedUsageFile {
            tool: tool.to_string(),
            size,
            mtime,
            records: records.iter().map(CachedRecord::from).collect(),
            codex_session: None,
            codex_parent_session: None,
            codex_created_at: None,
            codex_anchors: vec![],
            scan_offset: 0,
        },
    );
}

// ---------------------------------------------------------------------------
// Experimental agent sources (port of upstream v0.1.44)
// ---------------------------------------------------------------------------

// Read ZCode agent usage from `~/.zcode/cli/db/db.sqlite`.
// ZCode's `input_tokens` already includes cached tokens.
//
// v0.2.0 (upstream 2826aac): the SQL is now *constructed conditionally* —
// `provider_total_tokens` may be absent on older schemas, and the optional
// `session` table (when present) is joined to pull `session.directory` as
// the project dimension.

/// Convert epoch seconds (or milliseconds if > 10^10) to a "YYYY-MM-DD" day
/// string in Asia/Shanghai.
pub fn day_string_from_epoch(seconds: f64) -> String {
    let secs = if seconds > 10_000_000_000.0 {
        seconds / 1000.0
    } else {
        seconds
    };
    let tz = local_tz();
    let dt = tz
        .timestamp_opt(secs as i64, 0)
        .single()
        .unwrap_or_else(|| tz.timestamp_opt(0, 0).unwrap());
    dt.format("%Y-%m-%d").to_string()
}

/// Convert epoch seconds (or milliseconds) to an ISO-8601 string in Asia/Shanghai.
pub fn iso_string_from_epoch(seconds: f64) -> String {
    let secs = if seconds > 10_000_000_000.0 {
        seconds / 1000.0
    } else {
        seconds
    };
    let tz = local_tz();
    let dt = tz
        .timestamp_opt(secs as i64, 0)
        .single()
        .unwrap_or_else(|| tz.timestamp_opt(0, 0).unwrap());
    dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

// ---------------------------------------------------------------------------
// Agent Work aggregation (port of upstream AgentWorkAccumulator)
// ---------------------------------------------------------------------------

/// One (source, hour) rollup cell in `AgentWorkAccumulator.hourly`.
type AgentWorkHourRollup = (i64, i64, i64, i64, bool);

/// Accumulator for agent-work per-date aggregation.
#[derive(Default)]
struct AgentWorkAccumulator {
    total_tokens: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    cache_coverage_complete: bool,
    active_hours: BTreeSet<i64>,
    model_request_count: i64,
    tool_call_count: i64,
    /// Per-source, per-hour rollup: (source, hour) → (tokens, input, cached, output, coverage_complete)
    hourly: BTreeMap<(String, i64), AgentWorkHourRollup>,
    unbucketed_tokens: i64,
}

impl AgentWorkAccumulator {
    #[allow(dead_code)] // upstream-parity constructor (agent-work tests)
    fn new() -> Self {
        // Start with coverage complete = true; AND across all records.
        Self {
            cache_coverage_complete: true,
            ..Default::default()
        }
    }

    fn add(&mut self, rec: &UsageRecord, hour: Option<i64>) {
        self.total_tokens += rec.usage.total_tokens;
        self.input_tokens += rec.usage.input_tokens;
        self.cached_input_tokens += rec.usage.cache_read_input_tokens;
        self.output_tokens += rec.usage.output_tokens;
        self.model_request_count += 1;
        // cache_coverage_complete: AND of the per-record formula
        let coverage = is_cache_coverage_complete(&rec.usage);
        self.cache_coverage_complete &= coverage;
        match hour {
            Some(h) => {
                self.active_hours.insert(h);
                let key = (rec.tool.clone(), h);
                let entry = self.hourly.entry(key).or_insert((0, 0, 0, 0, true));
                entry.0 += rec.usage.total_tokens;
                entry.1 += rec.usage.input_tokens;
                entry.2 += rec.usage.cache_read_input_tokens;
                entry.3 += rec.usage.output_tokens;
                entry.4 &= coverage;
            }
            None => {
                self.unbucketed_tokens += rec.usage.total_tokens;
            }
        }
    }

    fn finalize(self, date: &str) -> DailyAgentWork {
        use crate::models::{AgentWorkHourBucket, AgentWorkHourlySource, AgentWorkSource};
        // Build 24-hour buckets.
        let mut hourly_buckets: Vec<AgentWorkHourBucket> = Vec::new();
        for h in 0..24 {
            let mut hour_sources: Vec<AgentWorkHourlySource> = Vec::new();
            // Collect all sources for this hour.
            let mut source_map: BTreeMap<String, (i64, i64, i64, i64, bool)> = BTreeMap::new();
            for ((src, hr), (tokens, inp, cached, outp, cov)) in &self.hourly {
                if *hr == h {
                    let e = source_map.entry(src.clone()).or_insert((0, 0, 0, 0, true));
                    e.0 += tokens;
                    e.1 += inp;
                    e.2 += cached;
                    e.3 += outp;
                    e.4 &= cov;
                }
            }
            for (src, (tokens, inp, cached, outp, cov)) in source_map {
                hour_sources.push(AgentWorkHourlySource {
                    source: src,
                    tokens,
                    input_tokens: inp,
                    cached_input_tokens: cached,
                    output_tokens: outp,
                    cache_coverage_complete: cov,
                });
            }
            if !hour_sources.is_empty() {
                hourly_buckets.push(AgentWorkHourBucket {
                    hour: h,
                    sources: hour_sources,
                });
            }
        }
        // Pad to 24 buckets if fewer (Mac always normalizes to 24).
        // (We already iterate 0..24, but only add non-empty hours. The Mac
        // version pads with empty entries — here we leave them out since the
        // UI can handle sparse buckets.)

        // Build per-source aggregates.
        let mut source_map: BTreeMap<String, (i64, i64, i64)> = BTreeMap::new();
        for ((src, _), (tokens, _, _, _, _)) in &self.hourly {
            let e = source_map.entry(src.clone()).or_insert((0, 0, 0));
            e.0 += tokens;
            e.1 += 1; // model request count proxy
        }
        // Also count unbucketed tokens per source.
        // (approximate: attribute unbucketed to tool from records)
        let sources: Vec<AgentWorkSource> = source_map
            .into_iter()
            .map(|(src, (tokens, reqs, _))| AgentWorkSource {
                source: src,
                tokens,
                model_request_count: reqs,
                tool_call_count: 0,
            })
            .collect();

        let unbucketed = if self.unbucketed_tokens > 0 {
            self.unbucketed_tokens
        } else {
            (self.total_tokens - self.hourly.values().map(|(t, _, _, _, _)| t).sum::<i64>()).max(0)
        };

        DailyAgentWork {
            date: date.to_string(),
            total_tokens: self.total_tokens,
            input_tokens: self.input_tokens,
            cached_input_tokens: self.cached_input_tokens,
            output_tokens: self.output_tokens,
            cache_coverage_complete: self.cache_coverage_complete,
            active_hours: self.active_hours.len() as i64,
            model_request_count: self.model_request_count,
            tool_call_count: self.tool_call_count,
            sources,
            hourly_buckets,
            unbucketed_tokens: unbucketed,
        }
    }
}

/// Check if a record's token breakdown is internally consistent (cache
/// coverage complete). Mirrors macOS's validation formula.
fn is_cache_coverage_complete(u: &TokenUsageCounts) -> bool {
    u.input_tokens >= 0
        && u.output_tokens >= 0
        && u.cache_creation_input_tokens >= 0
        && u.cache_read_input_tokens >= 0
        && u.reasoning_output_tokens >= 0
        && u.total_tokens == u.input_tokens + u.output_tokens
        && u.cache_creation_input_tokens + u.cache_read_input_tokens <= u.input_tokens
        && u.reasoning_output_tokens <= u.output_tokens
}

/// Aggregate records into agent-work data, keyed by date.
fn aggregate_agent_work(records: &[UsageRecord]) -> Vec<DailyAgentWork> {
    let mut accs: BTreeMap<String, AgentWorkAccumulator> = BTreeMap::new();
    for rec in records {
        // Agent work includes all records that have timestamps (sub-day resolution).
        let hour = rec
            .timestamp
            .as_ref()
            .and_then(|ts| hour_from_iso(ts))
            .map(|h| h as i64);
        let acc = accs.entry(rec.date.clone()).or_default();
        acc.add(rec, hour);
    }
    accs.into_iter()
        .filter(|(_, a)| a.total_tokens > 0)
        .map(|(date, a)| a.finalize(&date))
        .collect()
}

#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod scan_regression_tests;
