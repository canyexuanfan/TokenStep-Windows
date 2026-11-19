//! dsh collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

/// DeepSeek Harness usage, surfaced under the client name "DSH" (the
/// harness product name — "DeepSeek" alone would read as a model).
///
/// PRIMARY: `~/.dsh/dsh-tokenlens/usage-cache.json` — the harness's own
/// TokenLens statistics cache, i.e. the exact source its stats panel renders
/// (per session file: `models` → {"providerId/modelId": {model,
/// today:{day:counts}, total:counts}}, counts {input, output, cacheRead,
/// cacheWrite, total}). Input EXCLUDES cached tokens
/// (total = input+output+cacheRead+cacheWrite). Full history — matches the
/// panel's 1.5B exactly; day-level resolution only (no hour data used yet).
///
/// FALLBACK: the per-call billing journal `~/.dsh/.dshw-usage.json`
/// (events: {ts, day, model, cost, tokens}); it was rebuilt on 2026-11-16
/// (recharge-fix) so it misses older history, but covers fresh installs that
/// haven't opened the TokenLens panel yet.
pub(crate) fn collect_dsh() -> (Vec<UsageRecord>, SourceInfo) {
    if let Some((records, status)) = collect_dsh_tokenlens() {
        return (records, status);
    }
    collect_dsh_journal()
}

/// TokenLens cache path (primary DSH source).
fn collect_dsh_tokenlens() -> Option<(Vec<UsageRecord>, SourceInfo)> {
    let path = paths::dsh_tokenlens_cache();
    let text = fs::read_to_string(&path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let files_map = value.get("files").and_then(|v| v.as_object())?;
    // Aggregate (day, model) → counts across all session files.
    let mut by_day_model: BTreeMap<(String, String), (i64, i64, i64, i64)> = BTreeMap::new();
    for entry in files_map.values() {
        let Some(models) = entry.get("models").and_then(|v| v.as_object()) else {
            continue;
        };
        for m in models.values() {
            let model = m
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("deepseek")
                .to_string();
            let Some(today) = m.get("today").and_then(|v| v.as_object()) else {
                continue;
            };
            for (day, counts) in today {
                let input = counts.get("input").and_then(|v| v.as_i64()).unwrap_or(0);
                let output = counts.get("output").and_then(|v| v.as_i64()).unwrap_or(0);
                let cache_read = counts
                    .get("cacheRead")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let cache_write = counts
                    .get("cacheWrite")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let sum = input + output + cache_read + cache_write;
                if sum <= 0 {
                    continue;
                }
                let slot = by_day_model
                    .entry((day.clone(), model.clone()))
                    .or_insert((0, 0, 0, 0));
                slot.0 += input;
                slot.1 += output;
                slot.2 += cache_read;
                slot.3 += cache_write;
            }
        }
    }
    let mut records = Vec::new();
    for ((day, model), (input, output, cache_read, cache_write)) in &by_day_model {
        let usage = crate::agent_sources::canonical_counts(
            *input,
            *output,
            *cache_read,
            *cache_write,
            0,
            false,
            None,
        );
        records.push(UsageRecord {
            date: day.clone(),
            timestamp: None, // day-level only (the cache also has hours data; unused for now)
            tool: "DSH".to_string(),
            model: model.clone(),
            usage,
            cost_usd: None,
        });
    }
    let status = if records.is_empty() {
        "missing_valid_rows"
    } else {
        "ok"
    };
    let count = records.len() as i64;
    Some((
        records,
        SourceInfo {
            status: Some(status.into()),
            files: Some(files_map.len() as i64),
            records: Some(count),
            ..Default::default()
        },
    ))
}

/// Billing-journal fallback (rebuilt 2026-11-16; per-call totals only).
fn collect_dsh_journal() -> (Vec<UsageRecord>, SourceInfo) {
    let path = paths::dsh_usage_json();
    let Ok(text) = fs::read_to_string(&path) else {
        return (
            vec![],
            SourceInfo {
                status: Some("missing".into()),
                ..Default::default()
            },
        );
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (
            vec![],
            SourceInfo {
                status: Some("unreadable".into()),
                ..Default::default()
            },
        );
    };
    let Some(events) = value.get("events").and_then(|v| v.as_array()) else {
        return (
            vec![],
            SourceInfo {
                status: Some("no_events".into()),
                ..Default::default()
            },
        );
    };
    let mut records = Vec::new();
    for ev in events {
        let ts_ms = ev.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let tokens = ev.get("tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        if ts_ms <= 0 || tokens <= 0 {
            continue;
        }
        let model = ev
            .get("model")
            .and_then(|v| v.as_str())
            .filter(|m| !m.is_empty())
            .unwrap_or("deepseek")
            .to_string();
        let date = ev
            .get("day")
            .and_then(|v| v.as_str())
            .filter(|d| !d.is_empty())
            .map(String::from)
            .unwrap_or_else(|| day_string_from_epoch(ts_ms as f64 / 1000.0));
        let ts = iso_string_from_epoch(ts_ms as f64 / 1000.0);
        records.push(UsageRecord {
            date,
            timestamp: Some(ts),
            tool: "DSH".to_string(),
            model,
            usage: TokenUsageCounts {
                input_tokens: tokens,
                output_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: tokens,
            },
            cost_usd: None,
        });
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
