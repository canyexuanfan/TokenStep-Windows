//! cursor collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

// ── Cursor transcript estimation (v0.1.12) ────────────────────────────────
// Cursor's official usage API covers only Cursor-billed (official) models —
// third-party-model usage emits no events. The agent transcripts under
// ~/.cursor/projects/ carry the full conversation content, so tokens are
// ESTIMATED from the text: CJK chars ≈ 0.6 tokens each, other chars ≈ 1/3.5
// (standard heuristics for GLM/DeepSeek-class tokenizers on mixed zh/en code
// conversations). Records are marked model "unknown" and tool "Cursor".

fn is_cjk_char(c: char) -> bool {
    let u = c as u32;
    (0x4E00..=0x9FFF).contains(&u)
        || (0x3400..=0x4DBF).contains(&u)
        || (0x3000..=0x303F).contains(&u)
        || (0xFF00..=0xFFEF).contains(&u)
}

#[allow(dead_code)] // 0.1.12 Cursor estimation helper (kept for the T12
                    // precision-upgrade path)
fn estimate_tokens_from_text(text: &str) -> i64 {
    let mut cjk = 0f64;
    let mut other = 0f64;
    for c in text.chars() {
        if is_cjk_char(c) {
            cjk += 1.0;
        } else {
            other += 1.0;
        }
    }
    (other / 3.5).floor() as i64 + (cjk * 0.6).floor() as i64
}

fn parse_cursor_ts(tag: &str) -> Option<i64> {
    // "<timestamp>Thursday, Sep 3, 2026, 11:18 PM (UTC+8)</timestamp>"
    let inner = tag
        .strip_prefix("<timestamp>")?
        .strip_suffix("</timestamp>")?;
    let rest = inner.trim();
    let tz_hours: i32 = rest
        .rfind("(UTC")
        .and_then(|i| {
            let s = &rest[i + 4..rest.len().min(i + 12)];
            let digits: String = s
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '+' || *c == '-')
                .collect();
            digits.trim_start_matches('+').parse::<i32>().ok()
        })
        .unwrap_or(8);
    let cleaned = rest
        .split(',')
        .skip(1)
        .collect::<Vec<_>>()
        .join(",")
        .trim()
        .to_string();
    let cleaned = cleaned
        .split('(')
        .next()
        .unwrap_or(&cleaned)
        .trim()
        .to_string();
    let naive =
        chrono::NaiveDateTime::parse_from_str(&format!("{cleaned}:00"), "%b %d, %Y, %I:%M:%S %p")
            .ok()?;
    let offset = chrono::FixedOffset::east_opt(tz_hours * 3600)?;
    Some(offset.from_local_datetime(&naive).single()?.timestamp())
}

pub(crate) fn collect_cursor_transcripts(
    cache: &mut CollectorCache,
) -> (Vec<UsageRecord>, SourceInfo) {
    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let mut records: Vec<UsageRecord> = Vec::new();
    let mut files_seen = 0i64;
    let local_day = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string();
    // composerId → modelConfig.modelName map (Cursor stores the selected
    // model per conversation in state.vscdb composerData rows).
    let mut composer_models: BTreeMap<String, String> = BTreeMap::new();
    if let Some(conn) = crate::sqlite_readonly::open_readonly(&crate::paths::cursor_state_vscdb()) {
        let query_result = conn
            .prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData%'")
            .and_then(|mut st| {
                st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    .map(|rows| rows.collect::<Result<Vec<_>, _>>().unwrap_or_default())
            });
        if let Ok(rows) = query_result {
            for row in rows {
                if let Ok(j) = serde_json::from_str::<serde_json::Value>(&row.1) {
                    let id = row
                        .0
                        .strip_prefix("composerData:")
                        .unwrap_or("")
                        .to_string();
                    let model = j
                        .pointer("/modelConfig/modelName")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| {
                            j.pointer("/modelConfig/selectedModels/0/modelId")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        });
                    if let (Some(m), true) = (model, !id.is_empty()) {
                        composer_models.insert(id, m);
                    }
                }
            }
        }
    }
    for root in paths::cursor_transcript_roots() {
        for path in jsonl_files_under(&root) {
            let key = path.to_string_lossy().to_string();
            live_paths.insert(key.clone());
            files_seen += 1;
            if let Some(cached) = cached_records(cache, &key, "Cursor", &path) {
                records.extend(cached);
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let session_id = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let model = composer_models
                .get(&session_id)
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            let meta_day = fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| day_string_from_epoch(d.as_secs() as f64))
                .unwrap_or_else(|| local_day.clone());
            // per-day token estimation within this transcript; day tracked
            // from the user-line <timestamp> tags, falling back to file mtime.
            let mut by_day: BTreeMap<String, i64> = BTreeMap::new();
            let mut current_day = meta_day.clone();
            for line in text.lines() {
                let Ok(j) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let role = j.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let mut chars_cjk = 0f64;
                let mut chars_other = 0f64;
                let mut day_from_line: Option<String> = None;
                if let Some(content) = j.pointer("/message/content").and_then(|v| v.as_array()) {
                    for item in content {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            if role == "user" {
                                if let (Some(a), Some(b)) =
                                    (t.find("<timestamp>"), t.find("</timestamp>"))
                                {
                                    if let Some(ts) = parse_cursor_ts(&t[a..=b]) {
                                        let d = day_string_from_epoch(ts as f64);
                                        if day_from_line.is_none() {
                                            day_from_line = Some(d.clone());
                                        }
                                    }
                                }
                            }
                            for c in t.chars() {
                                if is_cjk_char(c) {
                                    chars_cjk += 1.0;
                                } else {
                                    chars_other += 1.0;
                                }
                            }
                        }
                        if let Some(args) = item.get("input").and_then(|v| v.as_str()) {
                            for c in args.chars() {
                                if is_cjk_char(c) {
                                    chars_cjk += 1.0;
                                } else {
                                    chars_other += 1.0;
                                }
                            }
                        }
                    }
                }
                if let Some(d) = day_from_line {
                    current_day = d;
                }
                let est = (chars_other / 3.5).floor() as i64 + (chars_cjk * 0.6).floor() as i64;
                if est > 0 {
                    *by_day.entry(current_day.clone()).or_insert(0) += est;
                }
            }
            for (day, tokens) in &by_day {
                records.push(UsageRecord {
                    date: day.clone(),
                    timestamp: None,
                    tool: "Cursor".into(),
                    model: model.clone(),
                    usage: TokenUsageCounts {
                        input_tokens: *tokens,
                        output_tokens: 0,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        reasoning_output_tokens: 0,
                        total_tokens: *tokens,
                    },
                    cost_usd: None,
                });
            }
            let meta = fs::metadata(&path).ok();
            let mut cached_meta_records: Vec<CachedRecord> = Vec::new();
            for (day, tokens) in &by_day {
                cached_meta_records.push(CachedRecord {
                    date: day.clone(),
                    timestamp: None,
                    tool: "Cursor".into(),
                    model: model.clone(),
                    usage: TokenUsageCounts {
                        input_tokens: *tokens,
                        output_tokens: 0,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        reasoning_output_tokens: 0,
                        total_tokens: *tokens,
                    },
                    cost_usd: None,
                });
            }
            cache.files.insert(
                key,
                CachedUsageFile {
                    tool: "Cursor".into(),
                    size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    mtime: meta
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0),
                    records: cached_meta_records,
                    scan_offset: 0,
                    codex_session: None,
                    codex_parent_session: None,
                    codex_created_at: None,
                    codex_anchors: Vec::new(),
                },
            );
        }
    }
    let stale: Vec<String> = cache
        .files
        .iter()
        .filter(|(k, v)| v.tool == "Cursor" && !live_paths.contains(k.as_str()))
        .map(|(k, _)| k.clone())
        .collect();
    for k in stale {
        cache.files.remove(&k);
    }
    let status = if files_seen > 0 {
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
