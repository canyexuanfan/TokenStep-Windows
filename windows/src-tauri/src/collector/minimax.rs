//! minimax collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

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
/// within each store and collision-free across them. Rows without a model
/// display as "MiniMax" (the client DB simply doesn't record it).
pub(crate) fn collect_minimax() -> (Vec<UsageRecord>, SourceInfo) {
    let mut records: Vec<UsageRecord> = Vec::new();
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
        records.append(&mut recs);
    }
    let status = match (dbs_ok, dbs_found) {
        (0, 0) => "missing_db",
        (0, _) => "schema_mismatch",
        _ => "ok",
    };
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

/// Read one MiniMax usage SQLite store; returns (records, schema_ok).
fn read_minimax_usage_db(
    db_path: &Path,
    table: &str,
    seen_rows: &mut BTreeSet<String>,
) -> (Vec<UsageRecord>, bool) {
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
        let ts_ms: i64 = row.get(1)?;
        let input: i64 = row.get(2)?;
        let output: i64 = row.get(3)?;
        let reasoning: i64 = row.get(4)?;
        let cache_read: i64 = row.get(5)?;
        let cache_write: i64 = row.get(6)?;
        let cost: f64 = row.get(7)?;
        Ok((
            model,
            ts_ms,
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            cost,
        ))
    });
    let mut records = Vec::new();
    if let Ok(iter) = rows {
        for row in iter.flatten() {
            let (model, ts_ms, input, output, reasoning, cache_read, cache_write, cost) = row;
            // Cross-store dedup: full-row fingerprint (turn_id is NOT unique —
            // one turn fans out to many API-call rows).
            let fingerprint =
                format!("{ts_ms}/{input}/{output}/{cache_read}/{cache_write}/{reasoning}");
            if !seen_rows.insert(fingerprint) {
                continue;
            }
            // Strip the provider prefix ("minimax/MiniMax-M3" → "MiniMax-M3");
            // rows the client left without a model display as 未记录模型 —
            // the runtime store simply does not record a model for them, and
            // naming them after the client would masquerade as a real model
            // in every 按模型 panel (0.1.13 fix for the per-provider look).
            let model = if model.is_empty() {
                "未记录模型".to_string()
            } else {
                model
                    .split_once('/')
                    .map(|(_, m)| m.to_string())
                    .unwrap_or(model)
            };
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
            records.push(UsageRecord {
                date,
                timestamp: Some(ts),
                tool: "MiniMax".to_string(),
                model,
                usage,
                cost_usd: Some(cost),
            });
        }
    }
    (records, true)
}
