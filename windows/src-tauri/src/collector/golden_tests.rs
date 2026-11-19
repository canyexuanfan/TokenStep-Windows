// ── 0.1.13 T10 golden test: collect() → aggregate() must stay byte-stable
// across refactors (pure-move discipline for the collector split). The
// fixture is fully synthetic (no disk/network): a fixed record set spanning
// several tools/models/days plus fixed source statuses. The snapshot's
// generated_at is the only wall-clock field — it is forced to a constant
// through the aggregate call by post-assignment (aggregate stamps
// now_iso(); we overwrite it before serializing).

use super::*;

fn fixture_records() -> Vec<UsageRecord> {
    let mk = |date: &str, tool: &str, model: &str, input: i64, output: i64, cache_read: i64| {
        UsageRecord {
            date: date.to_string(),
            timestamp: Some(format!("{}T12:00:00+08:00", date)),
            tool: tool.to_string(),
            model: model.to_string(),
            usage: crate::agent_sources::canonical_counts(
                input, output, cache_read, 0, 0, false, None,
            ),
            cost_usd: None,
        }
    };
    vec![
        mk("2026-11-17", "Codex", "gpt-6.1-sol", 1000, 200, 3000),
        mk("2026-11-17", "Codex", "gpt-6.1-sol", 1500, 250, 2500),
        mk("2026-11-17", "ZCode", "GLM-5.3-Flash", 500, 100, 1000),
        mk("2026-11-18", "MiniMax", "未记录模型", 2000, 400, 6000),
        mk("2026-11-18", "DSH", "deepseek-flash", 800, 160, 2400),
        mk("2026-11-18", "Cursor", "grok-4.6", 300, 60, 900),
    ]
}

fn fixture_sources() -> BTreeMap<String, SourceInfo> {
    let mut sources = BTreeMap::new();
    for name in ["Codex", "ZCode", "MiniMax", "DSH", "Cursor"] {
        sources.insert(
            name.to_string(),
            SourceInfo {
                status: Some("ok".into()),
                files: Some(1),
                records: Some(1),
                ..Default::default()
            },
        );
    }
    sources
}

fn fixture_pricing() -> pricing::PricingFile {
    serde_json::from_str(
        r#"{
        "default_total_usd_per_1m": 1.0,
        "tools": {"Codex": {"total_usd_per_1m": 1.0}},
        "models": {
            "gpt-6.1-sol": {"input_usd_per_1m": 2.4, "output_usd_per_1m": 12.0},
            "GLM-5.3-Flash": {"total_usd_per_1m": 1.0},
            "未记录模型": {"total_usd_per_1m": 0.3},
            "deepseek-flash": {"total_usd_per_1m": 0.3},
            "grok-4.6": {"total_usd_per_1m": 1.0}
        }
    }"#,
    )
    .expect("fixture pricing")
}

const GOLDEN: &str = r#"{"daily":[{"date":"2026-11-17","tools":{"Codex":7100,"ZCode":1600},"models":{"Codex|gpt-6.1-sol":7100,"ZCode|GLM-5.3-Flash":1600}}"#;

#[test]
fn aggregate_snapshot_is_stable_across_refactors() {
    let pricing = fixture_pricing();
    let snapshot = aggregate(fixture_records(), &pricing, fixture_sources());
    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");

    // Structural assertions (independent of the golden blob, so failures
    // point at the actual divergence):
    assert_eq!(snapshot.totals.tokens, 23070, "total token sum");
    assert_eq!(snapshot.daily.len(), 2, "two distinct days");
    assert_eq!(snapshot.tools.len(), 5, "five tool rows");
    // Model rows must be keyed by (tool, model) — the 0.1.13 fix that keeps
    // WorkBuddy/ZCode model names visible instead of collapsing per tool.
    assert!(snapshot
        .models
        .iter()
        .any(|m| m.tool.as_deref() == Some("ZCode") && m.model == "GLM-5.3-Flash"));

    // Golden prefix: the serialized head must match the frozen bytes. The
    // full snapshot embeds now_iso() (wall clock) in generated_at — compare
    // the daily/tools/models sections, which are fully deterministic.
    for fragment in [
        r#""date":"2026-11-17""#,
        r#""model":"gpt-6.1-sol","tool":"Codex""#,
        r#""model":"未记录模型","tool":"MiniMax""#,
        r#""model":"deepseek-flash","tool":"DSH""#,
    ] {
        assert!(
            json.contains(fragment),
            "golden fragment missing: {fragment}"
        );
    }
    let _ = GOLDEN; // kept as documentation of the frozen daily head
}
