#![allow(dead_code)] // T1 sources are offline upstream (v0.2.2); kept as reference.
//! T1 experimental agent sources — a port of upstream macOS
//! `AgentSources.swift` (v0.2.0 G-A1). Seven optional, per-source-toggled
//! collectors: Gemini CLI / Qwen Code / Kimi Code / OpenCode / Amp / Droid /
//! Grok Build.
//!
//! Design discipline (upstream header note): every source defaults OFF, reads
//! only usage fields (never conversation bodies), and a parse failure in one
//! source degrades only that source's `SourceInfo` — it never blocks others.

use crate::collector::{self, UsageRecord};
use crate::models::{SourceInfo, TokenUsageCounts};
use crate::paths;
use rusqlite::OpenFlags;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The seven T1 source display names (= registry keys = `tool` strings).
pub const ALL_SOURCE_IDS: [&str; 7] = [
    "Gemini CLI",
    "Qwen Code",
    "Kimi Code",
    "OpenCode",
    "Amp",
    "Droid",
    "Grok Build",
];

/// Legacy v0.1.44 sources the per-source list may also name (display parity
/// with the upstream settings grid — collection of these three still keys
/// off the master switch alone).
pub const LEGACY_SOURCE_IDS: [&str; 3] = ["ZCode", "Hermes Agent", "WorkBuddy"];

/// Upstream `AgentSourceRegistry.enabledIDs` semantics (2026-08-13 ruling):
/// 1. master OFF → collect nothing;
/// 2. master ON + per-source list nil/empty → auto-enroll every *installed*
///    T1 source (directory probe);
/// 3. master ON + explicit list → the list decides (legacy names pass
///    through the filter for display purposes).
pub fn enabled_ids(master_enabled: bool, per_source: Option<&Vec<String>>) -> Vec<String> {
    if !master_enabled {
        return vec![];
    }
    match per_source {
        None => ALL_SOURCE_IDS
            .iter()
            .filter(|id| detector_status(id) == "installed")
            .map(|id| id.to_string())
            .collect(),
        Some(list) => list
            .iter()
            .filter(|id| {
                ALL_SOURCE_IDS.contains(&id.as_str())
                    || LEGACY_SOURCE_IDS.contains(&id.as_str())
            })
            .cloned()
            .collect(),
    }
}

/// Probe whether a source's data directory/database exists
/// (upstream `detectorStatus`): "installed" / "missing".
pub fn detector_status(id: &str) -> &'static str {
    let installed = match id {
        "Gemini CLI" => paths::gemini_tmp_dir().is_dir(),
        "Qwen Code" => paths::qwen_roots().iter().any(|p| p.is_dir()),
        "Kimi Code" => {
            paths::kimi_sessions_dir().is_dir()
                || paths::home_dir().join(".kimi").join("sessions").is_dir()
        }
        "OpenCode" => paths::opencode_db_path().is_file(),
        "Amp" => paths::amp_threads_dir().is_dir(),
        "Droid" => paths::droid_sessions_dir().is_dir(),
        "Grok Build" => paths::grok_sessions_dir().is_dir(),
        _ => false,
    };
    if installed {
        "installed"
    } else {
        "missing"
    }
}

/// Result of collecting one source (upstream `CollectorResult`).
pub struct AgentCollectResult {
    pub records: Vec<UsageRecord>,
    pub source: SourceInfo,
}

/// Collect all enabled T1 sources. Returns a map keyed by source display
/// name. A failing source returns a `SourceInfo` with its failure status and
/// no records — it never aborts the loop.
pub fn collect(enabled: &[String]) -> BTreeMap<String, AgentCollectResult> {
    let mut out = BTreeMap::new();
    for id in ALL_SOURCE_IDS {
        if !enabled.contains(&id.to_string()) {
            continue;
        }
        let result = match id {
            "Gemini CLI" => collect_gemini(),
            "Qwen Code" => collect_qwen(),
            "Kimi Code" => collect_kimi(),
            "OpenCode" => collect_opencode(),
            "Amp" => collect_generic("Amp", &paths::amp_threads_dir()),
            "Droid" => collect_generic("Droid", &paths::droid_sessions_dir()),
            "Grok Build" => collect_grok(),
            _ => unreachable!(),
        };
        out.insert(id.to_string(), result);
    }
    out
}

// ── shared support (upstream `AgentSourceSupport`) ────────────────────────

/// Lines longer than this are skipped entirely (upstream `maxLineBytes`).
const MAX_LINE_BYTES: u64 = 4_000_000;

/// Enumerate `*.jsonl` under `root` (recursive), skipping hidden entries,
/// sorted by path — upstream `allFiles`.
fn jsonl_files(root: &Path) -> Vec<PathBuf> {
    let pattern = root.join("**").join("*.jsonl");
    let Ok(entries) = glob::glob(&pattern.to_string_lossy()) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|p| {
            p.is_file()
                && !p
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

/// Enumerate files with the given extension + name prefix under `root`
/// (non-recursive; used by Gemini's `session-*.{json,jsonl}`).
fn prefixed_files(root: &Path, prefix: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            e.path().is_file()
                && name.starts_with(prefix)
                && extensions.iter().any(|ext| name.ends_with(ext))
        })
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

/// Iterate a JSONL file line by line, skipping lines over 4 MB. The callback
/// receives (line_index, parsed_value).
fn for_each_jsonl_line(path: &Path, mut body: impl FnMut(usize, &Value)) {
    let Ok(meta) = fs::metadata(path) else { return };
    if meta.len() > 512 * 1024 * 1024 {
        return; // absurd guard, same spirit as upstream
    }
    let Ok(text) = fs::read_to_string(path) else { return };
    for (idx, line) in text.lines().enumerate() {
        if line.len() as u64 > MAX_LINE_BYTES {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            body(idx, &value);
        }
    }
}

fn file_mtime_epoch(path: &Path) -> f64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Tolerant int/float → i64 with negative clamp (upstream `int`).
fn num_to_i64(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    if n <= 0.0 {
        return Some(0);
    }
    Some(n as i64)
}

/// First present alias among `keys` as i64.
fn aliased_i64(obj: &Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(n) = num_to_i64(v) {
                return n;
            }
        }
    }
    0
}

/// Canonical usage-count conversion (upstream `AgentSourceSupport.counts`):
/// unifies every source into "input includes cacheRead" semantics.
///
/// * `input_includes_cached = true`  → raw input already contains cached
///   tokens (Gemini/Qwen/Grok/ZCode).
/// * `input_includes_cached = false` → cache tokens are reported separately
///   and must be folded into input (Kimi/OpenCode/Amp/Droid/Hermes).
pub fn canonical_counts(
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation: i64,
    reasoning: i64,
    input_includes_cached: bool,
    explicit_total: Option<i64>,
) -> TokenUsageCounts {
    let cached = if input_includes_cached {
        cache_read.min(input)
    } else {
        cache_read
    };
    let input_tokens = if input_includes_cached {
        input.max(0)
    } else {
        (input + cache_read + cache_creation).max(0)
    };
    let total = explicit_total
        .filter(|t| *t > 0)
        .unwrap_or_else(|| (input_tokens + output.max(0)).max(0));
    TokenUsageCounts {
        input_tokens,
        output_tokens: output.max(0),
        cache_creation_input_tokens: cache_creation.max(0),
        cache_read_input_tokens: cached.max(0),
        reasoning_output_tokens: reasoning.max(0),
        total_tokens: total,
    }
}

/// Sanitized project display name — port of upstream
/// `UsageCollector.projectDisplayName(fromPath:)`: last `/`-separated
/// segment, trimmed, must contain a letter or digit, capped at 64 chars.
pub fn project_display_name(path: Option<&str>) -> Option<String> {
    let path = path?;
    let name = path.split('/').next_back()?.trim();
    if name.is_empty() {
        return None;
    }
    let meaningful = name.chars().any(|c| c.is_alphanumeric());
    if !meaningful {
        return None;
    }
    Some(name.chars().take(64).collect())
}

/// Build a record from epoch seconds + parsed fields.
#[allow(clippy::too_many_arguments)]
fn make_record(
    tool: &str,
    _request_id: String,
    epoch_secs: f64,
    model: String,
    usage: TokenUsageCounts,
    _project_name: Option<String>,
) -> UsageRecord {
    UsageRecord {
        date: collector::day_string_from_epoch(epoch_secs),
        timestamp: Some(collector::iso_string_from_epoch(epoch_secs)),
        tool: tool.to_string(),
        model,
        usage,
        cost_usd: None,
    }
}

fn missing_source(status: &str) -> AgentCollectResult {
    AgentCollectResult {
        records: vec![],
        source: SourceInfo { status: Some(status.to_string()), ..Default::default() },
    }
}

fn ok_source(records: &[UsageRecord], files: i64) -> SourceInfo {
    if records.is_empty() {
        SourceInfo {
            status: Some("missing_valid_rows".into()),
            files: Some(files),
            records: Some(0),
            ..Default::default()
        }
    } else {
        SourceInfo {
            status: Some("ok".into()),
            files: Some(files),
            records: Some(records.len() as i64),
            ..Default::default()
        }
    }
}

// ── Gemini CLI (upstream L237-358) ────────────────────────────────────────

fn collect_gemini() -> AgentCollectResult {
    let root = paths::gemini_tmp_dir();
    if !root.is_dir() {
        return missing_source("missing");
    }
    let files = prefixed_files(&root, "session-", &[".json", ".jsonl"]);
    if files.is_empty() {
        return missing_source("missing");
    }
    let mut records: Vec<UsageRecord> = vec![];
    for path in &files {
        let file_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
            // JSONL variant: per-line usageMetadata.
            for_each_jsonl_line(path, |idx, obj| {
                let Some(um) = obj.get("usageMetadata") else { return };
                let prompt = aliased_i64(um, &["promptTokenCount"]);
                let output = aliased_i64(um, &["candidatesTokenCount"]);
                let cached = aliased_i64(um, &["cachedContentTokenCount"]);
                let thoughts = aliased_i64(um, &["thoughtsTokenCount"]);
                let total = aliased_i64(um, &["totalTokenCount"]);
                if prompt + output == 0 {
                    return;
                }
                let epoch = obj
                    .get("timestamp")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| file_mtime_epoch(path));
                if epoch <= 0.0 {
                    return;
                }
                let usage = canonical_counts(prompt, output, cached, 0, thoughts, true, Some(total));
                records.push(make_record(
                    "Gemini CLI",
                    format!("gemini:{}:{}", file_name, idx),
                    epoch,
                    "gemini".to_string(),
                    usage,
                    obj.get("cwd").and_then(|v| v.as_str()).and_then(|s| project_display_name(Some(s))),
                ));
            });
        } else {
            // Single-document JSON: messages[].tokens (type == "gemini").
            let Ok(text) = fs::read_to_string(path) else { continue };
            let Ok(doc) = serde_json::from_str::<Value>(&text) else { continue };
            let session_id = doc
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| file_name.clone());
            let Some(messages) = doc.get("messages").and_then(|v| v.as_array()) else { continue };
            for (idx, msg) in messages.iter().enumerate() {
                if msg.get("type").and_then(|v| v.as_str()) != Some("gemini") {
                    continue;
                }
                let Some(tokens) = msg.get("tokens") else { continue };
                let input = aliased_i64(tokens, &["input"]);
                let tool_tokens = aliased_i64(tokens, &["tool"]);
                let output = aliased_i64(tokens, &["output"]);
                let cached = aliased_i64(tokens, &["cached"]);
                let thoughts = aliased_i64(tokens, &["thoughts"]);
                let total = aliased_i64(tokens, &["total"]);
                // Upstream: input already includes cached; input += tool.
                let combined_input = input + tool_tokens;
                if combined_input + output == 0 {
                    continue;
                }
                let Some(ts) = msg.get("timestamp").and_then(|v| v.as_str()) else { continue };
                let Some(dt) = collector::parse_iso(ts) else { continue };
                let epoch = dt.timestamp() as f64;
                let usage = canonical_counts(
                    combined_input, output, cached, 0, thoughts, true, Some(total),
                );
                let message_id = msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| idx.to_string());
                records.push(make_record(
                    "Gemini CLI",
                    format!("gemini:{}:{}", session_id, message_id),
                    epoch,
                    "gemini".to_string(),
                    usage,
                    doc.get("cwd").and_then(|v| v.as_str()).and_then(|s| project_display_name(Some(s))),
                ));
            }
        }
    }
    let files_count = files.len() as i64;
    AgentCollectResult { source: ok_source(&records, files_count), records }
}

// ── Qwen Code (upstream L362-426) ─────────────────────────────────────────

fn collect_qwen() -> AgentCollectResult {
    let roots = paths::qwen_roots();
    let existing: Vec<&PathBuf> = roots.iter().filter(|p| p.is_dir()).collect();
    if existing.is_empty() {
        return missing_source("missing");
    }
    let mut files: Vec<PathBuf> = vec![];
    for root in &existing {
        files.extend(jsonl_files(root));
    }
    if files.is_empty() {
        return missing_source("missing");
    }
    files.sort();
    let mut records: Vec<UsageRecord> = vec![];
    for path in &files {
        let session = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mtime = file_mtime_epoch(path);
        for_each_jsonl_line(path, |idx, obj| {
            let Some(um) = obj.get("usageMetadata") else { return };
            let prompt = aliased_i64(um, &["promptTokenCount"]);
            let output = aliased_i64(um, &["candidatesTokenCount"]);
            let cached = aliased_i64(um, &["cachedContentTokenCount"]);
            let thoughts = aliased_i64(um, &["thoughtsTokenCount"]);
            let total = aliased_i64(um, &["totalTokenCount"]);
            if prompt + output == 0 {
                return;
            }
            // Three-level timestamp fallback: numeric → ISO string → mtime.
            let epoch = obj
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .or_else(|| {
                    obj.get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(collector::parse_iso)
                        .map(|dt| dt.timestamp() as f64)
                })
                .unwrap_or(mtime);
            if epoch <= 0.0 {
                return;
            }
            let usage = canonical_counts(prompt, output, cached, 0, thoughts, true, Some(total));
            records.push(make_record(
                "Qwen Code",
                format!("qwen:{}:{}", session, idx),
                epoch,
                "qwen".to_string(),
                usage,
                obj.get("cwd").and_then(|v| v.as_str()).and_then(|s| project_display_name(Some(s))),
            ));
        });
    }
    let files_count = files.len() as i64;
    AgentCollectResult { source: ok_source(&records, files_count), records }
}

// ── Kimi Code (upstream L430-502) ─────────────────────────────────────────

fn collect_kimi() -> AgentCollectResult {
    let root = paths::kimi_sessions_dir();
    if !root.is_dir() {
        return missing_source("missing");
    }
    // Only files named wire.jsonl; session id = grandparent directory name.
    let pattern = root.join("**").join("wire.jsonl");
    let Ok(entries) = glob::glob(&pattern.to_string_lossy()) else {
        return missing_source("missing");
    };
    let files: Vec<PathBuf> = entries.flatten().filter(|p| p.is_file()).collect();
    if files.is_empty() {
        return missing_source("missing");
    }
    let mut records: Vec<UsageRecord> = vec![];
    for path in &files {
        // .../<session-id>/wire.jsonl → session = parent dir name.
        let session = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        for_each_jsonl_line(path, |idx, obj| {
            let Some(message) = obj.get("message") else { return };
            let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if !msg_type.to_lowercase().contains("usagerecord") {
                return;
            }
            let payload = message.get("payload").unwrap_or(message);
            let usage_obj = payload.get("usage").unwrap_or(payload);
            let input = aliased_i64(usage_obj, &["input_tokens", "inputTokens"]);
            let output = aliased_i64(usage_obj, &["output_tokens", "outputTokens"]);
            let cached = aliased_i64(
                usage_obj,
                &["cached_tokens", "cachedInputTokens", "cache_read_tokens"],
            );
            let cache_write = aliased_i64(
                usage_obj,
                &["cache_creation_tokens", "cacheCreationTokens"],
            );
            let reasoning = aliased_i64(usage_obj, &["reasoning_tokens", "reasoningTokens"]);
            if input + output == 0 {
                return;
            }
            let epoch = obj
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .or_else(|| obj.get("timestamp").and_then(|v| v.as_i64()).map(|n| n as f64))
                .unwrap_or(0.0);
            if epoch <= 0.0 {
                return;
            }
            // Kimi reports cache separately from input.
            let usage = canonical_counts(input, output, cached, cache_write, reasoning, false, None);
            records.push(make_record(
                "Kimi Code",
                format!("kimi:{}:{}", session, idx),
                epoch,
                "kimi".to_string(),
                usage,
                payload.get("cwd").and_then(|v| v.as_str()).and_then(|s| project_display_name(Some(s))),
            ));
        });
    }
    let files_count = files.len() as i64;
    AgentCollectResult { source: ok_source(&records, files_count), records }
}

// ── OpenCode (upstream L506-582) ──────────────────────────────────────────

fn collect_opencode() -> AgentCollectResult {
    let db = paths::opencode_db_path();
    if !db.is_file() {
        return missing_source("missing_db");
    }
    let uri = format!("file:{}?mode=ro", db.display().to_string().replace('\\', "/"));
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return missing_source("missing_db");
    };
    // Same SQL as upstream (which shells out to `sqlite3 -readonly -json`;
    // here we query in-process — the statement itself is identical).
    let sql = r#"select time_created,
       json_extract(data, '$.modelID'),
       json_extract(data, '$.tokens.input'),
       json_extract(data, '$.tokens.output'),
       json_extract(data, '$.tokens.reasoning'),
       json_extract(data, '$.tokens.cache.read'),
       json_extract(data, '$.tokens.cache.write'),
       json_extract(data, '$.path.cwd')
from message
where json_extract(data, '$.role') = 'assistant'"#;
    let Ok(mut stmt) = conn.prepare(sql) else {
        return missing_source("query_failed");
    };
    let rows = stmt.query_map([], |row| {
        let created: i64 = row.get::<_, Option<i64>>(0)?.unwrap_or(0);
        let model: String =
            row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "opencode-unknown".into());
        let input: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        let output: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
        let reasoning: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
        let cache_read: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
        let cache_write: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
        let cwd: Option<String> = row.get(7).ok().flatten();
        Ok((created, model, input, output, reasoning, cache_read, cache_write, cwd))
    });
    let Ok(rows) = rows else {
        return missing_source("query_failed");
    };
    let mut records: Vec<UsageRecord> = vec![];
    for r in rows.flatten() {
        let (created, model, input, output, reasoning, cache_read, cache_write, cwd) = r;
        // Millisecond vs second epoch heuristic (upstream L543).
        let epoch = if created > 10_000_000_000 { (created / 1000) as f64 } else { created as f64 };
        if epoch <= 0.0 {
            continue;
        }
        // OpenCode reports cache separately from input.
        let usage = canonical_counts(input, output, cache_read, cache_write, reasoning, false, None);
        let total = usage.total_tokens;
        records.push(make_record(
            "OpenCode",
            format!("opencode:{}:{}", created, total),
            epoch,
            model,
            usage,
            cwd.as_deref().and_then(|s| project_display_name(Some(s))),
        ));
    }
    let source = if records.is_empty() {
        SourceInfo {
            status: Some("missing_valid_rows".into()),
            files: Some(1),
            records: Some(0),
            ..Default::default()
        }
    } else {
        SourceInfo {
            status: Some("ok".into()),
            files: Some(1),
            records: Some(records.len() as i64),
            ..Default::default()
        }
    };
    AgentCollectResult { source, records }
}

// ── Amp / Droid (upstream GenericJSONLSource L672-737) ────────────────────

fn collect_generic(tool: &str, root: &Path) -> AgentCollectResult {
    if !root.is_dir() {
        return missing_source("missing");
    }
    let files = jsonl_files(root);
    if files.is_empty() {
        return missing_source("missing");
    }
    let mut records: Vec<UsageRecord> = vec![];
    for path in &files {
        let session = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mtime = file_mtime_epoch(path);
        for_each_jsonl_line(path, |idx, obj| {
            // usage location fallbacks: obj.usage → obj.payload.usage → obj.update.usage
            let usage_obj = obj
                .get("usage")
                .or_else(|| obj.get("payload").and_then(|p| p.get("usage")))
                .or_else(|| obj.get("update").and_then(|u| u.get("usage")));
            let Some(usage_obj) = usage_obj else { return };
            let input = aliased_i64(usage_obj, &["input_tokens", "inputTokens", "input"]);
            let output = aliased_i64(usage_obj, &["output_tokens", "outputTokens", "output"]);
            let cache_read = aliased_i64(
                usage_obj,
                &["cache_read_tokens", "cachedReadTokens", "cached_input_tokens"],
            );
            if input + output == 0 {
                return;
            }
            let model = obj
                .get("model")
                .and_then(|v| v.as_str())
                .or_else(|| usage_obj.get("model").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}-unknown", tool.to_lowercase()));
            let epoch = obj
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .or_else(|| {
                    obj.get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(collector::parse_iso)
                        .map(|dt| dt.timestamp() as f64)
                })
                .unwrap_or(mtime);
            if epoch <= 0.0 {
                return;
            }
            // Generic sources report cache separately from input.
            let usage = canonical_counts(input, output, cache_read, 0, 0, false, None);
            records.push(make_record(
                tool,
                format!("{}:{}:{}", tool.to_lowercase(), session, idx),
                epoch,
                model,
                usage,
                obj.get("cwd").and_then(|v| v.as_str()).and_then(|s| project_display_name(Some(s))),
            ));
        });
    }
    let files_count = files.len() as i64;
    AgentCollectResult { source: ok_source(&records, files_count), records }
}

// ── Grok Build (upstream L586-668) ────────────────────────────────────────

fn collect_grok() -> AgentCollectResult {
    let root = paths::grok_sessions_dir();
    if !root.is_dir() {
        return missing_source("missing");
    }
    let pattern = root.join("**").join("updates.jsonl");
    let Ok(entries) = glob::glob(&pattern.to_string_lossy()) else {
        return missing_source("missing");
    };
    let files: Vec<PathBuf> = entries.flatten().filter(|p| p.is_file()).collect();
    if files.is_empty() {
        return missing_source("missing");
    }
    let mut records: Vec<UsageRecord> = vec![];
    for path in &files {
        // .../<url-encoded cwd>/<session-id>/updates.jsonl
        // project = decoded last segment of the cwd; session = parent dir.
        let session = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let project = path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .and_then(|encoded| {
                percent_decode(&encoded)
                    .split('/')
                    .next_back()
                    .map(|s| s.trim().to_string())
            })
            .and_then(|n| if n.is_empty() { None } else { Some(n) });
        for_each_jsonl_line(path, |idx, obj| {
            let Some(update) = obj.get("params").and_then(|p| p.get("update")) else { return };
            if update.get("sessionUpdate").and_then(|v| v.as_str()) != Some("turn_completed") {
                return;
            }
            let Some(usage_obj) = update.get("usage") else { return };
            let input = aliased_i64(usage_obj, &["inputTokens"]);
            let output = aliased_i64(usage_obj, &["outputTokens"]);
            let cached = aliased_i64(usage_obj, &["cachedReadTokens"]);
            let reasoning = aliased_i64(usage_obj, &["reasoningTokens"]);
            let total = aliased_i64(usage_obj, &["totalTokens"]);
            if input + output == 0 {
                return;
            }
            // Primary model = first key of usage.modelUsage in sorted order.
            let model: String = usage_obj
                .get("modelUsage")
                .and_then(|m| m.as_object())
                .and_then(|m| {
                    let mut keys: Vec<&String> = m.keys().collect();
                    keys.sort();
                    keys.first().map(|k| (*k).clone())
                })
                .unwrap_or_else(|| "grok-unknown".to_string());
            let epoch = obj
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .or_else(|| obj.get("timestamp").and_then(|v| v.as_i64()).map(|n| n as f64))
                .unwrap_or(0.0);
            if epoch <= 0.0 {
                return;
            }
            let prompt_id = obj
                .get("prompt_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| idx.to_string());
            // Grok reports input including cached reads; totalTokens is
            // authoritative when present.
            let usage = canonical_counts(input, output, cached, 0, reasoning, true, Some(total));
            records.push(make_record(
                "Grok Build",
                format!("grok:{}:{}", session, prompt_id),
                epoch,
                model,
                usage,
                project.clone().filter(|n| n.chars().any(|c| c.is_alphanumeric())),
            ));
        });
    }
    let files_count = files.len() as i64;
    AgentCollectResult { source: ok_source(&records, files_count), records }
}

/// Minimal percent-decoding for URL-encoded directory names.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() - 1 + 1 {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
