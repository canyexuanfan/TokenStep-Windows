//! claude collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

/// Build the dedupe key for a Claude Code assistant row (port of upstream
/// v0.1.32 `claudeResponseKey`). Prefers the message's response id, then the
/// request id, then the transcript row's uuid, falling back to the file+line.
fn claude_response_key(
    obj: &serde_json::Value,
    message: &serde_json::Value,
    path_key: &str,
    line_no: usize,
) -> String {
    for s in [
        message.get("id").and_then(|v| v.as_str()),
        obj.get("requestId").and_then(|v| v.as_str()),
        obj.get("request_id").and_then(|v| v.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        let t = s.trim();
        if !t.is_empty() {
            return format!("response:{}", t);
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

use super::*;

pub(crate) fn collect_claude_code(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    let root = paths::claude_projects_root();
    let mut all_paths = jsonl_files_under(&root);
    all_paths.sort();

    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let mut records: Vec<UsageRecord> = Vec::new();

    for path in &all_paths {
        let key = path.to_string_lossy().to_string();
        live_paths.insert(key.clone());

        if let Some(cached) = cached_records(cache, &key, "Claude Code", path) {
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

        let mut responses: BTreeMap<String, ClaudeCandidate> = BTreeMap::new();
        // Stream line-by-line (memory optimization, port of macOS 170f655).
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
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
            let model = model_key(message.get("model").and_then(|v| v.as_str()).unwrap_or(""));
            let candidate = ClaudeCandidate {
                day,
                timestamp: timestamp.to_string(),
                model,
                usage: usage.clone(),
                has_stop_reason,
                line_no,
            };
            if let Some(existing) = responses.get(&response_key) {
                if !candidate.is_preferred_over(existing) {
                    continue;
                }
            }
            responses.insert(response_key, candidate);
        }
        let file_records: Vec<UsageRecord> = responses
            .values()
            .map(|c| UsageRecord {
                date: c.day.clone(),
                timestamp: Some(c.timestamp.clone()),
                tool: "Claude Code".to_string(),
                model: c.model.clone(),
                usage: c.usage.clone(),
                cost_usd: None,
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
