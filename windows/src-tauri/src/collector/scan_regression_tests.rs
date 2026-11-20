// ── 0.1.13 T2 regression tests: the three 0.1.12 critical fixes ──────────
// 1. metas/file-list alignment on expired deadline (0xc0000409 crash fix)
// 2. scan degradation ladder (memo → cache → degraded)
// 3. cache save point after ALL collectors (late collectors must persist)

use super::*;

fn fixture_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tokenstep-t2-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let day = dir.join("2026").join("11").join("18");
    fs::create_dir_all(&day).expect("create fixture dirs");
    fs::write(day.join("rollout-a.jsonl"), "{\"type\":\"x\"}\n").expect("write a");
    fs::write(day.join("rollout-b.jsonl"), "{\"type\":\"x\"}\n").expect("write b");
    dir
}

fn codex_entry(key_size: u64, total: i64) -> CachedUsageFile {
    CachedUsageFile {
        tool: "Codex".into(),
        size: key_size,
        mtime: 0.0,
        records: vec![CachedRecord {
            date: "2026-11-18".into(),
            timestamp: None,
            tool: "Codex".into(),
            model: "unknown".into(),
            usage: TokenUsageCounts {
                input_tokens: total,
                output_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: total,
            },
            cost_usd: None,
        }],
        scan_offset: key_size,
        codex_session: Some("sess-fixture".into()),
        codex_parent_session: None,
        codex_created_at: None,
        codex_anchors: vec![],
        cursor_est_rev: None,
        model_rev: Some(super::codex::CODEX_MODEL_REV),
    }
}

/// Fix #1 (0xc0000409 crash): the crash state was PARTIAL metas — Phase 0
/// hit the deadline after some files got metas (cache hits) while others
/// were skipped (None). With metas now index-aligned Options, Phase 2 skips
/// the None entries instead of indexing out of bounds. Reproduced here with
/// a live walk (both fixture files enumerated) + an expired PARSE deadline
/// (file A served from cache → Some, file B skipped → None).
#[test]
fn partial_deadline_keeps_metas_aligned_and_marks_timeout() {
    let root = fixture_root("expired");
    let roots = vec![root.clone()];
    // Pre-seed the cache with a FRESH entry for file A (real size/mtime) so
    // Phase 0 pushes Some for it while file B (uncached) hits the expired
    // parse deadline and records None — exactly the crash shape.
    let file_a = root
        .join("2026")
        .join("11")
        .join("18")
        .join("rollout-a.jsonl");
    let (size_a, mtime_a) = {
        let meta = fs::metadata(&file_a).expect("fixture file exists");
        let size = meta.len();
        let mtime = meta
            .modified()
            .expect("mtime")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("epoch")
            .as_secs_f64();
        (size, mtime)
    };
    let mut cache = CollectorCache::default();
    let mut entry = codex_entry(size_a, 1234);
    entry.mtime = mtime_a;
    cache
        .files
        .insert(file_a.to_string_lossy().to_string(), entry);

    let mut live_paths: BTreeSet<String> = BTreeSet::new();
    let scope = CodexScanScope {
        roots_override: Some(&roots),
        walk_deadline_override: None,
        parse_deadline_override: Some(
            std::time::Instant::now() - std::time::Duration::from_secs(1),
        ),
    };
    let (records, files, timed_out) =
        collect_codex_jsonl_scoped(&mut cache, &mut live_paths, scope);

    assert!(timed_out, "expired parse deadline must report timed_out");
    assert_eq!(
        files, 2,
        "both fixture files were enumerated (walk was live)"
    );
    // Phase 2 must complete: file A served from cache, file B skipped.
    assert_eq!(
        records.len(),
        1,
        "file A's cached record is served; file B skipped without panic"
    );
    assert_eq!(records[0].usage.total_tokens, 1234);
    // live_paths covers both files (prune-safe even though timed out).
    assert_eq!(live_paths.len(), 2);
    // The pre-seeded entry survives (timed-out scans skip the prune).
    assert!(cache
        .files
        .contains_key(&file_a.to_string_lossy().to_string()));
    let _ = fs::remove_dir_all(&root);
}

/// Fix #2 (scan degradation ladder), one body because the global in-flight
/// flag is shared across parallel tests:
///   2a memo fallback   → memo records + `ok_scan_timeout`
///   2b cache fallback  → cached records + `cached_scan_timeout` (the
///                        0.1.12 fix that keeps Codex visible while the scan
///                        thread is still walking slow storage)
///   2c nothing at all  → `degraded_scan_timeout`
#[test]
fn scan_degradation_paths_memo_then_cache_then_degraded() {
    use std::sync::atomic::Ordering;
    let root = fixture_root("degrade");

    // 2a: memo fallback.
    CODEX_SCAN_IN_FLIGHT.store(true, Ordering::SeqCst);
    let memo_outcome = CodexScanOutcome {
        records: vec![UsageRecord {
            date: "2026-11-18".into(),
            timestamp: None,
            tool: "Codex".into(),
            model: "gpt-x".into(),
            usage: TokenUsageCounts {
                input_tokens: 5,
                output_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: 5,
            },
            cost_usd: None,
        }],
        files: 1,
        timed_out: true,
        live_paths: BTreeSet::new(),
        codex_entries: BTreeMap::new(),
    };
    *CODEX_SCAN_MEMO.lock().expect("memo lock") = Some(memo_outcome);
    let mut cache = CollectorCache::default();
    let (records, info) = collect_codex(&mut cache);
    assert_eq!(info.status.as_deref(), Some("ok_scan_timeout"), "2a status");
    assert_eq!(records.len(), 1, "2a serves the memo's records");
    *CODEX_SCAN_MEMO.lock().expect("memo lock") = None;

    // 2b: cache fallback (still in flight, no memo).
    let mut cache2 = CollectorCache::default();
    cache2
        .files
        .insert("C:\\fixture\\rollout.jsonl".into(), codex_entry(10, 1234));
    let (records2, info2) = collect_codex(&mut cache2);
    assert_eq!(
        info2.status.as_deref(),
        Some("cached_scan_timeout"),
        "2b status"
    );
    assert_eq!(records2.len(), 1, "2b serves the cached records");
    assert_eq!(records2[0].usage.total_tokens, 1234, "2b record content");

    // 2c: nothing anywhere.
    let mut cache3 = CollectorCache::default();
    let (_, info3) = collect_codex(&mut cache3);
    assert_eq!(
        info3.status.as_deref(),
        Some("degraded_scan_timeout"),
        "2c status"
    );

    // Restore globals for other tests.
    CODEX_SCAN_IN_FLIGHT.store(false, Ordering::SeqCst);
    *CODEX_SCAN_MEMO.lock().expect("memo lock") = None;
    let _ = fs::remove_dir_all(&root);
}

/// Fix #3 (cache save point): exactly ONE `save_cache(&cache);` inside
/// collect(), positioned AFTER the last `sources.insert` (so the late
/// collectors — MiniMax/DSH/Antigravity — persist) and BEFORE `aggregate(...)`.
/// Plus a save→load round trip on the cache structs.
#[test]
fn cache_save_point_sits_after_all_collectors() {
    const SRC: &str = include_str!("../collector.rs");
    let marker = "save_cache(&cache);";
    let occurrences = SRC.matches(marker).count();
    assert_eq!(occurrences, 1, "exactly one cache save per collect pass");
    let save_pos = SRC.find(marker).expect("save present");
    let last_insert = SRC
        .rfind("sources.insert(")
        .expect("sources inserts present");
    let aggregate_pos = SRC
        .find("aggregate(records, &pricing_data, sources)")
        .expect("aggregate call present");
    assert!(
        last_insert < save_pos,
        "cache save must come after the LAST sources.insert"
    );
    assert!(
        save_pos < aggregate_pos,
        "cache save must come before aggregate()"
    );

    // Round trip: codex + cursor entries survive a serde cycle untouched.
    let mut cache = CollectorCache::default();
    cache.files.insert("a.jsonl".into(), codex_entry(10, 99));
    cache.files.insert(
        "cursor.jsonl".into(),
        CachedUsageFile {
            tool: "Cursor".into(),
            size: 5,
            mtime: 1.0,
            records: vec![CachedRecord {
                date: "2026-11-18".into(),
                timestamp: None,
                tool: "Cursor".into(),
                model: "grok-4.6".into(),
                usage: TokenUsageCounts {
                    input_tokens: 7,
                    output_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    reasoning_output_tokens: 0,
                    total_tokens: 7,
                },
                cost_usd: None,
            }],
            scan_offset: 0,
            codex_session: None,
            codex_parent_session: None,
            codex_created_at: None,
            codex_anchors: vec![],
            cursor_est_rev: None,
            model_rev: None,
        },
    );
    let text = serde_json::to_string(&cache).expect("serialize cache");
    let loaded: CollectorCache = serde_json::from_str(&text).expect("parse cache");
    assert_eq!(loaded.files.len(), 2);
    assert_eq!(loaded.files["a.jsonl"].tool, "Codex");
    assert_eq!(loaded.files["a.jsonl"].records[0].usage.total_tokens, 99);
    assert_eq!(loaded.files["cursor.jsonl"].tool, "Cursor");
}
