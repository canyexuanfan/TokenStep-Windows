//! antigravity collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

/// Antigravity (Google IDE / agy CLI) per-turn usage from the brain
/// transcript logs under `~/.gemini/antigravity*/brain/<session>/
/// .system_generated/logs/chunks/transcript/*.jsonl`. MODEL records carry
/// input_tokens / cache_read_tokens / output_tokens (input EXCLUDES cache —
/// totals are input+output+cache_read) and an RFC-3339 `created_at`.
/// Records do not name the model: it is tracked per session from
/// USER_SETTINGS_CHANGE lines ("user changed setting `Model Selection`
/// from X to Y"), falling back to "unknown". Chunk files are processed in
/// sorted order so a model switch in an early chunk applies to later ones.
pub(crate) fn collect_antigravity(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let mut records: Vec<UsageRecord> = Vec::new();
    let mut files_seen = 0i64;
    let local_day = chrono::Utc::now()
        .with_timezone(&local_tz())
        .format("%Y-%m-%d")
        .to_string();
    for root in paths::antigravity_brain_roots() {
        let Ok(sess_entries) = fs::read_dir(root.join("brain")) else {
            continue;
        };
        for sess in sess_entries.flatten() {
            let Ok(ft) = sess.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let tdir = sess
                .path()
                .join(".system_generated")
                .join("logs")
                .join("chunks")
                .join("transcript");
            let mut chunks: Vec<PathBuf> = fs::read_dir(&tdir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("jsonl"))
                        .map(|e| e.path())
                        .collect()
                })
                .unwrap_or_default();
            // The CLI mirrors the same transcript at logs/transcript{,_full}.jsonl
            // when no chunks exist — prefer chunks (live, growing), then the
            // full mirror, then the plain one, so a session is read exactly once.
            if chunks.is_empty() {
                let logs = sess.path().join(".system_generated").join("logs");
                for name in ["transcript_full.jsonl", "transcript.jsonl"] {
                    let p = logs.join(name);
                    if p.is_file() {
                        chunks.push(p);
                        break;
                    }
                }
            }
            chunks.sort();
            // Session-level model tracking across sorted chunk files.
            let mut session_model: Option<String> = None;
            for path in chunks {
                let key = path.to_string_lossy().to_string();
                live_paths.insert(key.clone());
                files_seen += 1;
                if let Some(cached) = cached_records(cache, &key, "Antigravity", &path) {
                    records.extend(cached);
                    continue;
                }
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                let meta_day = fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| day_string_from_epoch(d.as_secs() as f64))
                    .unwrap_or_else(|| local_day.clone());
                let mut file_records: Vec<UsageRecord> = Vec::new();
                for line in text.lines() {
                    if !line.contains("\"input_tokens\"") && !line.contains("Model Selection` from")
                    {
                        continue;
                    }
                    let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else {
                        continue;
                    };
                    if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
                        if let Some(idx) = content.find("Model Selection` from ") {
                            let rest = &content[idx + "Model Selection` from ".len()..];
                            if let Some(tidx) = rest.find(" to ") {
                                let after = &rest[tidx + 4..];
                                let end = after.find(['.', '\n']).unwrap_or(after.len());
                                let name = after[..end].trim().trim_end_matches('.').to_string();
                                if !name.is_empty() {
                                    session_model = Some(name);
                                }
                            }
                        }
                    }
                    let input = obj
                        .get("input_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let output = obj
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let cache_read = obj
                        .get("cache_read_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if input <= 0 && output <= 0 {
                        continue;
                    }
                    let usage = crate::agent_sources::canonical_counts(
                        input, output, cache_read, 0, 0, false, None,
                    );
                    let stamp = obj
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
                    let (date, ts) = match stamp {
                        Some(dt) => (
                            day_string_from_epoch(dt.timestamp() as f64),
                            Some(iso_string_from_epoch(dt.timestamp() as f64)),
                        ),
                        None => (meta_day.clone(), None),
                    };
                    file_records.push(UsageRecord {
                        date,
                        timestamp: ts,
                        tool: "Antigravity".to_string(),
                        model: session_model.clone().unwrap_or_else(|| "unknown".into()),
                        usage,
                        cost_usd: None,
                    });
                }
                records.extend(file_records.iter().cloned());
                let meta = file_meta(&path);
                cache.files.insert(
                    key,
                    CachedUsageFile {
                        tool: "Antigravity".into(),
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
    }
    // Drop stale entries for files that disappeared.
    let stale: Vec<String> = cache
        .files
        .iter()
        .filter(|(k, v)| v.tool == "Antigravity" && !live_paths.contains(k.as_str()))
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
