//! zcode_hermes collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

pub(crate) fn collect_zcode() -> (Vec<UsageRecord>, SourceInfo) {
    let db_path = paths::zcode_db_path();
    if !db_path.exists() {
        return (
            vec![],
            SourceInfo {
                status: Some("missing_db".into()),
                ..Default::default()
            },
        );
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => {
            return (
                vec![],
                SourceInfo {
                    status: Some("unreadable_db".into()),
                    ..Default::default()
                },
            )
        }
    };
    // Verify schema: table model_usage must exist with key columns.
    let has_table = conn
        .prepare(
            "SELECT count(*) as n FROM sqlite_master WHERE type='table' AND name='model_usage'",
        )
        .and_then(|mut s| s.query_row([], |r| r.get::<_, i64>(0)))
        .map(|n| n > 0)
        .unwrap_or(false);
    if !has_table {
        return (
            vec![],
            SourceInfo {
                status: Some("missing_table".into()),
                ..Default::default()
            },
        );
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
        "id",
        "session_id",
        "status",
        "started_at",
        "model_id",
        "input_tokens",
        "output_tokens",
        "reasoning_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "computed_total_tokens",
        "tool_call_count",
    ];
    if required_cols
        .iter()
        .any(|c| !available_columns.contains(*c))
    {
        return (
            vec![],
            SourceInfo {
                status: Some("schema_mismatch".into()),
                ..Default::default()
            },
        );
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
        Err(_) => {
            return (
                vec![],
                SourceInfo {
                    status: Some("schema_mismatch".into()),
                    ..Default::default()
                },
            )
        }
    };
    let rows = stmt.query_map([], |row| {
        let started_at: f64 = row.get(0)?;
        let model: String = row
            .get::<_, Option<String>>(1)?
            .unwrap_or_else(|| "unknown".into());
        let input: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        let output: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
        let reasoning: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
        let cache_create: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
        let cache_read: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
        let computed_total: i64 = row.get::<_, Option<i64>>(7)?.unwrap_or(0);
        let provider_total: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(0);
        // tool_call_count (column 9) is read for forward compatibility with
        // the upstream agent-work accumulator; not consumed here.
        let _project_directory: Option<String> = if has_session_table {
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
            Some(if computed_total > 0 {
                computed_total
            } else {
                provider_total
            }),
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
        })
    });
    let mut records = Vec::new();
    if let Ok(iter) = rows {
        for r in iter.flatten() {
            records.push(r);
        }
    }
    let status = if records.is_empty() {
        "missing_valid_rows"
    } else {
        "ok"
    };
    let count = records.len() as i64;
    (
        records,
        SourceInfo {
            status: Some(status.into()),
            files: Some(1),
            records: Some(count),
            ..Default::default()
        },
    )
}

/// Read Hermes agent usage from `~/.hermes/state.db`.
/// Hermes has cache tokens SEPARATE from input (inputIncludesCachedTokens=false).
pub(crate) fn collect_hermes() -> (Vec<UsageRecord>, SourceInfo) {
    let db_path = paths::hermes_db_path();
    if !db_path.exists() {
        return (
            vec![],
            SourceInfo {
                status: Some("missing_db".into()),
                ..Default::default()
            },
        );
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => {
            return (
                vec![],
                SourceInfo {
                    status: Some("unreadable_db".into()),
                    ..Default::default()
                },
            )
        }
    };
    let has_table = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='sessions'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if !has_table {
        return (
            vec![],
            SourceInfo {
                status: Some("missing_table".into()),
                ..Default::default()
            },
        );
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
        Err(_) => {
            return (
                vec![],
                SourceInfo {
                    status: Some("schema_mismatch".into()),
                    ..Default::default()
                },
            )
        }
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
        })
    });
    let mut records = Vec::new();
    if let Ok(iter) = rows {
        for r in iter.flatten() {
            records.push(r);
        }
    }
    let status = if records.is_empty() {
        "missing_valid_rows"
    } else {
        "ok"
    };
    let count = records.len() as i64;
    (
        records,
        SourceInfo {
            status: Some(status.into()),
            files: Some(1),
            records: Some(count),
            ..Default::default()
        },
    )
}
