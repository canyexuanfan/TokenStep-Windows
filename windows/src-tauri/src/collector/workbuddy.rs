//! workbuddy collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

/// WorkBuddy usage collector (v0.1.12): reads the per-project JSONL event
/// streams under ~/.workbuddy/projects/ — each function_call line carries
/// {timestamp, providerData.model, message.usage{input,output,total}}.
/// These are the same event_usage files the Token Rank client scans. Files
/// are mtime/size cached like Codex/Claude (they are append-only, but stay
/// small — a full rescan of all 458 files costs well under a second).
pub(crate) fn collect_workbuddy(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let mut records: Vec<UsageRecord> = Vec::new();
    let mut files_seen = 0i64;
    for root in paths::workbuddy_project_roots() {
        for path in jsonl_files_under(&root) {
            let key = path.to_string_lossy().to_string();
            live_paths.insert(key.clone());
            files_seen += 1;
            if let Some(cached) = cached_records(cache, &key, "WorkBuddy", &path) {
                records.extend(cached);
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let mut file_records: Vec<UsageRecord> = Vec::new();
            for line in text.lines() {
                if line.is_empty() || !line.contains("input_tokens") {
                    continue;
                }
                let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let Some(ts_raw) = obj.get("timestamp").and_then(|v| v.as_f64()) else {
                    continue;
                };
                if ts_raw <= 0.0 {
                    continue;
                }
                let Some(usage_val) = obj
                    .pointer("/message/usage")
                    .or_else(|| obj.pointer("/providerData/usage"))
                else {
                    continue;
                };
                let usage = usage_val.clone();
                let input = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let output = usage
                    .get("output_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let total = usage
                    .get("total_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(input + output);
                if total <= 0 {
                    continue;
                }
                let model = obj
                    .pointer("/providerData/model")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("model").and_then(|v| v.as_str()))
                    .unwrap_or("unknown")
                    .to_string();
                let ts_secs = ts_raw / 1000.0;
                file_records.push(UsageRecord {
                    date: day_string_from_epoch(ts_secs),
                    timestamp: Some(iso_string_from_epoch(ts_secs)),
                    tool: "WorkBuddy".into(),
                    model,
                    usage: TokenUsageCounts {
                        input_tokens: input,
                        output_tokens: output,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        reasoning_output_tokens: 0,
                        total_tokens: total,
                    },
                    cost_usd: None,
                });
            }
            records.extend(file_records.iter().cloned());
            let meta = file_meta(&path);
            cache.files.insert(
                key,
                CachedUsageFile {
                    tool: "WorkBuddy".into(),
                    size: meta.map(|m| m.0).unwrap_or(0),
                    mtime: meta.map(|m| m.1).unwrap_or(0.0),
                    records: file_records.iter().map(CachedRecord::from).collect(),
                    scan_offset: 0,
                    codex_session: None,
                    codex_parent_session: None,
                    codex_created_at: None,
                    codex_anchors: Vec::new(),
                    cursor_est_rev: None,
                },
            );
        }
    }
    // Drop stale entries for files that disappeared.
    let stale: Vec<String> = cache
        .files
        .iter()
        .filter(|(k, v)| v.tool == "WorkBuddy" && !live_paths.contains(*k))
        .map(|(k, _)| k.clone())
        .collect();
    for k in stale {
        cache.files.remove(&k);
    }
    let status = if files_seen > 0 || !records.is_empty() {
        SourceInfo {
            status: Some("ok".into()),
            files: Some(files_seen),
            records: Some(records.len() as i64),
            ..Default::default()
        }
    } else {
        SourceInfo {
            status: Some("missing".into()),
            files: Some(0),
            records: Some(0),
            ..Default::default()
        }
    };
    (records, status)
}

/// Probe for WorkBuddy presence (no usage extracted). Returns source status only.
#[allow(dead_code)] // upstream-parity probe (used when usage scanning is disabled)
fn discover_workbuddy() -> SourceInfo {
    let roots = paths::workbuddy_roots();
    let found = roots.iter().filter(|r| r.exists()).count();
    SourceInfo {
        status: Some(
            if found > 0 {
                "discovered_no_usage"
            } else {
                "missing"
            }
            .into(),
        ),
        files: Some(found as i64),
        records: Some(0),
        ..Default::default()
    }
}
