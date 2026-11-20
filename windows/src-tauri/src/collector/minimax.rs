//! minimax collector — split from collector.rs in 0.1.13 T10 (pure move,
//! golden-test guarded). 0.1.13 later rounds: session model attribution —
//! rows the usage stores leave without a model are attributed from the
//! session detail directories (`llm-call.json` / `messages.jsonl` majority),
//! verified to cover 100% of the NULL-model usage on real data.

use super::*;

use base64::Engine as _;

/// MiniMax (Mavis desktop runtime) per-turn usage from two same-schema
/// SQLite stores covering disjoint date ranges:
///   - `~/.minimax/sqlite.db` table `token_usage` (May–Jul 2026 era)
///   - `~/.minimax/v2/sqlite/runtime-state.sqlite` table
///     `local_runtime_token_usage` (live store, Jul 2026 → now)
///
/// Schema: id / session_id / agent_name / framework_type / turn_id / model
/// ("minimax/MiniMax-M3", often NULL in the live store) / ts (epoch ms) /
/// input_tokens / output_tokens / reasoning_tokens / cache_read_tokens /
/// cache_write_tokens / cost_usd / raw. In `raw` total = input + output +
/// cache_read, so input EXCLUDES cached tokens
/// (inputIncludesCachedTokens=false). NOTE: turn_id is NOT unique (one turn
/// fans out to many API-call rows), so cross-store dedup keys on the full
/// row fingerprint (ts + per-component counts) instead — verified unique
/// within each store and collision-free across them.
pub(crate) fn collect_minimax(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    let mut rows: Vec<MinimaxRow> = Vec::new();
    let mut seen_rows: BTreeSet<String> = BTreeSet::new();
    let mut dbs_found = 0usize;
    let mut dbs_ok = 0usize;
    for (path, table) in [
        (paths::minimax_db_path(), "token_usage"),
        (
            paths::minimax_runtime_db_path(),
            "local_runtime_token_usage",
        ),
    ] {
        if !path.exists() {
            continue;
        }
        dbs_found += 1;
        let (mut recs, ok) = read_minimax_usage_db(&path, table, &mut seen_rows);
        if ok {
            dbs_ok += 1;
        }
        rows.append(&mut recs);
    }
    let status = match (dbs_ok, dbs_found) {
        (0, 0) => "missing_db",
        (0, _) => "schema_mismatch",
        _ => "ok",
    };

    // Attribution: the client leaves most live-store rows without a model,
    // but every session carries its model in the detail directory
    // (`~/.minimax/v2/sessions/…/llm-call.json`, falling back to the
    // majority model across `messages.jsonl`). Verified to attribute 100%
    // of NULL-model usage on real data; rows with nothing to attribute keep
    // the honest 未记录模型 label.
    let needs_attribution = rows.iter().any(|r| {
        r.model_raw.is_empty() && !cache.minimax_session_models.contains_key(&r.session_id)
    });
    let dir_index = if needs_attribution {
        minimax_session_dir_index()
    } else {
        BTreeMap::new()
    };
    for row in rows.iter_mut() {
        if !row.model_raw.is_empty() {
            continue;
        }
        let model = match cache.minimax_session_models.get(&row.session_id) {
            Some(cached) => Some(cached.model.clone()),
            None => {
                let resolved = dir_index
                    .get(&row.session_id)
                    .and_then(|dir| minimax_session_model(dir));
                if let Some(m) = &resolved {
                    let source_mtime = minimax_source_mtime(dir_index.get(&row.session_id));
                    cache.minimax_session_models.insert(
                        row.session_id.clone(),
                        MinimaxSessionModel {
                            model: m.clone(),
                            source_mtime,
                        },
                    );
                }
                resolved
            }
        };
        // strip the provider prefix ("minimax/MiniMax-M3" → "MiniMax-M3")
        row.model_raw = model.unwrap_or_default();
    }

    let mut records = Vec::new();
    for row in rows {
        let model = if row.model_raw.is_empty() {
            // rows with no model anywhere display as 未记录模型 — naming
            // them after the client would masquerade as a real model in
            // every 按模型 panel (0.1.13 fix for the per-provider look).
            "未记录模型".to_string()
        } else {
            row.model_raw
                .split_once('/')
                .map(|(_, m)| m.to_string())
                .unwrap_or(row.model_raw)
        };
        records.push(UsageRecord {
            model,
            ..row.record
        });
    }
    let count = records.len() as i64;
    (
        records,
        SourceInfo {
            status: Some(status.into()),
            files: Some(dbs_ok as i64),
            records: Some(count),
            ..Default::default()
        },
    )
}

/// One usage-store row plus the fields attribution needs before the record
/// is finalized (session id + raw model; empty raw model = unattributed).
struct MinimaxRow {
    session_id: String,
    model_raw: String,
    record: UsageRecord,
}

/// Map session_id → detail directory for every MiniMax session directory on
/// disk. Directory names are `<time>-session_<base64(sessionId)>`; the
/// base64 payload is the full session id.
fn minimax_session_dir_index() -> BTreeMap<String, PathBuf> {
    let mut map = BTreeMap::new();
    let root = paths::minimax_sessions_root();
    let years = match fs::read_dir(&root) {
        Ok(r) => r,
        Err(_) => return map,
    };
    for year in years.flatten() {
        let months = match fs::read_dir(year.path()) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for month in months.flatten() {
            let days = match fs::read_dir(month.path()) {
                Ok(r) => r,
                Err(_) => continue,
            };
            for day in days.flatten() {
                let entries = match fs::read_dir(day.path()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let Some(encoded) = name.split("session_").nth(1) else {
                        continue;
                    };
                    let padded = format!("{}{}", encoded, "=".repeat((4 - encoded.len() % 4) % 4));
                    if let Ok(bytes) =
                        base64::engine::general_purpose::STANDARD.decode(padded.as_bytes())
                    {
                        if let Ok(sid) = String::from_utf8(bytes) {
                            map.insert(sid, entry.path());
                        }
                    }
                }
            }
        }
    }
    map
}

/// Model for one session directory: `llm-call.json` first (the session's
/// LLM config), else the majority model across `messages.jsonl` assistant
/// messages.
fn minimax_session_model(dir: &Path) -> Option<String> {
    let llm_call = dir.join("llm-call.json");
    if let Ok(text) = fs::read_to_string(&llm_call) {
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(m) = j.get("model").and_then(|v| v.as_str()) {
                if !m.is_empty() {
                    return Some(m.to_string());
                }
            }
        }
    }
    let messages = dir.join("messages.jsonl");
    let text = fs::read_to_string(&messages).ok()?;
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for line in text.lines() {
        let Ok(j) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let model = j
            .pointer("/message/model")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !model.is_empty() {
            *counts.entry(model.to_string()).or_insert(0) += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(m, _)| m)
}

/// mtime of the file a session's model attribution was read from (0 when
/// the directory is gone — a cached entry then survives until re-resolved).
fn minimax_source_mtime(dir: Option<&PathBuf>) -> f64 {
    let Some(dir) = dir else {
        return 0.0;
    };
    for name in ["llm-call.json", "messages.jsonl"] {
        let p = dir.join(name);
        if let Ok(meta) = fs::metadata(&p) {
            if let Ok(m) = meta.modified() {
                if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                    return d.as_secs_f64();
                }
            }
        }
    }
    0.0
}

/// Read one MiniMax usage SQLite store; returns (rows, schema_ok).
fn read_minimax_usage_db(
    db_path: &Path,
    table: &str,
    seen_rows: &mut BTreeSet<String>,
) -> (Vec<MinimaxRow>, bool) {
    let conn = match rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return (vec![], false),
    };
    // Probe required columns (app schema may change between versions).
    let available_columns: BTreeSet<String> = conn
        .prepare(&format!("pragma table_info({table})"))
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
        "model",
        "ts",
        "session_id",
        "turn_id",
        "input_tokens",
        "output_tokens",
        "reasoning_tokens",
        "cache_read_tokens",
        "cache_write_tokens",
        "cost_usd",
    ];
    if required_cols
        .iter()
        .any(|c| !available_columns.contains(*c))
    {
        return (vec![], false);
    }
    let sql = format!(
        r#"select
    coalesce(nullif({table}.model, ''), '') as display_model,
    {table}.session_id,
    {table}.ts,
    coalesce({table}.input_tokens, 0),
    coalesce({table}.output_tokens, 0),
    coalesce({table}.reasoning_tokens, 0),
    coalesce({table}.cache_read_tokens, 0),
    coalesce({table}.cache_write_tokens, 0),
    coalesce({table}.cost_usd, 0.0)
from {table}
where coalesce({table}.input_tokens, 0)
    + coalesce({table}.output_tokens, 0)
    + coalesce({table}.reasoning_tokens, 0)
    + coalesce({table}.cache_read_tokens, 0)
    + coalesce({table}.cache_write_tokens, 0) > 0
order by {table}.ts, {table}.id"#
    );
    let mut stmt = match conn.prepare(sql.as_str()) {
        Ok(s) => s,
        Err(_) => return (vec![], false),
    };
    let rows = stmt.query_map([], |row| {
        let model: String = row.get::<_, Option<String>>(0)?.unwrap_or_default();
        let session_id: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
        let ts_ms: i64 = row.get(2)?;
        let input: i64 = row.get(3)?;
        let output: i64 = row.get(4)?;
        let reasoning: i64 = row.get(5)?;
        let cache_read: i64 = row.get(6)?;
        let cache_write: i64 = row.get(7)?;
        let cost: f64 = row.get(8)?;
        Ok((
            model,
            session_id,
            ts_ms,
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            cost,
        ))
    });
    let mut rows_out = Vec::new();
    if let Ok(iter) = rows {
        for row in iter.flatten() {
            let (model, session_id, ts_ms, input, output, reasoning, cache_read, cache_write, cost) =
                row;
            // Cross-store dedup: full-row fingerprint (turn_id is NOT unique —
            // one turn fans out to many API-call rows).
            let fingerprint =
                format!("{ts_ms}/{input}/{output}/{cache_read}/{cache_write}/{reasoning}");
            if !seen_rows.insert(fingerprint) {
                continue;
            }
            let usage = crate::agent_sources::canonical_counts(
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
                false,
                None,
            );
            let date = day_string_from_epoch(ts_ms as f64 / 1000.0);
            let ts = iso_string_from_epoch(ts_ms as f64 / 1000.0);
            rows_out.push(MinimaxRow {
                session_id,
                model_raw: model,
                record: UsageRecord {
                    date,
                    timestamp: Some(ts),
                    tool: "MiniMax".to_string(),
                    model: String::new(),
                    usage,
                    cost_usd: Some(cost),
                },
            });
        }
    }
    (rows_out, true)
}
