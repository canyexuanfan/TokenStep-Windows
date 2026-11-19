//! ccswitch collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

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
    let secs: i64 = if raw > 1e12 {
        (raw / 1000.0) as i64
    } else {
        raw as i64
    };
    let tz = local_tz();
    let dt = tz.timestamp_opt(secs, 0).single()?;
    Some(dt.format("%Y-%m-%d").to_string())
}

/// One CC Switch proxy-log row (values as returned by SQLite, pre-coercion).
type CCSwitchRow = (
    rusqlite::types::Value,
    rusqlite::types::Value,
    rusqlite::types::Value,
    rusqlite::types::Value,
    rusqlite::types::Value,
    rusqlite::types::Value,
    rusqlite::types::Value,
    rusqlite::types::Value,
);

pub(crate) fn collect_ccswitch() -> (Vec<UsageRecord>, SourceInfo) {
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

    let db_path = match paths::ccswitch_db_candidates()
        .into_iter()
        .find(|p| p.exists())
    {
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

    let row_data: Vec<CCSwitchRow> = match conn.prepare(&sql) {
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
        });
    }

    let status = if records.is_empty() {
        "missing_valid_rows"
    } else {
        "ok"
    };
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
