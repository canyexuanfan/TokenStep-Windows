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
    /// Direct USD cost from sources that report it (e.g. CC Switch proxy logs),
    /// bypassing the pricing table. `None` → estimate from usage via pricing.
    #[allow(dead_code)]
    cost_usd: Option<f64>,
}

/// Public entry point: collect from all sources, aggregate, return a snapshot.
pub fn collect() -> UsageSnapshot {
    let pricing_data = pricing::load();

    let (codex_records, mut codex_source) = collect_codex();
    let (claude_records, mut claude_source) = collect_claude_code();
    let (ccswitch_records, mut ccswitch_source) = collect_ccswitch();

    let mut records: Vec<UsageRecord> = codex_records;
    records.extend(claude_records.iter().cloned());
    records.extend(ccswitch_records.iter().cloned());

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
        .filter(|(k, _)| k.ends_with("via CC Switch") || k.ends_with("via CC Switch (experimental)"))
        .map(|(_, v)| *v)
        .sum();
    ccswitch_source.records = Some(ccswitch_count);

    let mut sources = BTreeMap::new();
    sources.insert("Codex".to_string(), codex_source);
    sources.insert("Claude Code".to_string(), claude_source);
    sources.insert("CC Switch Proxy".to_string(), ccswitch_source);

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
            cost_usd: None,
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
                cost_usd: None,
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
                cost_usd: None,
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
// CC Switch proxy (SQLite) — port of upstream `collectCCSwitchProxyUsage`.
// CC Switch is a local proxy routing Claude/Codex/Gemini traffic; its DB
// stores per-request token + cost rows we aggregate as a usage source.
// ---------------------------------------------------------------------------

/// Map a CC Switch `app_type` to the tool name shown in the UI, mirroring
/// upstream `ccSwitchToolName`.
fn cc_switch_tool_name(app_type: &str) -> String {
    let normalized = app_type.trim().to_lowercase();
    match normalized.as_str() {
        "claude" => "Claude Code via CC Switch".to_string(),
        "codex" => "Codex via CC Switch".to_string(),
        "gemini" => "Gemini via CC Switch".to_string(),
        _ => {
            let raw = app_type.trim();
            let label = if raw.is_empty() { "unknown" } else { raw };
            format!("{} via CC Switch (experimental)", label)
        }
    }
}

/// Bucket a CC Switch `created_at` (epoch seconds OR milliseconds) into a
/// local day string. Mirrors upstream's epoch-millis compatibility.
fn cc_switch_epoch_day(v: &rusqlite::types::Value) -> Option<String> {
    use rusqlite::types::Value;
    let raw: f64 = match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) => *r,
        Value::Text(s) => s.parse().ok()?,
        Value::Null => return None,
        Value::Blob(b) => String::from_utf8_lossy(b).parse().ok()?,
    };
    // Heuristic: values > 1e12 are milliseconds (epoch ms vs epoch s).
    let secs: i64 = if raw > 1e12 { (raw / 1000.0) as i64 } else { raw as i64 };
    let tz = local_tz();
    let dt = tz.timestamp_opt(secs, 0).single()?;
    Some(dt.format("%Y-%m-%d").to_string())
}

fn collect_ccswitch() -> (Vec<UsageRecord>, SourceInfo) {
    let missing = |status: &str| {
        (
            Vec::new(),
            SourceInfo {
                status: Some(status.to_string()),
                files: Some(0),
                records: Some(0),
            },
        )
    };

    let db_path = match paths::ccswitch_db_candidates().into_iter().find(|p| p.exists()) {
        Some(p) => p,
        None => return missing("missing_db"),
    };

    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => {
            return (
                Vec::new(),
                SourceInfo {
                    status: Some("unreadable_db".to_string()),
                    files: Some(1),
                    records: Some(0),
                },
            )
        }
    };

    // Validate the schema before querying — mirrors upstream's
    // `pragma table_info(proxy_request_logs)` + required-columns check.
    let required: BTreeSet<&str> = [
        "request_id",
        "app_type",
        "provider_id",
        "model",
        "request_model",
        "pricing_model",
        "input_tokens",
        "output_tokens",
        "cache_read_tokens",
        "cache_creation_tokens",
        "total_cost_usd",
        "status_code",
        "created_at",
        "data_source",
    ]
    .into_iter()
    .collect();

    let available: BTreeSet<String> = match conn.prepare("pragma table_info(proxy_request_logs)") {
        Ok(mut s) => match s.query_map([], |row| row.get::<_, String>(1)) {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => {
                return (
                    Vec::new(),
                    SourceInfo {
                        status: Some("schema_unreadable".to_string()),
                        files: Some(1),
                        records: Some(0),
                    },
                )
            }
        },
        Err(_) => {
            return (
                Vec::new(),
                SourceInfo {
                    status: Some("schema_unreadable".to_string()),
                    files: Some(1),
                    records: Some(0),
                },
            )
        }
    };
    if available.is_empty() {
        return (
            Vec::new(),
            SourceInfo {
                status: Some("missing_table".to_string()),
                files: Some(1),
                records: Some(0),
            },
        );
    }
    let all_present = required.iter().all(|r| available.contains(*r));
    if !all_present {
        return (
            Vec::new(),
            SourceInfo {
                status: Some("schema_mismatch".to_string()),
                files: Some(1),
                records: Some(0),
            },
        );
    }

    // The aggregate query mirrors upstream exactly (coalesce model fallbacks,
    // filter to successful proxy rows with tokens).
    let sql = "select \
        created_at, app_type, \
        coalesce(nullif(pricing_model, ''), nullif(model, ''), nullif(request_model, ''), 'unknown') as display_model, \
        coalesce(input_tokens, 0) as input_tokens, \
        coalesce(output_tokens, 0) as output_tokens, \
        coalesce(cache_read_tokens, 0) as cache_read_tokens, \
        coalesce(cache_creation_tokens, 0) as cache_creation_tokens, \
        cast(coalesce(nullif(total_cost_usd, ''), '0') as real) as total_cost_usd \
        from proxy_request_logs \
        where coalesce(data_source, 'proxy') = 'proxy' \
          and status_code >= 200 and status_code < 300 \
          and (coalesce(input_tokens, 0) + coalesce(output_tokens, 0) \
               + coalesce(cache_read_tokens, 0) + coalesce(cache_creation_tokens, 0)) > 0 \
        order by created_at, request_id";

    // Collect rows inside the prepare block so the statement's borrow ends
    // before we build records (the Rows iterator borrows the statement).
    let row_data: Vec<(
        rusqlite::types::Value,
        rusqlite::types::Value,
        rusqlite::types::Value,
        rusqlite::types::Value,
        rusqlite::types::Value,
        rusqlite::types::Value,
        rusqlite::types::Value,
        rusqlite::types::Value,
    )> = match conn.prepare(sql) {
        Ok(mut s) => match s.query_map([], |row| {
            Ok((
                row.get::<_, rusqlite::types::Value>(0)?,
                row.get::<_, rusqlite::types::Value>(1)?,
                row.get::<_, rusqlite::types::Value>(2)?,
                row.get::<_, rusqlite::types::Value>(3)?,
                row.get::<_, rusqlite::types::Value>(4)?,
                row.get::<_, rusqlite::types::Value>(5)?,
                row.get::<_, rusqlite::types::Value>(6)?,
                row.get::<_, rusqlite::types::Value>(7)?,
            ))
        }) {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => {
                return (
                    Vec::new(),
                    SourceInfo {
                        status: Some("query_failed".to_string()),
                        files: Some(1),
                        records: Some(0),
                    },
                )
            }
        },
        Err(_) => {
            return (
                Vec::new(),
                SourceInfo {
                    status: Some("query_failed".to_string()),
                    files: Some(1),
                    records: Some(0),
                },
            )
        }
    };

    let mut records = Vec::new();
    for (
        created_at,
        app_type,
        display_model,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        total_cost_usd,
    ) in row_data
    {

        let day = match cc_switch_epoch_day(&created_at) {
            Some(d) => d,
            None => continue,
        };
        let input = sqlite_value_as_f64(&input_tokens) as i64;
        let output = sqlite_value_as_f64(&output_tokens) as i64;
        let cache_read = sqlite_value_as_f64(&cache_read_tokens) as i64;
        let cache_creation = sqlite_value_as_f64(&cache_creation_tokens) as i64;
        let total = input + output + cache_read + cache_creation;
        if total <= 0 {
            continue;
        }

        let cost_usd = {
            let raw = sqlite_value_as_f64(&total_cost_usd);
            if raw.is_finite() && raw > 0.0 {
                Some(raw)
            } else {
                None
            }
        };

        records.push(UsageRecord {
            date: day,
            tool: cc_switch_tool_name(&sqlite_value_as_string(&app_type)),
            model: model_key(&sqlite_value_as_string(&display_model)),
            usage: TokenUsageCounts {
                input_tokens: input,
                output_tokens: output,
                cache_read_input_tokens: cache_read,
                cache_creation_input_tokens: cache_creation,
                reasoning_output_tokens: 0,
                total_tokens: total,
            },
            cost_usd,
        });
    }

    let status = if records.is_empty() { "missing_proxy_rows" } else { "ok" };
    let count = records.len();
    (
        records,
        SourceInfo {
            status: Some(status.to_string()),
            files: Some(1),
            records: Some(count as i64),
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
        // Sources that report a direct USD cost (CC Switch proxy logs) bypass
        // the pricing table; others are estimated from token counts.
        let cost = record
            .cost_usd
            .filter(|c| c.is_finite())
            .unwrap_or_else(|| pricing::estimate_cost(&record.usage, &record.tool, &record.model, pricing_data));

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
        *daily_entry
            .models
            .entry(record.model.clone())
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
            models: d.models,
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
    #[serde(default)]
    cost_usd: Option<f64>,
}

impl From<&UsageRecord> for CachedRecord {
    fn from(r: &UsageRecord) -> Self {
        CachedRecord {
            date: r.date.clone(),
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
            version: 2,
            files: BTreeMap::new(),
        }
    }
}

fn load_cache() -> CollectorCache {
    let Ok(text) = fs::read_to_string(paths::collector_cache_json()) else {
        return CollectorCache::default();
    };
    match serde_json::from_str::<CollectorCache>(&text) {
        // Bump on schema change; old caches are rebuilt from source.
        Ok(c) if c.version == 2 => c,
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
        },
    );
}
