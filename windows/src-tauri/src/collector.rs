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
    DailyAgentWork, DailyRhythm, DailyUsage, HourlyTokenBucket, ModelUsage, ProjectUsage, RhythmTag,
    SourceInfo, TokenUsageCounts, ToolUsage, UsageSnapshot, UsageTotals,
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
    /// Sanitized project display name (upstream B1-lite): last path segment
    /// of the working directory, never a full path. `None` = unassigned.
    pub project_name: Option<String>,
    /// Stable per-record identity (e.g. `gemini:<session>:<idx>`) used for
    /// idempotent rescans by the T1 agent sources. Underscore-prefixed
    /// because mainline collectors don't set it yet.
    pub _request_id: String,
}

/// Public entry point: collect from all sources, aggregate, return a snapshot.
/// Mirrors upstream macOS `UsageCollector.collect()` (v0.2.0): Codex + Claude
/// Code + CC Switch records are summed; legacy experimental sources
/// (ZCode / Hermes / WorkBuddy) key off the master switch alone; T1 agent
/// sources (Gemini CLI / Qwen / Kimi / OpenCode / Amp / Droid / Grok) follow
/// the per-source list semantics of `agent_sources::enabled_ids`.
pub fn collect(include_experimental: bool, experimental_source_ids: Option<&Vec<String>>) -> UsageSnapshot {
    let pricing_data = pricing::load();

    // One shared collector cache for the whole pass: each collector only
    // prunes its own entries (single save at the end).
    let mut cache = load_cache();
    let (codex_records, mut codex_source) = collect_codex(&mut cache);
    let (claude_records, mut claude_source) = collect_claude_code(&mut cache);
    save_cache(&cache);
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
        (vec![], SourceInfo { status: Some("disabled".into()), ..Default::default() })
    };
    let (hermes_records, hermes_source) = if include_experimental {
        collect_hermes()
    } else {
        (vec![], SourceInfo { status: Some("disabled".into()), ..Default::default() })
    };
    let workbuddy_source = if include_experimental {
        discover_workbuddy()
    } else {
        SourceInfo { status: Some("disabled".into()), ..Default::default() }
    };
    records.extend(zcode_records.iter().cloned());
    records.extend(hermes_records.iter().cloned());

    // T1 experimental agent sources (upstream G-A1): master switch + explicit
    // per-source list; auto-enroll installed sources when no list was saved.
    let enabled_t1 = crate::agent_sources::enabled_ids(include_experimental, experimental_source_ids);
    let agent_source_results = crate::agent_sources::collect(&enabled_t1);
    for result in agent_source_results.values() {
        records.extend(result.records.iter().cloned());
    }

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
    sources.insert("ZCode".to_string(), zcode_source);
    sources.insert("Hermes Agent".to_string(), hermes_source);
    sources.insert("WorkBuddy".to_string(), workbuddy_source);
    // Merge T1 source infos (each keyed by its display name).
    for (id, result) in agent_source_results {
        sources.insert(id, result.source);
    }

    aggregate(records, &pricing_data, sources)
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

fn collect_codex(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    // Primary: JSONL rollouts (per-turn token counts), matching the Python
    // collector's precedence. SQLite `threads` (per-thread totals) is only a
    // fallback when no JSONL data exists. The cache is shared with the other
    // collectors (single load/save per pass — fixing the historic clobber
    // where each collector's save wiped the others' entries).
    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let (records, files) = collect_codex_jsonl(cache, &mut live_paths);
    let stale: Vec<String> = cache
        .files
        .iter()
        .filter(|(k, entry)| entry.tool == "Codex" && !live_paths.contains(k.as_str()))
        .map(|(k, _)| k.clone())
        .collect();
    for k in stale {
        cache.files.remove(&k);
    }

    if !records.is_empty() {
        return (
            records,
            SourceInfo {
                status: Some("ok".to_string()),
                files: Some(files),
                records: None,
                ..Default::default()
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
                    ..Default::default()
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
            ..Default::default()
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
            timestamp: None, // Codex SQLite threads table has no sub-day resolution
            tool: "Codex".to_string(),
            model: model_key(&sqlite_value_as_string(&model)),
            usage,
            cost_usd: None,
            project_name: None,
            _request_id: String::new(),
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

    // ── Phase 0: per-file fork metadata (own session / parent / created_at) ──
    // session_meta sits near the head of each rollout, so only the first
    // chunk of every file is read here.
    let mut metas: Vec<(String, String, Option<String>, Option<f64>)> =
        Vec::with_capacity(all_paths.len());
    for path in &all_paths {
        let key = path.to_string_lossy().to_string();
        live_paths.insert(key.clone());
        let cached_meta = cache.files.get(&key).filter(|e| {
            let Some((size, mtime)) = file_meta(path) else { return false };
            entry_is_fresh(entry_meta(e), size, mtime)
        });
        if let Some(entry) = cached_meta {
            let session = entry
                .codex_session
                .clone()
                .unwrap_or_else(|| fallback_session_from_path(path));
            metas.push((key, session, entry.codex_parent_session.clone(), entry.codex_created_at));
            continue;
        }
        let (session, parent, created) = codex_head_meta(path);
        metas.push((key, session, parent, created));
    }

    // Parent sessions that at least one live file forks from.
    let parent_ids: BTreeSet<String> = metas
        .iter()
        .filter_map(|(_, _, parent, _)| parent.clone())
        .collect();

    // ── Phase 1: anchors keyed by the file's OWN session id (children look
    // their parent up here). Only files referenced as a parent need anchors.
    let mut anchors_by_session: BTreeMap<String, Vec<CachedAnchor>> = BTreeMap::new();
    for (index, (key, own_session, _, _)) in metas.iter().enumerate() {
        if !parent_ids.contains(own_session) {
            continue;
        }
        let path = &all_paths[index];
        if let Some(entry) = cache.files.get(key) {
            if entry_is_fresh_by_meta(entry, path) {
                anchors_by_session.insert(own_session.clone(), entry.codex_anchors.clone());
                continue;
            }
        }
        // Uncached parent file: extract its anchors by streaming.
        let anchors = codex_scan_anchors(path);
        anchors_by_session.insert(own_session.clone(), anchors);
    }

    // ── Phase 2: per-file scan + delta (fork files inherit the parent anchor) ──
    for (index, path) in all_paths.iter().enumerate() {
        let key = path.to_string_lossy().to_string();
        let (own_session, parent, created_at) = {
            let (_, s, p, c) = &metas[index];
            (s.clone(), p.clone(), *c)
        };

        if let Some(cached) = cached_records(cache, &key, "Codex", path) {
            records.extend(cached);
            // Publish this cached file's anchors for its children.
            if parent_ids.contains(&own_session) {
                if let Some(entry) = cache.files.get(&key) {
                    anchors_by_session.insert(own_session.clone(), entry.codex_anchors.clone());
                }
            }
            continue;
        }

        // ── Token accounting calibration rev8 (upstream v0.1.44 1b54e5c) ──
        // A token_count event carries BOTH a cumulative usage
        // (`info.total_token_usage`, monotonically growing per session) and
        // the last turn's snapshot (`info.last_token_usage`). Summing either
        // directly double-counts O(N^2); the per-event usage is the DELTA
        // between consecutive cumulative values (with sentinel / reset
        // handling). Fork files inherit the parent's anchor so the inherited
        // history is counted exactly once.
        let mut session_id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut final_model = String::from("unknown");
        let mut session_project: Option<String> = None;
        let mut events: Vec<CodexTokenEvent> = Vec::new();

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
                // Upstream B1-lite: remember the session's working directory so
                // every token_count record in this file can carry a project.
                if session_project.is_none() {
                    session_project = payload
                        .and_then(|p| p.get("cwd"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| crate::agent_sources::project_display_name(Some(s)));
                }
            }
            if ev_type == "turn_context" {
                if let Some(m) = payload.and_then(|p| p.get("model")).and_then(|v| v.as_str()) {
                    if !m.is_empty() {
                        final_model = model_key(m);
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
            let cumulative = info
                .and_then(|i| i.get("total_token_usage"))
                .map(|v| normalize_usage(Some(v)));
            let last = info
                .and_then(|i| i.get("last_token_usage"))
                .map(|v| normalize_usage(Some(v)));
            let model_context_window = info
                .and_then(|i| i.get("model_context_window"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            events.push(CodexTokenEvent {
                timestamp: obj
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                model: final_model.clone(),
                cumulative,
                last,
                model_context_window,
            });
        }

        // Resolve the parent anchor: the parent's cumulative value at (or
        // just before) this file's creation time.
        let parent_anchor: Option<TokenUsageCounts> = parent
            .as_ref()
            .and_then(|p| anchors_by_session.get(p))
            .and_then(|anchors| codex_anchor_at_or_before(created_at, anchors));
        let parent_first: Option<TokenUsageCounts> = parent
            .as_ref()
            .and_then(|p| anchors_by_session.get(p))
            .and_then(|anchors| anchors.first())
            .map(|a| a.usage())
            .filter(|u| u.total_tokens > 0);
        let inherited_from = parent_anchor
            .as_ref()
            .map(|a| a.total_tokens)
            .unwrap_or(0);

        let file_records = codex_delta_records(
            &session_id,
            &events,
            session_project.as_deref(),
            parent_anchor.as_ref(),
            parent_first.as_ref(),
            created_at,
            &mut seen,
        );
        records.extend(file_records.clone());

        // Persist with fork metadata + this file's own anchors so future
        // children (and cache hits) can inherit.
        let anchors: Vec<CachedAnchor> = events
            .iter()
            .filter_map(|e| {
                let u = e.cumulative.as_ref()?;
                if u.total_tokens <= 0 {
                    return None;
                }
                let ts = e
                    .timestamp
                    .as_deref()
                    .and_then(parse_iso)
                    .map(|dt| dt.timestamp() as f64)?;
                Some(CachedAnchor::from_usage(ts, u))
            })
            .collect();
        update_codex_cache(
            cache,
            &key,
            path,
            &file_records,
            &session_id,
            parent,
            created_at,
            anchors.clone(),
        );
        // Publish anchors for children of this session.
        if parent_ids.contains(&session_id) {
            anchors_by_session.insert(session_id.clone(), anchors);
        }
        let _ = inherited_from;
    }

    (records, all_paths.len() as i64)
}

/// Fallback session key for cache entries saved before `codex_session`
/// existed: the rollout file stem.
fn fallback_session_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Read a rollout's head to extract (session id, parent session, created_at).
/// session_meta is the first relevant line in practice; cap the hunt at 256 KB.
fn codex_head_meta(path: &Path) -> (String, Option<String>, Option<f64>) {
    use std::io::Read;
    let session = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let Ok(mut file) = fs::File::open(path) else {
        return (session, None, None);
    };
    let mut buf = Vec::with_capacity(256 * 1024);
    let mut chunk = [0u8; 64 * 1024];
    while buf.len() < 256 * 1024 {
        match file.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text_head = String::from_utf8_lossy(&buf);
    for line in text_head.lines() {
        if !line.contains("session_meta") {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        let payload = obj.get("payload");
        let id = payload
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or(session);
        let parent = codex_parent_session_id(payload);
        let created = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .or_else(|| payload.and_then(|p| p.get("timestamp")).and_then(|v| v.as_str()))
            .and_then(parse_iso)
            .map(|dt| dt.timestamp() as f64);
        return (id, parent, created);
    }
    (session, None, None)
}

/// Upstream `codexParentSessionID`: source.subagent.thread_spawn.parent_thread_id,
/// then payload.parent_thread_id / forked_from_id.
fn codex_parent_session_id(payload: Option<&serde_json::Value>) -> Option<String> {
    let payload = payload?;
    if let Some(parent) = payload
        .get("source")
        .and_then(|s| s.get("subagent"))
        .and_then(|s| s.get("thread_spawn"))
        .and_then(|t| t.get("parent_thread_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        return Some(parent.trim().to_string());
    }
    for key in ["parent_thread_id", "forked_from_id"] {
        if let Some(parent) = payload
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            return Some(parent.trim().to_string());
        }
    }
    None
}

/// Stream a file and collect its cumulative anchors (ts + usage), skipping
/// consecutive duplicates to keep the list compact.
fn codex_scan_anchors(path: &Path) -> Vec<CachedAnchor> {
    use std::io::BufRead;
    let mut anchors: Vec<CachedAnchor> = Vec::new();
    let Ok(file) = fs::File::open(path) else { return anchors };
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("token_count") {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if obj.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(info) = obj
            .get("payload")
            .and_then(|p| p.get("info"))
        else {
            continue;
        };
        let Some(cumulative) = info.get("total_token_usage") else { continue };
        let usage = normalize_usage(Some(cumulative));
        if usage.total_tokens <= 0 {
            continue;
        }
        let Some(ts) = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_iso)
            .map(|dt| dt.timestamp() as f64)
        else {
            continue;
        };
        // Skip consecutive duplicates (same total).
        if anchors.last().map(|a| a.total) == Some(usage.total_tokens) {
            continue;
        }
        anchors.push(CachedAnchor::from_usage(ts, &usage));
    }
    anchors
}

/// Upstream `codexAnchor(atOrBefore:)`: binary search for the newest anchor
/// whose timestamp is <= the child's creation time.
fn codex_anchor_at_or_before(created_at: Option<f64>, anchors: &[CachedAnchor]) -> Option<TokenUsageCounts> {
    let created = created_at?;
    let idx = anchors.partition_point(|a| a.ts <= created);
    if idx == 0 {
        return None;
    }
    let anchor = &anchors[idx - 1];
    if anchor.total <= 0 {
        return None;
    }
    Some(anchor.usage())
}

/// Freshness check for a cache entry against live file metadata.
fn entry_is_fresh(entry: (u64, f64), size: u64, mtime: f64) -> bool {
    entry.0 == size && (entry.1 - mtime).abs() < 0.001
}

fn entry_meta(entry: &CachedUsageFile) -> (u64, f64) {
    (entry.size, entry.mtime)
}

fn entry_is_fresh_by_meta(entry: &CachedUsageFile, path: &Path) -> bool {
    let Some((size, mtime)) = file_meta(path) else { return false };
    entry_is_fresh(entry_meta(entry), size, mtime)
}

/// Codex-specific cache write carrying fork metadata + anchors.
fn update_codex_cache(
    cache: &mut CollectorCache,
    key: &str,
    path: &Path,
    records: &[UsageRecord],
    session_id: &str,
    parent: Option<String>,
    created_at: Option<f64>,
    anchors: Vec<CachedAnchor>,
) {
    let Some((size, mtime)) = file_meta(path) else {
        return;
    };
    cache.files.insert(
        key.to_string(),
        CachedUsageFile {
            tool: "Codex".to_string(),
            size,
            mtime,
            records: records.iter().map(CachedRecord::from).collect(),
            codex_session: Some(session_id.to_string()),
            codex_parent_session: parent,
            codex_created_at: created_at,
            codex_anchors: anchors,
        },
    );
}

// ---------------------------------------------------------------------------
// Codex token accounting calibration rev8 (port of upstream v0.1.44/v0.2.0
// scanCodexSessionFile + codexDeltaRecords + codexIncrementUsage)
// ---------------------------------------------------------------------------

/// One `token_count` event from a Codex rollout, holding both usage flavors.
#[derive(Debug, Clone)]
struct CodexTokenEvent {
    timestamp: Option<String>,
    model: String,
    /// `info.total_token_usage` — session-cumulative (monotonic per epoch).
    cumulative: Option<TokenUsageCounts>,
    /// `info.last_token_usage` — last turn's snapshot.
    last: Option<TokenUsageCounts>,
    /// `info.model_context_window` — used to recognize sentinel values.
    model_context_window: i64,
}

impl CodexTokenEvent {
    fn cumulative_present(&self) -> bool {
        self.cumulative.is_some()
    }
}

/// A first-event cumulative above this (with a valid per-turn snapshot) is
/// treated as an inherited counter from a resume chain, not fresh usage.
const INHERITED_COUNTER_FLOOR_TOKENS: i64 = 10_000_000;

/// Upstream `isCodexBreakdownConsistent`: all parts non-negative, input+output
/// equals total, cache contained in input, reasoning contained in output.
fn codex_breakdown_consistent(u: &TokenUsageCounts, total: i64) -> bool {
    u.input_tokens >= 0
        && u.output_tokens >= 0
        && u.cache_creation_input_tokens >= 0
        && u.cache_read_input_tokens >= 0
        && u.reasoning_output_tokens >= 0
        && u.input_tokens + u.output_tokens == total
        && u.cache_creation_input_tokens + u.cache_read_input_tokens <= u.input_tokens
        && u.reasoning_output_tokens <= u.output_tokens
}

/// Upstream `isCodexContextWindowSentinel`: an all-zero cumulative whose total
/// equals the model context window is a synthetic marker, not usage.
fn codex_is_context_window_sentinel(event: &CodexTokenEvent) -> bool {
    let Some(c) = event.cumulative.as_ref() else { return false };
    c.input_tokens == 0
        && c.output_tokens == 0
        && c.cache_creation_input_tokens == 0
        && c.cache_read_input_tokens == 0
        && c.reasoning_output_tokens == 0
        && event.last.as_ref().map(|l| l.total_tokens).unwrap_or(0) == 0
        && event.model_context_window > 0
        && c.total_tokens == event.model_context_window
}

/// Upstream `isCredibleCodexReset`: a drop in the cumulative total counts as a
/// counter reset only if the `last` snapshot matches the new value, or a later
/// event climbs from it while staying below the previous maximum.
fn codex_is_credible_reset(
    index: usize,
    events: &[CodexTokenEvent],
    current: &TokenUsageCounts,
    previous: &TokenUsageCounts,
) -> bool {
    if let Some(last) = events[index].last.as_ref() {
        if last.total_tokens == current.total_tokens
            && codex_breakdown_consistent(last, current.total_tokens)
        {
            return true;
        }
    }
    for candidate in events.iter().skip(index + 1) {
        let Some(next) = candidate.cumulative.as_ref() else { continue };
        if next.total_tokens <= 0 {
            continue;
        }
        if next.total_tokens == current.total_tokens {
            continue;
        }
        return next.total_tokens > current.total_tokens && next.total_tokens < previous.total_tokens;
    }
    false
}

/// Upstream `codexIncrementUsage`: distribute a delta total across fields.
/// Prefers the `last` snapshot when it exactly matches the delta; else the
/// per-field difference (when monotonic); else a total-only record.
fn codex_increment_usage(
    current: &TokenUsageCounts,
    previous: Option<&TokenUsageCounts>,
    last: Option<&TokenUsageCounts>,
    total: i64,
) -> TokenUsageCounts {
    if let Some(last) = last {
        if last.total_tokens == total && codex_breakdown_consistent(last, total) {
            let mut result = last.clone();
            result.total_tokens = total;
            return result;
        }
    }
    let empty = TokenUsageCounts::default();
    let prev = previous.unwrap_or(&empty);
    let monotonic = current.input_tokens >= prev.input_tokens
        && current.output_tokens >= prev.output_tokens
        && current.cache_creation_input_tokens >= prev.cache_creation_input_tokens
        && current.cache_read_input_tokens >= prev.cache_read_input_tokens
        && current.reasoning_output_tokens >= prev.reasoning_output_tokens;
    if monotonic {
        let mut result = TokenUsageCounts {
            input_tokens: current.input_tokens - prev.input_tokens,
            output_tokens: current.output_tokens - prev.output_tokens,
            cache_creation_input_tokens: current.cache_creation_input_tokens
                - prev.cache_creation_input_tokens,
            cache_read_input_tokens: current.cache_read_input_tokens - prev.cache_read_input_tokens,
            reasoning_output_tokens: current.reasoning_output_tokens
                - prev.reasoning_output_tokens,
            total_tokens: total,
        };
        if codex_breakdown_consistent(&result, total) {
            return result;
        }
        result = TokenUsageCounts::default();
        result.total_tokens = total;
        return result;
    }
    let mut result = TokenUsageCounts::default();
    result.total_tokens = total;
    result
}

/// Upstream `codexDeltaRecords`: turn one file's token_count events into
/// per-event delta records. Files without the cumulative schema fall back to
/// the legacy per-event `last` estimate (dedup'd). `seen` is the cross-file
/// request-id set, preserved between files of the same collection pass.
fn codex_delta_records(
    session_id: &str,
    events: &[CodexTokenEvent],
    project_name: Option<&str>,
    parent_anchor: Option<&TokenUsageCounts>,
    parent_first_cumulative: Option<&TokenUsageCounts>,
    child_created_at: Option<f64>,
    seen: &mut std::collections::BTreeSet<String>,
) -> Vec<UsageRecord> {
    let mut records: Vec<UsageRecord> = Vec::new();
    let has_cumulative_schema = events.iter().any(|e| e.cumulative_present());

    if !has_cumulative_schema {
        // Legacy schema: only `last_token_usage` exists — use it directly
        // (upstream "codex_last_usage_legacy_estimate").
        for event in events {
            let Some(usage) = event.last.as_ref() else { continue };
            if usage.total_tokens <= 0 {
                continue;
            }
            let Some(timestamp) = event.timestamp.as_ref() else { continue };
            let Some(day) = day_string_from_iso(timestamp) else { continue };
            let fingerprint = format!(
                "{}:{}:{}:{}:{}:{}",
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
                usage.reasoning_output_tokens,
                usage.total_tokens
            );
            let request_id = format!("codex:legacy:{session_id}:{timestamp}:{fingerprint}");
            if !seen.insert(request_id) {
                continue;
            }
            let mut usage = usage.clone();
            usage.finalize_total();
            records.push(UsageRecord {
                date: day,
                timestamp: Some(timestamp.clone()),
                tool: "Codex".to_string(),
                model: event.model.clone(),
                usage,
                cost_usd: None,
                project_name: project_name.map(String::from),
                _request_id: String::new(),
            });
        }
        return records;
    }

    // Cumulative schema: per-event usage = delta between consecutive totals.
    // A fork file inherits the parent's anchor: events up to (and including)
    // the matching cumulative value are inherited history counted by the
    // parent, so delta-ing resumes from there (upstream codexForkAnchor).
    let mut previous: Option<TokenUsageCounts> = None;
    let mut start_index = 0usize;
    if let Some(anchor) = parent_anchor {
        if anchor.total_tokens > 0 {
            if let Some(anchor_index) = events
                .iter()
                .position(|e| matches!(e.cumulative.as_ref(), Some(c) if c == anchor))
            {
                previous = Some(anchor.clone());
                start_index = anchor_index + 1;
            }
        }
    }
    // Dump fallback: a fork file whose FIRST cumulative equals the parent's
    // first cumulative field-for-field is a transcript dump (compact/resume
    // writes a fresh rollout copying the parent's whole event history; the
    // exact anchor no longer matches because the parent kept growing after
    // the dump). Copied events all carry timestamps <= the dump moment, so
    // delta-ing resumes strictly after `child_created_at`.
    if start_index == 0 {
        if let (Some(parent_first), Some(created)) = (parent_first_cumulative, child_created_at) {
            let is_dump = parent_first.total_tokens > 0
                && matches!(events.first().and_then(|e| e.cumulative.as_ref()),
                            Some(c) if c == parent_first);
            if is_dump {
                let mut last_before: Option<&TokenUsageCounts> = None;
                let mut split = events.len();
                for (index, event) in events.iter().enumerate() {
                    let Some(ts) = event
                        .timestamp
                        .as_deref()
                        .and_then(parse_iso)
                        .map(|dt| dt.timestamp() as f64)
                    else {
                        continue;
                    };
                    if ts > created {
                        split = index;
                        break;
                    }
                    if let Some(c) = event.cumulative.as_ref() {
                        last_before = Some(c);
                    }
                }
                previous = last_before.cloned().or_else(|| parent_anchor.cloned());
                start_index = split;
            }
        }
    }
    let mut epoch = 0usize;
    for (index, event) in events.iter().enumerate().skip(start_index) {
        if !event.cumulative_present() {
            continue;
        }
        let Some(current) = event.cumulative.as_ref() else { continue };
        if current.total_tokens <= 0 {
            continue;
        }
        let Some(timestamp) = event.timestamp.as_ref() else { continue };
        let Some(day) = day_string_from_iso(timestamp) else { continue };

        let (delta_total, is_reset): (i64, bool);
        match previous.as_ref() {
            Some(prev) => {
                if current.total_tokens == prev.total_tokens {
                    continue; // duplicate snapshot
                } else if current.total_tokens > prev.total_tokens {
                    delta_total = current.total_tokens - prev.total_tokens;
                    is_reset = false;
                } else if codex_is_context_window_sentinel(event) {
                    continue; // synthetic context-window marker
                } else if codex_is_credible_reset(index, events, current, prev) {
                    epoch += 1;
                    delta_total = current.total_tokens;
                    is_reset = true;
                } else {
                    continue; // non-credible drop — skip rather than guess
                }
            }
            None => {
                // Inherited-counter guard: a first cumulative in the tens of
                // millions+ with a valid per-turn snapshot is a resumed
                // counter (resume chains point parent_thread_id at the
                // ORIGINAL session, whose anchors long stopped growing, so
                // fork anchoring can't reach it). Count only this turn's
                // real usage instead of re-counting the whole history.
                let last_total = event.last.as_ref().map(|l| l.total_tokens).unwrap_or(0);
                if current.total_tokens > INHERITED_COUNTER_FLOOR_TOKENS && last_total > 0 {
                    delta_total = last_total;
                } else {
                    delta_total = current.total_tokens;
                }
                is_reset = false;
            }
        }
        if delta_total <= 0 {
            continue;
        }
        let usage = codex_increment_usage(
            current,
            if is_reset { None } else { previous.as_ref() },
            event.last.as_ref(),
            delta_total,
        );
        let request_id =
            format!("codex:cumulative:{session_id}:{epoch}:{}", current.total_tokens);
        if !seen.insert(request_id) {
            previous = Some(current.clone());
            continue;
        }
        records.push(UsageRecord {
            date: day,
            timestamp: Some(timestamp.clone()),
            tool: "Codex".to_string(),
            model: event.model.clone(),
            usage,
            cost_usd: None,
            project_name: project_name.map(String::from),
            _request_id: String::new(),
        });
        previous = Some(current.clone());
    }
    records
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

/// Build the dedupe key for a Claude Code assistant row (port of upstream
/// v0.1.32 `claudeResponseKey`). Prefers the message's response id, then the
/// request id, then the transcript row's uuid, falling back to the file+line.
fn claude_response_key(
    obj: &serde_json::Value,
    message: &serde_json::Value,
    path_key: &str,
    line_no: usize,
) -> String {
    for value in [
        message.get("id").and_then(|v| v.as_str()),
        obj.get("requestId").and_then(|v| v.as_str()),
        obj.get("request_id").and_then(|v| v.as_str()),
    ] {
        if let Some(s) = value {
            let t = s.trim();
            if !t.is_empty() {
                return format!("response:{}", t);
            }
        }
    }
    if let Some(uuid) = obj.get("uuid").and_then(|v| v.as_str()) {
        let t = uuid.trim();
        if !t.is_empty() {
            return format!("uuid:{}", t);
        }
    }
    format!("line:{}:{}", path_key, line_no)
}

/// One candidate row for a Claude Code response, used to pick the preferred
/// row when the same response appears multiple times (port of upstream
/// v0.1.32 `ClaudeUsageCandidate`).
struct ClaudeCandidate {
    day: String,
    timestamp: String,
    model: String,
    usage: TokenUsageCounts,
    has_stop_reason: bool,
    line_no: usize,
    /// Sanitized project from the row's top-level `cwd` (upstream B1-lite).
    project_name: Option<String>,
}

impl ClaudeCandidate {
    /// Whether this candidate should replace the existing one for the same
    /// response key. Prefers a row with a stop_reason; ties broken by newer
    /// timestamp, then later line number.
    fn is_preferred_over(&self, other: &ClaudeCandidate) -> bool {
        if self.has_stop_reason != other.has_stop_reason {
            return self.has_stop_reason;
        }
        if self.timestamp != other.timestamp {
            return self.timestamp > other.timestamp;
        }
        self.line_no > other.line_no
    }
}

fn collect_claude_code(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    let root = paths::claude_projects_root();
    let mut all_paths = jsonl_files_under(&root);
    all_paths.sort();

    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let mut records: Vec<UsageRecord> = Vec::new();

    for path in &all_paths {
        let key = path.to_string_lossy().to_string();
        live_paths.insert(key.clone());

        if let Some(cached) = cached_records(&cache, &key, "Claude Code", path) {
            records.extend(cached);
            continue;
        }

        // Per-file response map: dedupe Claude Code assistant rows by their
        // response identity (port of upstream v0.1.32 ce44b6f). A single
        // assistant response can appear multiple times in a transcript (e.g.
        // with and without a stop_reason, or re-emitted on resume); the older
        // uuid-only dedupe kept the FIRST one, which could be the partial
        // pre-stop row. We now keep the preferred candidate: prefer a row
        // WITH stop_reason, then the newer timestamp, then the later line.
        let file_records: Vec<UsageRecord>;
        let mut responses: BTreeMap<String, ClaudeCandidate> = BTreeMap::new();
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
            let response_key = claude_response_key(&obj, message, &key, line_no);
            let has_stop_reason = message
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let model = model_key(
                message
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            );
            let candidate = ClaudeCandidate {
                day,
                timestamp: timestamp.to_string(),
                model,
                usage: usage.clone(),
                has_stop_reason,
                line_no,
                project_name: obj
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .and_then(|s| crate::agent_sources::project_display_name(Some(s))),
            };
            if let Some(existing) = responses.get(&response_key) {
                if !candidate.is_preferred_over(existing) {
                    continue;
                }
            }
            responses.insert(response_key, candidate);
        }
        file_records = responses
            .values()
            .map(|c| UsageRecord {
                date: c.day.clone(),
                timestamp: Some(c.timestamp.clone()),
                tool: "Claude Code".to_string(),
                model: c.model.clone(),
                usage: c.usage.clone(),
                cost_usd: None,
                project_name: c.project_name.clone(),
                _request_id: String::new(),
            })
            .collect();

        records.extend(file_records.clone());
        update_cache(cache, &key, "Claude Code", path, &file_records);
    }

    let stale: Vec<String> = cache
        .files
        .iter()
        .filter(|(k, entry)| entry.tool == "Claude Code" && !live_paths.contains(k.as_str()))
        .map(|(k, _)| k.clone())
        .collect();
    for k in stale {
        cache.files.remove(&k);
    }

    let status = if records.is_empty() { "missing" } else { "ok" };
    (
        records,
        SourceInfo {
            status: Some(status.to_string()),
            files: Some(all_paths.len() as i64),
            records: None,
            ..Default::default()
        },
    )
}

// ---------------------------------------------------------------------------
// CC Switch proxy (SQLite) — port of upstream `collectCCSwitchProxyUsage`.
// ---------------------------------------------------------------------------

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

fn cc_switch_epoch_day(v: &rusqlite::types::Value) -> Option<String> {
    use rusqlite::types::Value;
    let raw: f64 = match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) => *r,
        Value::Text(s) => s.parse().ok()?,
        Value::Null => return None,
        Value::Blob(b) => String::from_utf8_lossy(b).parse().ok()?,
    };
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
                ..Default::default()
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
                    ..Default::default()
                },
            )
        }
    };

    // data_source is intentionally NOT required (older DBs lack it; upstream
    // v0.1.32 treats it as optional).
    let required: BTreeSet<&str> = [
        "request_id", "app_type", "provider_id", "model", "request_model",
        "pricing_model", "input_tokens", "output_tokens", "cache_read_tokens",
        "cache_creation_tokens", "total_cost_usd", "status_code", "created_at",
    ]
    .into_iter()
    .collect();

    let available: BTreeSet<String> = match conn.prepare("pragma table_info(proxy_request_logs)") {
        Ok(mut s) => match s.query_map([], |row| row.get::<_, String>(1)) {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => return missing("schema_unreadable"),
        },
        Err(_) => return missing("schema_unreadable"),
    };
    if available.is_empty() {
        return missing("missing_table");
    }
    if !required.iter().all(|r| available.contains(*r)) {
        return missing("schema_mismatch");
    }

    // data_source filter: only apply when the column exists, treat empty as
    // 'proxy' (matches upstream v0.1.32).
    let data_source_filter = if available.contains("data_source") {
        "and coalesce(nullif(data_source, ''), 'proxy') = 'proxy'"
    } else {
        ""
    };
    let sql = format!(
        "select created_at, app_type, \
        coalesce(nullif(pricing_model, ''), nullif(model, ''), nullif(request_model, ''), 'unknown') as display_model, \
        coalesce(input_tokens, 0) as input_tokens, \
        coalesce(output_tokens, 0) as output_tokens, \
        coalesce(cache_read_tokens, 0) as cache_read_tokens, \
        coalesce(cache_creation_tokens, 0) as cache_creation_tokens, \
        cast(coalesce(nullif(total_cost_usd, ''), '0') as real) as total_cost_usd \
        from proxy_request_logs \
        where status_code >= 200 and status_code < 300 {} \
        and (coalesce(input_tokens, 0) + coalesce(output_tokens, 0) \
             + coalesce(cache_read_tokens, 0) + coalesce(cache_creation_tokens, 0)) > 0 \
        order by created_at, request_id",
        data_source_filter
    );

    let row_data: Vec<(
        rusqlite::types::Value, rusqlite::types::Value, rusqlite::types::Value,
        rusqlite::types::Value, rusqlite::types::Value, rusqlite::types::Value,
        rusqlite::types::Value, rusqlite::types::Value,
    )> = match conn.prepare(&sql) {
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
            Err(_) => return missing("query_failed"),
        },
        Err(_) => return missing("query_failed"),
    };

    let mut records = Vec::new();
    for (created_at, app_type, display_model, input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, total_cost_usd) in row_data {
        let day = match cc_switch_epoch_day(&created_at) { Some(d) => d, None => continue };
        let input = sqlite_value_as_f64(&input_tokens) as i64;
        let output = sqlite_value_as_f64(&output_tokens) as i64;
        let cache_read = sqlite_value_as_f64(&cache_read_tokens) as i64;
        let cache_creation = sqlite_value_as_f64(&cache_creation_tokens) as i64;
        let total = input + output + cache_read + cache_creation;
        if total <= 0 { continue; }
        let cost_usd = {
            let raw = sqlite_value_as_f64(&total_cost_usd);
            if raw.is_finite() && raw > 0.0 { Some(raw) } else { None }
        };
        records.push(UsageRecord {
            date: day,
            timestamp: iso_from_epoch(&created_at),
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
            project_name: None,
            _request_id: String::new(),
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
            ..Default::default()
        },
    )
}

// Aggregation
// ---------------------------------------------------------------------------

fn aggregate(
    records: Vec<UsageRecord>,
    pricing_data: &pricing::PricingFile,
    sources: BTreeMap<String, SourceInfo>,
) -> UsageSnapshot {
    let mut daily: BTreeMap<String, DailyAccumulator> = BTreeMap::new();
    let mut rhythms: BTreeMap<String, RhythmAccumulator> = BTreeMap::new();
    let mut tools: BTreeMap<String, UsageAccumulator> = BTreeMap::new();
    let mut models: BTreeMap<(String, String), UsageAccumulator> = BTreeMap::new();
    // Project-dimension totals (upstream B1-lite `ProjectUsageAccumulator`);
    // "" groups records without a project context.
    let mut project_totals: BTreeMap<String, ProjectUsageAccumulator> = BTreeMap::new();

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

        // Project dimension: fold into both the daily and global accumulators.
        let record_project = record.project_name.clone().unwrap_or_default();
        let daily_project = daily_entry
            .projects
            .entry(record_project.clone())
            .or_default();
        daily_project.tokens += record.usage.total_tokens;
        daily_project.cost += cost;
        *daily_project.tools.entry(record.tool.clone()).or_insert(0) += record.usage.total_tokens;
        *daily_project.models.entry(record.model.clone()).or_insert(0) += record.usage.total_tokens;
        let global_entry = project_totals.entry(record_project).or_default();
        global_entry.tokens += record.usage.total_tokens;
        global_entry.cost += cost;
        *global_entry.tools.entry(record.tool.clone()).or_insert(0) += record.usage.total_tokens;
        *global_entry.models.entry(record.model.clone()).or_insert(0) += record.usage.total_tokens;

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
        .map(|d| {
            let mut projects: Vec<ProjectUsage> = d
                .projects
                .into_iter()
                .map(|(name, acc)| acc.into_project_usage(name))
                .collect();
            projects.sort_by(|a, b| b.tokens.cmp(&a.tokens));
            DailyUsage {
                date: d.date,
                tools: d.tools,
                models: d.models,
                total_tokens: d.total_tokens,
                cost: round(d.cost, 4),
                projects: Some(projects),
            }
        })
        .collect();
    daily_rows.sort_by(|a, b| a.date.cmp(&b.date));

    let mut rhythm_rows: Vec<DailyRhythm> = rhythms
        .into_values()
        .map(|r| r.into_daily_rhythm())
        .filter(|r| r.total_tokens > 0)
        .collect();
    rhythm_rows.sort_by(|a, b| a.date.cmp(&b.date));

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

    // Global project aggregates, tokens-descending (upstream B1-lite).
    let mut project_rows: Vec<ProjectUsage> = project_totals
        .into_iter()
        .map(|(name, acc)| acc.into_project_usage(name))
        .collect();
    project_rows.sort_by(|a, b| b.tokens.cmp(&a.tokens));

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
        source_attempt: None,
        projects: project_rows,
    }
}

#[derive(Default)]
struct DailyAccumulator {
    date: String,
    tools: BTreeMap<String, i64>,
    /// Per-model token breakdown for this day (mirrors upstream
    /// DailyAccumulator.models). Drives the Today view's model split.
    models: BTreeMap<String, i64>,
    /// Per-project accumulators keyed by sanitized name ("" = unassigned).
    projects: BTreeMap<String, ProjectUsageAccumulator>,
    total_tokens: i64,
    cost: f64,
}

/// Port of upstream `ProjectUsageAccumulator`: sums tokens/cost and
/// per-tool / per-model breakdowns for one project on one day (or globally).
#[derive(Default, Clone)]
struct ProjectUsageAccumulator {
    tokens: i64,
    cost: f64,
    tools: BTreeMap<String, i64>,
    models: BTreeMap<String, i64>,
}

impl ProjectUsageAccumulator {
    /// Round cost to 4 decimals (upstream `(cost * 10_000).rounded() / 10_000`)
    /// and drop tools/models that carry no tokens.
    fn into_project_usage(self, name: String) -> ProjectUsage {
        ProjectUsage {
            name,
            tokens: self.tokens,
            cost: (self.cost * 10_000.0).round() / 10_000.0,
            tools: self.tools.into_iter().filter(|(_, v)| *v > 0).collect(),
            models: self.models.into_iter().filter(|(_, v)| *v > 0).collect(),
        }
    }
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
            if peak_hour >= 0 { Some(peak_hour) } else { None },
            peak_tokens,
            active_hours,
            first_active_hour,
        );

        DailyRhythm {
            date: self.date,
            buckets,
            total_tokens,
            peak_hour: if peak_hour >= 0 { Some(peak_hour) } else { None },
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
        let night_share = share(tokens_in_hours(&[21, 22, 23, 0, 1, 2], significant), total_tokens);
        if night_share >= 0.35 || (peak_hour.map(|h| h >= 21 || h <= 2).unwrap_or(false) && night_share >= 0.25) {
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
    (start..=end).filter_map(|h| significant.get(h as usize)).sum()
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
const COLLECTOR_CACHE_VERSION: i32 = 10;

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
    let secs: i64 = if raw > 1e12 { (raw / 1000.0) as i64 } else { raw as i64 };
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
    #[serde(default, rename = "codex_session", skip_serializing_if = "Option::is_none")]
    codex_session: Option<String>,
    #[serde(default, rename = "codex_parent_session", skip_serializing_if = "Option::is_none")]
    codex_parent_session: Option<String>,
    #[serde(default, rename = "codex_created_at", skip_serializing_if = "Option::is_none")]
    codex_created_at: Option<f64>,
    #[serde(default)]
    codex_anchors: Vec<CachedAnchor>,
}

/// One cumulative anchor point: (epoch secs, usage snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Project dimension (v0.2.0 B1-lite); older caches decode as None.
    #[serde(default)]
    project_name: Option<String>,
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
            project_name: r.project_name.clone(),
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
    let Ok(text) = fs::read_to_string(paths::collector_cache_json()) else {
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
                timestamp: c.timestamp.clone(),
                tool: c.tool.clone(),
                model: c.model.clone(),
                usage: c.usage.clone(),
                cost_usd: c.cost_usd,
                project_name: c.project_name.clone(),
                _request_id: String::new(),
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
        },
    );
}

// ---------------------------------------------------------------------------
// Experimental agent sources (port of upstream v0.1.44)
// ---------------------------------------------------------------------------

/// Read ZCode agent usage from `~/.zcode/cli/db/db.sqlite`.
/// ZCode's `input_tokens` already includes cached tokens.
///
/// v0.2.0 (upstream 2826aac): the SQL is now *constructed conditionally* —
/// `provider_total_tokens` may be absent on older schemas, and the optional
/// `session` table (when present) is joined to pull `session.directory` as
/// the project dimension.
fn collect_zcode() -> (Vec<UsageRecord>, SourceInfo) {
    let db_path = paths::zcode_db_path();
    if !db_path.exists() {
        return (vec![], SourceInfo {
            status: Some("missing_db".into()),
            ..Default::default()
        });
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return (vec![], SourceInfo {
            status: Some("unreadable_db".into()),
            ..Default::default()
        }),
    };
    // Verify schema: table model_usage must exist with key columns.
    let has_table = conn
        .prepare("SELECT count(*) as n FROM sqlite_master WHERE type='table' AND name='model_usage'")
        .and_then(|mut s| s.query_row([], |r| r.get::<_, i64>(0)))
        .map(|n| n > 0)
        .unwrap_or(false);
    if !has_table {
        return (vec![], SourceInfo {
            status: Some("missing_table".into()),
            ..Default::default()
        });
    }
    // Probe required columns via pragma table_info (upstream v0.2.0).
    let available_columns: BTreeSet<String> = conn
        .prepare("pragma table_info(model_usage)")
        .and_then(|mut s| {
            let mut cols = BTreeSet::new();
            let mut rows = s.query([])?;
            while let Some(row) = rows.next()? {
                if let Ok(name) = row.get::<_, String>(1) {
                    cols.insert(name);
                }
            }
            Ok(cols)
        })
        .unwrap_or_default();
    let required_cols = [
        "id", "session_id", "status", "started_at", "model_id", "input_tokens",
        "output_tokens", "reasoning_tokens", "cache_creation_input_tokens",
        "cache_read_input_tokens", "computed_total_tokens", "tool_call_count",
    ];
    if required_cols.iter().any(|c| !available_columns.contains(*c)) {
        return (vec![], SourceInfo {
            status: Some("schema_mismatch".into()),
            ..Default::default()
        });
    }
    // Optional pieces, decided BEFORE拼接 (the v0.2.0 fix): provider totals
    // column and the session join for project_directory.
    let provider_total_expr = if available_columns.contains("provider_total_tokens") {
        "coalesce(model_usage.provider_total_tokens, 0)"
    } else {
        "0"
    };
    let has_session_table = conn
        .prepare("SELECT count(*) as n FROM sqlite_master WHERE type='table' AND name='session'")
        .and_then(|mut s| s.query_row([], |r| r.get::<_, i64>(0)))
        .map(|n| n > 0)
        .unwrap_or(false);
    let project_column = if has_session_table {
        ",\n    coalesce(session.directory, '') as project_directory"
    } else {
        ""
    };
    let session_join = if has_session_table {
        "left join session on session.id = model_usage.session_id\n"
    } else {
        ""
    };
    let sql = format!(
        r#"select
    model_usage.started_at,
    coalesce(nullif(model_usage.model_id, ''), 'unknown') as display_model,
    coalesce(model_usage.input_tokens, 0) as input_tokens,
    coalesce(model_usage.output_tokens, 0) as output_tokens,
    coalesce(model_usage.reasoning_tokens, 0) as reasoning_tokens,
    coalesce(model_usage.cache_creation_input_tokens, 0) as cache_creation_input_tokens,
    coalesce(model_usage.cache_read_input_tokens, 0) as cache_read_input_tokens,
    coalesce(model_usage.computed_total_tokens, 0) as computed_total_tokens,
    {provider_total_expr} as provider_total_tokens,
    coalesce(model_usage.tool_call_count, 0) as tool_call_count{project_column}
from model_usage
{session_join}where model_usage.status = 'completed'
    and (
        coalesce(model_usage.computed_total_tokens, 0) > 0
        or {provider_total_expr} > 0
        or (
            coalesce(model_usage.input_tokens, 0)
            + coalesce(model_usage.output_tokens, 0)
            + coalesce(model_usage.reasoning_tokens, 0)
            + coalesce(model_usage.cache_creation_input_tokens, 0)
            + coalesce(model_usage.cache_read_input_tokens, 0)
        ) > 0
    )
order by model_usage.started_at, model_usage.id"#
    );
    let mut stmt = match conn.prepare(sql.as_str()) {
        Ok(s) => s,
        Err(_) => return (vec![], SourceInfo {
            status: Some("schema_mismatch".into()),
            ..Default::default()
        }),
    };
    let rows = stmt.query_map([], |row| {
        let started_at: f64 = row.get(0)?;
        let model: String = row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "unknown".into());
        let input: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        let output: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
        let reasoning: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
        let cache_create: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
        let cache_read: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
        let computed_total: i64 = row.get::<_, Option<i64>>(7)?.unwrap_or(0);
        let provider_total: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(0);
        // tool_call_count (column 9) is read for forward compatibility with
        // the upstream agent-work accumulator; not consumed here.
        let project_directory: Option<String> = if has_session_table {
            row.get::<_, Option<String>>(10)?
        } else {
            None
        };
        // canonicalUsageCounts with inputIncludesCachedTokens=true: raw input
        // already contains cached tokens — do NOT add cache again (v0.2.0 fix).
        let usage = crate::agent_sources::canonical_counts(
            input,
            output,
            cache_read,
            cache_create,
            reasoning,
            true,
            Some(if computed_total > 0 { computed_total } else { provider_total }),
        );
        let date = day_string_from_epoch(started_at);
        let ts = iso_string_from_epoch(started_at);
        Ok(UsageRecord {
            date,
            timestamp: Some(ts),
            tool: "ZCode".to_string(),
            model,
            usage,
            cost_usd: None,
            project_name: crate::agent_sources::project_display_name(
                project_directory.as_deref(),
            ),
            _request_id: String::new(),
        })
    });
    let mut records = Vec::new();
    if let Ok(iter) = rows {
        for r in iter.flatten() {
            records.push(r);
        }
    }
    let status = if records.is_empty() { "missing_valid_rows" } else { "ok" };
    let count = records.len() as i64;
    (records, SourceInfo {
        status: Some(status.into()),
        files: Some(1),
        records: Some(count),
        ..Default::default()
    })
}

/// Read Hermes agent usage from `~/.hermes/state.db`.
/// Hermes has cache tokens SEPARATE from input (inputIncludesCachedTokens=false).
fn collect_hermes() -> (Vec<UsageRecord>, SourceInfo) {
    let db_path = paths::hermes_db_path();
    if !db_path.exists() {
        return (vec![], SourceInfo {
            status: Some("missing_db".into()),
            ..Default::default()
        });
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return (vec![], SourceInfo {
            status: Some("unreadable_db".into()),
            ..Default::default()
        }),
    };
    let has_table = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='sessions'")
        .and_then(|mut s| Ok(s.exists([])?))
        .unwrap_or(false);
    if !has_table {
        return (vec![], SourceInfo {
            status: Some("missing_table".into()),
            ..Default::default()
        });
    }
    let sql = r#"SELECT
        id, started_at, model,
        input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
        reasoning_tokens
        FROM sessions
        WHERE coalesce(input_tokens,0) + coalesce(output_tokens,0)
            + coalesce(cache_read_tokens,0) + coalesce(cache_write_tokens,0) > 0
        ORDER BY started_at, id"#;
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return (vec![], SourceInfo {
            status: Some("schema_mismatch".into()),
            ..Default::default()
        }),
    };
    let rows = stmt.query_map([], |row| {
        let started_at: f64 = row.get(1)?;
        let model: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
        let input: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
        let output: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
        let cache_read: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
        let cache_write: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
        let reasoning: i64 = row.get::<_, Option<i64>>(7)?.unwrap_or(0);
        // canonicalUsageCounts: inputIncludesCachedTokens=false for Hermes.
        // input stays as-is, cache tokens are separate.
        let canonical_input = input.max(0);
        let derived_total = canonical_input + output.max(0);
        let date = day_string_from_epoch(started_at);
        let ts = iso_string_from_epoch(started_at);
        Ok(UsageRecord {
            date,
            timestamp: Some(ts),
            tool: "Hermes Agent".to_string(),
            model,
            usage: TokenUsageCounts {
                input_tokens: canonical_input,
                output_tokens: output.max(0),
                cache_creation_input_tokens: cache_write.max(0),
                cache_read_input_tokens: cache_read.max(0),
                reasoning_output_tokens: reasoning.max(0),
                total_tokens: derived_total.max(0),
            },
            cost_usd: None,
            project_name: None,
            _request_id: String::new(),
        })
    });
    let mut records = Vec::new();
    if let Ok(iter) = rows {
        for r in iter.flatten() {
            records.push(r);
        }
    }
    let status = if records.is_empty() { "missing_valid_rows" } else { "ok" };
    let count = records.len() as i64;
    (records, SourceInfo {
        status: Some(status.into()),
        files: Some(1),
        records: Some(count),
        ..Default::default()
    })
}

/// Probe for WorkBuddy presence (no usage extracted). Returns source status only.
fn discover_workbuddy() -> SourceInfo {
    let roots = paths::workbuddy_roots();
    let found = roots.iter().filter(|r| r.exists()).count();
    SourceInfo {
        status: Some(if found > 0 { "discovered_no_usage" } else { "missing" }.into()),
        files: Some(found as i64),
        records: Some(0),
        ..Default::default()
    }
}

/// Convert epoch seconds (or milliseconds if > 10^10) to a "YYYY-MM-DD" day
/// string in Asia/Shanghai.
pub fn day_string_from_epoch(seconds: f64) -> String {
    let secs = if seconds > 10_000_000_000.0 { seconds / 1000.0 } else { seconds };
    let tz = local_tz();
    let dt = tz.timestamp_opt(secs as i64, 0).single().unwrap_or_else(|| tz.timestamp_opt(0, 0).unwrap());
    dt.format("%Y-%m-%d").to_string()
}

/// Convert epoch seconds (or milliseconds) to an ISO-8601 string in Asia/Shanghai.
pub fn iso_string_from_epoch(seconds: f64) -> String {
    let secs = if seconds > 10_000_000_000.0 { seconds / 1000.0 } else { seconds };
    let tz = local_tz();
    let dt = tz.timestamp_opt(secs as i64, 0).single().unwrap_or_else(|| tz.timestamp_opt(0, 0).unwrap());
    dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

// ---------------------------------------------------------------------------
// Agent Work aggregation (port of upstream AgentWorkAccumulator)
// ---------------------------------------------------------------------------

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
    hourly: BTreeMap<(String, i64), (i64, i64, i64, i64, bool)>,
    unbucketed_tokens: i64,
}

impl AgentWorkAccumulator {
    fn new() -> Self {
        // Start with coverage complete = true; AND across all records.
        Self { cache_coverage_complete: true, ..Default::default() }
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
                    e.0 += tokens; e.1 += inp; e.2 += cached; e.3 += outp; e.4 &= cov;
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
                hourly_buckets.push(AgentWorkHourBucket { hour: h, sources: hour_sources });
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
        let hour = rec.timestamp.as_ref().and_then(|ts| hour_from_iso(ts)).map(|h| h as i64);
        let acc = accs.entry(rec.date.clone()).or_insert_with(AgentWorkAccumulator::new);
        acc.add(rec, hour);
    }
    accs.into_iter()
        .filter(|(_, a)| a.total_tokens > 0)
        .map(|(date, a)| a.finalize(&date))
        .collect()
}
