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
    DailyUsage, ModelUsage, SourceInfo, TokenUsageCounts, ToolUsage, UsageSnapshot,
    UsageTotals,
};
use crate::paths;
use crate::pricing;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Asia/Shanghai offset (+08:00), used to bucket timestamps into local days.
fn local_tz() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid offset")
}

/// A single parsed usage record before aggregation.
#[derive(Debug, Clone)]
struct UsageRecord {
    date: String,
    tool: String,
    model: String,
    usage: TokenUsageCounts,
}

/// Public entry point: collect from all sources, aggregate, return a snapshot.
pub fn collect() -> UsageSnapshot {
    let pricing_data = pricing::load();

    let (codex_records, mut codex_source) = collect_codex();
    let (claude_records, mut claude_source) = collect_claude_code();

    let mut records: Vec<UsageRecord> = codex_records;
    let claude_count = records.len();
    records.extend(claude_records.iter().cloned());

    // Stamp per-source record counts.
    let codex_count = claude_count; // (codex came first; recounted below)
    let _ = codex_count;

    // Recount precisely per tool.
    let mut counts: HashMap<String, i64> = HashMap::new();
    for r in &records {
        *counts.entry(r.tool.clone()).or_insert(0) += 1;
    }
    if let Some(n) = counts.get("Codex") {
        codex_source.records = Some(*n);
    }
    if let Some(n) = counts.get("Claude Code") {
        claude_source.records = Some(*n);
    }

    let mut sources = BTreeMap::new();
    sources.insert("Codex".to_string(), codex_source);
    sources.insert("Claude Code".to_string(), claude_source);

    aggregate(records, &pricing_data, sources)
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

fn collect_codex() -> (Vec<UsageRecord>, SourceInfo) {
    // Primary: JSONL rollouts (per-turn token counts), matching the Python
    // collector's precedence. SQLite `threads` (per-thread totals) is only a
    // fallback when no JSONL data exists.
    let mut cache = load_cache();
    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let (records, files) = collect_codex_jsonl(&mut cache, &mut live_paths);
    cache.files.retain(|k, _| live_paths.contains(k));
    save_cache(&cache);

    if !records.is_empty() {
        return (
            records,
            SourceInfo {
                status: Some("ok".to_string()),
                files: Some(files),
                records: None,
            },
        );
    }

    // Fallback: SQLite `threads` table (per-thread totals).
    if let Some((records, files)) = collect_codex_sqlite() {
        if !records.is_empty() {
            return (
                records,
                SourceInfo {
                    status: Some("fallback_threads".to_string()),
                    files: Some(files as i64),
                    records: None,
                },
            );
        }
    }

    (
        Vec::new(),
        SourceInfo {
            status: Some("missing".to_string()),
            files: Some(files),
            records: None,
        },
    )
}

/// Read Codex usage from the `threads` table. `tokens_used` is a per-thread
/// total — the value the macOS app surfaces when JSONL has no token data.
fn collect_codex_sqlite() -> Option<(Vec<UsageRecord>, usize)> {
    let db_path = paths::codex_sqlite_candidates()
        .into_iter()
        .find(|p| p.exists())?;
    // Read-only open; safe even while Codex is running.
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    let mut stmt = conn
        .prepare("select created_at, model, tokens_used from threads where tokens_used > 0")
        .ok()?;

    let rows = stmt
        .query_map([], |row| {
            let created_at: rusqlite::types::Value = row.get(0)?;
            let model: rusqlite::types::Value = row.get(1)?;
            let tokens: rusqlite::types::Value = row.get(2)?;
            Ok((created_at, model, tokens))
        })
        .ok()?;

    let mut records = Vec::new();
    for row in rows.flatten() {
        let (created_at, model, tokens) = row;
        let tokens = sqlite_value_as_f64(&tokens);
        if tokens <= 0.0 {
            continue;
        }
        let day = match sqlite_value_as_epoch_day(&created_at) {
            Some(d) => d,
            None => continue,
        };
        let mut usage = TokenUsageCounts::default();
        usage.total_tokens = tokens as i64;
        records.push(UsageRecord {
            date: day,
            tool: "Codex".to_string(),
            model: model_key(&sqlite_value_as_string(&model)),
            usage,
        });
    }

    Some((records, 1))
}

fn collect_codex_jsonl(
    cache: &mut CollectorCache,
    live_paths: &mut BTreeSet<String>,
) -> (Vec<UsageRecord>, i64) {
    let roots = paths::codex_jsonl_roots();
    let mut all_paths = Vec::new();
    for root in &roots {
        all_paths.extend(jsonl_files_under(root));
    }
    all_paths.sort();

    let mut records: Vec<UsageRecord> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for path in &all_paths {
        let key = path.to_string_lossy().to_string();
        live_paths.insert(key.clone());

        if let Some(cached) = cached_records(cache, &key, "Codex", path) {
            records.extend(cached);
            continue;
        }

        let mut file_records: Vec<UsageRecord> = Vec::new();
        let mut session_id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut current_model = String::from("unknown");
        let mut event_index = 0i64;

        // Stream the file line-by-line instead of read_to_string, so a 9.7 GB
        // rollout never sits fully in memory (memory optimization, port of
        // macOS commit 170f655). Single line is parsed + dropped each iteration.
        let Ok(file) = fs::File::open(path) else { continue };
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        for line in reader.lines().map_while(Result::ok) {
            // Cheap marker pre-filter, like the Swift scan.
            if !line.contains("token_count")
                && !line.contains("session_meta")
                && !line.contains("turn_context")
            {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let ev_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let payload = obj.get("payload");

            if ev_type == "session_meta" {
                if let Some(id) = payload.and_then(|p| p.get("id")).and_then(|v| v.as_str()) {
                    if !id.is_empty() {
                        session_id = id.to_string();
                    }
                }
            }
            if ev_type == "turn_context" {
                if let Some(m) = payload.and_then(|p| p.get("model")).and_then(|v| v.as_str()) {
                    if !m.is_empty() {
                        current_model = model_key(m);
                    }
                }
            }
            if ev_type != "event_msg" {
                continue;
            }
            let inner_type = payload.and_then(|p| p.get("type")).and_then(|v| v.as_str());
            if inner_type != Some("token_count") {
                continue;
            }
            let info = payload.and_then(|p| p.get("info"));
            let raw_usage = info.and_then(|i| i.get("last_token_usage"));
            let mut usage = normalize_usage(raw_usage);
            usage.finalize_total();
            if usage.total_tokens <= 0 {
                continue;
            }
            let Some(timestamp) = obj.get("timestamp").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(day) = day_string_from_iso(timestamp) else {
                continue;
            };
            event_index += 1;
            let dedupe = format!("{session_id}|{timestamp}|{event_index}|{}", usage.total_tokens);
            if !seen.insert(dedupe) {
                continue;
            }
            file_records.push(UsageRecord {
                date: day,
                tool: "Codex".to_string(),
                model: current_model.clone(),
                usage,
            });
        }

        records.extend(file_records.clone());
        update_cache(cache, &key, "Codex", path, &file_records);
    }

    (records, all_paths.len() as i64)
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

fn collect_claude_code() -> (Vec<UsageRecord>, SourceInfo) {
    let root = paths::claude_projects_root();
    let mut all_paths = jsonl_files_under(&root);
    all_paths.sort();

    let mut cache = load_cache();
    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let mut records: Vec<UsageRecord> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for path in &all_paths {
        let key = path.to_string_lossy().to_string();
        live_paths.insert(key.clone());

        if let Some(cached) = cached_records(&cache, &key, "Claude Code", path) {
            records.extend(cached);
            continue;
        }

        let mut file_records: Vec<UsageRecord> = Vec::new();
        // Stream line-by-line (memory optimization, port of macOS 170f655).
        let Ok(file) = fs::File::open(path) else { continue };
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        for (line_no, line) in reader.lines().map_while(Result::ok).enumerate() {
            if !line.contains("usage") {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if obj.get("type").and_then(|v| v.as_str()) != Some("assistant") {
                continue;
            }
            let Some(message) = obj.get("message") else {
                continue;
            };
            let mut usage = normalize_usage(message.get("usage"));
            usage.finalize_total();
            if usage.total_tokens <= 0 {
                continue;
            }
            let Some(timestamp) = obj.get("timestamp").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(day) = day_string_from_iso(timestamp) else {
                continue;
            };
            let unique = obj
                .get("uuid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{key}:{line_no}"));
            if !seen.insert(unique) {
                continue;
            }
            let model = model_key(
                message
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            );
            file_records.push(UsageRecord {
                date: day,
                tool: "Claude Code".to_string(),
                model,
                usage,
            });
        }

        records.extend(file_records.clone());
        update_cache(&mut cache, &key, "Claude Code", path, &file_records);
    }

    cache.files.retain(|k, _| live_paths.contains(k));
    save_cache(&cache);

    let status = if records.is_empty() { "missing" } else { "ok" };
    (
        records,
        SourceInfo {
            status: Some(status.to_string()),
            files: Some(all_paths.len() as i64),
            records: None,
        },
    )
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

fn aggregate(
    records: Vec<UsageRecord>,
    pricing_data: &pricing::PricingFile,
    sources: BTreeMap<String, SourceInfo>,
) -> UsageSnapshot {
    let mut daily: BTreeMap<String, DailyAccumulator> = BTreeMap::new();
    let mut tools: BTreeMap<String, UsageAccumulator> = BTreeMap::new();
    let mut models: BTreeMap<(String, String), UsageAccumulator> = BTreeMap::new();

    for record in &records {
        let cost = pricing::estimate_cost(&record.usage, &record.tool, &record.model, pricing_data);

        let daily_entry = daily
            .entry(record.date.clone())
            .or_insert_with(|| DailyAccumulator {
                date: record.date.clone(),
                ..Default::default()
            });
        *daily_entry
            .tools
            .entry(record.tool.clone())
            .or_insert(0) += record.usage.total_tokens;
        daily_entry.total_tokens += record.usage.total_tokens;
        daily_entry.cost += cost;

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
            total_tokens: d.total_tokens,
            cost: round(d.cost, 4),
        })
        .collect();
    daily_rows.sort_by(|a, b| a.date.cmp(&b.date));

    let mut tool_rows: Vec<ToolUsage> = tools
        .into_iter()
        .map(|(tool, acc)| ToolUsage {
            tool,
            tokens: acc.usage.total_tokens,
            percent: Some(percent(acc.usage.total_tokens, total_tokens)),
        })
        .collect();
    tool_rows.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    let mut model_rows: Vec<ModelUsage> = models
        .into_iter()
        .map(|((tool, model), acc)| ModelUsage {
            model,
            tool: Some(tool),
            tokens: acc.usage.total_tokens,
            percent: Some(percent(acc.usage.total_tokens, total_tokens)),
        })
        .collect();
    model_rows.sort_by(|a, b| b.tokens.cmp(&a.tokens));

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
        tools: tool_rows,
        models: model_rows,
        sources,
    }
}

#[derive(Default)]
struct DailyAccumulator {
    date: String,
    tools: BTreeMap<String, i64>,
    total_tokens: i64,
    cost: f64,
}

#[derive(Default)]
struct UsageAccumulator {
    usage: TokenUsageCounts,
    cost: f64,
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
fn day_string_from_iso(ts: &str) -> Option<String> {
    parse_iso(ts).map(|dt| dt.format("%Y-%m-%d").to_string())
}

fn parse_iso(ts: &str) -> Option<DateTime<FixedOffset>> {
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

fn now_iso() -> String {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRecord {
    date: String,
    tool: String,
    model: String,
    usage: TokenUsageCounts,
}

impl From<&UsageRecord> for CachedRecord {
    fn from(r: &UsageRecord) -> Self {
        CachedRecord {
            date: r.date.clone(),
            tool: r.tool.clone(),
            model: r.model.clone(),
            usage: r.usage.clone(),
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
            version: 1,
            files: BTreeMap::new(),
        }
    }
}

fn load_cache() -> CollectorCache {
    let Ok(text) = fs::read_to_string(paths::collector_cache_json()) else {
        return CollectorCache::default();
    };
    match serde_json::from_str::<CollectorCache>(&text) {
        Ok(c) if c.version == 1 => c,
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
    if let Ok(text) = serde_json::to_string_pretty(cache) {
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
    if entry.tool != tool
        || entry.size != meta.0
        || (entry.mtime - meta.1).abs() >= 0.001
    {
        return None;
    }
    Some(
        entry
            .records
            .iter()
            .map(|c| UsageRecord {
                date: c.date.clone(),
                tool: c.tool.clone(),
                model: c.model.clone(),
                usage: c.usage.clone(),
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
        },
    );
}
