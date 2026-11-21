//! codex collector — split from collector.rs in 0.1.13 T10 (pure move, golden-test guarded).

use super::*;

/// Model-attribution revision: bumped when model attribution changes so
/// cached entries re-parse. rev 2 (0.1.13): rev-1 entries were written by
/// the incremental path BEFORE it learned to skip stale-rev entries — they
/// kept legacy "unknown" models from the old parse, so they must re-parse
/// under the fixed rules (file-level opening-model fallback + new-format
/// token_usage_record support).
pub(crate) const CODEX_MODEL_REV: i64 = 2;

/// Parse the recently-ACTIVE rollout files on the calling thread, before the
/// background scan spawns. Long-running sessions keep appending rows to
/// files created days earlier — scanning only today's directory misses them
/// entirely, and the full backlog re-parse on the slow sessions volume lags
/// hours behind, so recent-day stats would stay wrong for a long time.
/// Candidate files come from the cached index (plus today's directory),
/// ranked by current mtime, newest first, capped. Usage rows are bucketed by
/// their own line timestamp (the true activity day), not the file's
/// directory date. Only new-format `token_usage_record` rows are handled
/// (today's CLI writes these); anything else falls through to the
/// background scan. Writes cache entries ONLY — the normal flow serves the
/// records back from the cache, so there is exactly one attribution point
/// per file and no double counting.
#[cfg_attr(test, allow(dead_code))]
fn codex_quick_recent(cache: &mut CollectorCache, max_files: usize, max_total_bytes: u64) {
    const RECENT_WINDOW_SECS: f64 = 48.0 * 3600.0;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let mut candidates: Vec<String> = cache
        .files
        .iter()
        .filter(|(_, e)| e.tool == "Codex")
        .map(|(k, _)| k.clone())
        .collect();
    let local_day = chrono::Utc::now()
        .with_timezone(&local_tz())
        .format("%Y-%m-%d")
        .to_string();
    let seg: Vec<&str> = local_day.split('-').collect();
    if seg.len() == 3 {
        let mut root = match paths::codex_jsonl_roots().into_iter().next() {
            Some(r) => r,
            None => return,
        };
        for p in &seg {
            root.push(p);
        }
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    let key = path.to_string_lossy().to_string();
                    if !candidates.contains(&key) {
                        candidates.push(key);
                    }
                }
            }
        }
    }
    let mut active: Vec<(f64, u64, PathBuf)> = Vec::new();
    for key in &candidates {
        let path = PathBuf::from(key);
        let Some((size, mtime)) = file_meta(&path) else {
            continue;
        };
        if now - mtime > RECENT_WINDOW_SECS {
            continue;
        }
        active.push((mtime, size, path));
    }
    active.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut budget = max_total_bytes;
    for (mtime, size, path) in active.into_iter().take(max_files) {
        if size > budget {
            continue;
        }
        budget -= size;
        let key = path.to_string_lossy().to_string();
        if let Some(e) = cache.files.get(&key) {
            if e.model_rev == Some(CODEX_MODEL_REV)
                && e.size == size
                && (e.mtime - mtime).abs() < 0.001
                && e.scan_offset == size
                && !e.records.is_empty()
            {
                continue;
            }
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let fallback_day = day_string_from_epoch(mtime);
        let mut session_id = fallback_session_from_path(&path);
        let mut final_model = String::from("unknown");
        let mut records: Vec<UsageRecord> = Vec::new();
        for line in text.lines() {
            if !line.contains("token_usage_record")
                && !line.contains("turn_context")
                && !line.contains("session_meta")
            {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let ev_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match ev_type {
                "session_meta" => {
                    if let Some(id) = obj.pointer("/payload/id").and_then(|v| v.as_str()) {
                        if !id.is_empty() {
                            session_id = id.to_string();
                        }
                    }
                }
                "turn_context" => {
                    if let Some(m) = obj.pointer("/payload/model").and_then(|v| v.as_str()) {
                        if !m.is_empty() {
                            final_model = model_key(m);
                        }
                    }
                }
                "token_usage_record" => {
                    if let Some(rec) = codex_new_format_record(&obj, &final_model, &fallback_day) {
                        records.push(rec);
                    }
                }
                _ => {}
            }
        }
        if records.iter().any(|r| r.model == "unknown") {
            if let Some(opening) = records
                .iter()
                .find(|r| r.model != "unknown")
                .map(|r| r.model.clone())
            {
                for r in records.iter_mut() {
                    if r.model == "unknown" {
                        r.model = opening.clone();
                    }
                }
            }
        }
        cache.files.insert(
            key,
            CachedUsageFile {
                tool: "Codex".into(),
                size,
                mtime,
                records: records.iter().map(CachedRecord::from).collect(),
                scan_offset: size,
                codex_session: Some(session_id),
                codex_parent_session: None,
                codex_created_at: None,
                codex_anchors: Vec::new(),
                cursor_est_rev: None,
                model_rev: Some(CODEX_MODEL_REV),
            },
        );
    }
}

pub(crate) fn collect_codex(cache: &mut CollectorCache) -> (Vec<UsageRecord>, SourceInfo) {
    // Primary: JSONL rollouts (per-turn token counts), matching the Python
    // collector's precedence. SQLite `threads` (per-thread totals) is only a
    // fallback when no JSONL data exists. The cache is shared with the other
    // collectors (single load/save per pass — fixing the historic clobber
    // where each collector's save wiped the others' entries).
    //
    // Recently-active rollouts first (fast, calling thread) so recent-day
    // stats never depend on the slow backlog scan; the spawned thread then
    // skips those files via the rev gate. Skipped in test builds — it reads
    // the real sessions volume and would pollute fixture caches.
    #[cfg(not(test))]
    codex_quick_recent(cache, 24, 256 * 1024 * 1024);
    // The scan itself runs on a detached thread with a private cache clone:
    // every cached_records() freshness check stats files on the (possibly
    // glacial) sessions volume, which must never block the refresh pass.
    // Completed outcomes are memoized; a timed-out wait serves the memo.
    let job_files = cache.files.clone();
    let (tx, rx) = std::sync::mpsc::channel::<CodexScanOutcome>();
    let mut spawn_ok = false;
    if !CODEX_SCAN_IN_FLIGHT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        spawn_ok = std::thread::Builder::new()
            .name("codex-scan".into())
            .spawn(move || {
                let mut local_cache = CollectorCache {
                    version: COLLECTOR_CACHE_VERSION,
                    files: job_files,
                    minimax_session_models: BTreeMap::new(),
                };
                let mut live_paths = BTreeSet::new();
                let (records, files, timed_out) =
                    collect_codex_jsonl(&mut local_cache, &mut live_paths);
                let codex_entries: BTreeMap<String, CachedUsageFile> = local_cache
                    .files
                    .into_iter()
                    .filter(|(_, e)| e.tool == "Codex")
                    .collect();
                let outcome = CodexScanOutcome {
                    records,
                    files,
                    timed_out,
                    live_paths,
                    codex_entries,
                };
                if let Ok(mut memo) = CODEX_SCAN_MEMO.lock() {
                    *memo = Some(outcome.clone());
                }
                let _ = tx.send(outcome);
                CODEX_SCAN_IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
            })
            .is_ok();
    }
    let outcome = if spawn_ok {
        match rx.recv_timeout(CODEX_SCAN_WAIT) {
            Ok(o) => Some(o),
            Err(_) => CODEX_SCAN_MEMO.lock().ok().and_then(|m| m.clone()),
        }
    } else {
        // A scan is already in flight (or spawn failed): serve the memo.
        CODEX_SCAN_MEMO.lock().ok().and_then(|m| m.clone())
    };

    let Some(outcome) = outcome else {
        // Scan unavailable (first pass still stuck on slow storage): serve
        // the records already parsed into the shared cache so Codex doesn't
        // vanish from the stats while the scan thread keeps working. Only
        // rev-2 entries are served — older entries predate the current
        // attribution rules (mixed legacy over-counting / unparsed rows);
        // their sessions are filled from the threads ledger instead.
        let mut records = Vec::new();
        let mut files = 0i64;
        for entry in cache
            .files
            .values()
            .filter(|e| e.tool == "Codex" && e.model_rev == Some(CODEX_MODEL_REV))
        {
            files += 1;
            for c in &entry.records {
                records.push(UsageRecord {
                    date: c.date.clone(),
                    timestamp: c.timestamp.clone(),
                    tool: c.tool.clone(),
                    model: c.model.clone(),
                    usage: c.usage.clone(),
                    cost_usd: c.cost_usd,
                });
            }
        }
        let status = if records.is_empty() {
            "degraded_scan_timeout"
        } else {
            "cached_scan_timeout"
        };
        let count = records.len() as i64;
        return (
            records,
            SourceInfo {
                status: Some(status.into()),
                files: Some(files),
                records: Some(count),
                ..Default::default()
            },
        );
    };

    // Merge the scanned codex entries into the shared cache. A timed-out scan
    // saw only part of the tree: its live_paths are incomplete, so pruning
    // cached entries would delete valid history — skip the prune then.
    if !outcome.timed_out {
        let stale: Vec<String> = cache
            .files
            .iter()
            .filter(|(k, entry)| entry.tool == "Codex" && !outcome.live_paths.contains(k.as_str()))
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            cache.files.remove(&k);
        }
    }
    for (k, entry) in outcome.codex_entries {
        cache.files.insert(k, entry);
    }

    let scan_timed_out = outcome.timed_out;
    let files = outcome.files;
    if !outcome.records.is_empty() {
        let records = outcome.records;
        return (
            records,
            SourceInfo {
                status: Some(if scan_timed_out {
                    "ok_scan_timeout".to_string()
                } else {
                    "ok".to_string()
                }),
                files: Some(files),
                records: None,
                ..Default::default()
            },
        );
    }

    // Fallback: SQLite `threads` table (per-thread totals).
    if let Some((records, files)) = collect_codex_sqlite() {
        if !records.is_empty() {
            return (
                records,
                SourceInfo {
                    status: Some(if scan_timed_out {
                        "ok_sqlite_scan_timeout".to_string()
                    } else {
                        "ok_sqlite".to_string()
                    }),
                    files: Some(files as i64),
                    records: None,
                    ..Default::default()
                },
            );
        }
    }

    (
        Vec::new(),
        SourceInfo {
            status: Some(if scan_timed_out {
                "degraded_scan_timeout".to_string()
            } else {
                "missing".to_string()
            }),
            files: Some(files),
            records: None,
            ..Default::default()
        },
    )
}

/// Read Codex usage from the `threads` table. `tokens_used` is a per-thread
/// total — the value the macOS app surfaces when JSONL has no token data.
fn collect_codex_sqlite() -> Option<(Vec<UsageRecord>, usize)> {
    let db_path = paths::codex_sqlite_candidates()
        .into_iter()
        .find(|p| p.exists())?;
    // Read-only open; safe even while Codex is running.
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    let mut stmt = conn
        .prepare("select created_at, model, tokens_used from threads where tokens_used > 0")
        .ok()?;

    let rows = stmt
        .query_map([], |row| {
            let created_at: rusqlite::types::Value = row.get(0)?;
            let model: rusqlite::types::Value = row.get(1)?;
            let tokens: rusqlite::types::Value = row.get(2)?;
            Ok((created_at, model, tokens))
        })
        .ok()?;

    let mut records = Vec::new();
    for row in rows.flatten() {
        let (created_at, model, tokens) = row;
        let tokens = sqlite_value_as_f64(&tokens);
        if tokens <= 0.0 {
            continue;
        }
        let day = match sqlite_value_as_epoch_day(&created_at) {
            Some(d) => d,
            None => continue,
        };
        let usage = TokenUsageCounts {
            total_tokens: tokens as i64,
            ..Default::default()
        };
        records.push(UsageRecord {
            date: day,
            timestamp: None, // Codex SQLite threads table has no sub-day resolution
            tool: "Codex".to_string(),
            model: model_key(&sqlite_value_as_string(&model)),
            usage,
            cost_usd: None,
        });
    }

    Some((records, 1))
}

/// Scan budget for the Codex JSONL source. The sessions root can be a
/// junction onto a pathologically slow volume (41 GB tree, metadata-only
/// walks take minutes — and the first full pass streams every rollout). The
/// scan runs on a DETACHED thread and never blocks the refresh pass (which
/// waits only `CODEX_SCAN_WAIT` = 20s before serving the memo/cache), so the
/// budget only needs to bound the background thread, not the UI. 30s here
/// made the scan abort mid-walk EVERY pass on slow storage (walk alone >
/// 2 min) and Codex sat in `cached_scan_timeout` forever; 30 min lets it
/// actually finish, after which the memo carries a complete, untimed-out
/// result and the source reports plain "ok".
/// Scan budget for the detached codex-scan thread (0.1.13: kept at 30min —
/// on a slow volume the budget bounds each pass, and results merge into the
/// cache when the pass ends, so a shorter budget means today's files (parsed
/// first under newest-first ordering) reach the stats sooner; already-parsed
/// files skip via the model_rev gate + incremental fast path on later
/// passes, so the backlog converges over consecutive passes).
const CODEX_SCAN_BUDGET: std::time::Duration = std::time::Duration::from_secs(1800);

/// TEMP-DIAG (codex scan): heartbeat + completion diagnostics written to
/// cache/codex-scan-progress.json so a slow cold scan is observable.
fn codex_scan_diag_write(payload: serde_json::Value) {
    let path = paths::collector_cache_json()
        .parent()
        .map(|d| d.join("codex-scan-progress.json"));
    if let Some(path) = path {
        let _ = fs::write(&path, serde_json::to_string(&payload).unwrap_or_default());
    }
}

fn collect_codex_jsonl(
    cache: &mut CollectorCache,
    live_paths: &mut BTreeSet<String>,
) -> (Vec<UsageRecord>, i64, bool) {
    collect_codex_jsonl_scoped(cache, live_paths, CodexScanScope::default())
}

/// Testability seam for the Codex JSONL scan (0.1.13 T2): production callers
/// use `Default` (real roots + the standard budget deadline). Regression
/// tests inject a fixture root plus SEPARATE walk/parse deadlines — an
/// expired PARSE deadline with a live WALK deadline reproduces the 0.1.12
/// crash state (files enumerated, then Phase 0 skips uncached files, metas
/// go partially None) without touching the user's sessions directory.
#[derive(Clone, Copy, Default)]
pub(crate) struct CodexScanScope<'a> {
    pub(crate) roots_override: Option<&'a [PathBuf]>,
    pub(crate) walk_deadline_override: Option<std::time::Instant>,
    pub(crate) parse_deadline_override: Option<std::time::Instant>,
}

/// Fork metadata per rollout file: (key, own session, parent session,
/// created_at epoch secs). Kept as a type alias - the metas list must stay
/// index-aligned with the enumerated file list (0.1.12 crash fix).
type CodexFileMeta = (String, String, Option<String>, Option<f64>);

pub(crate) fn collect_codex_jsonl_scoped(
    cache: &mut CollectorCache,
    live_paths: &mut BTreeSet<String>,
    scope: CodexScanScope<'_>,
) -> (Vec<UsageRecord>, i64, bool) {
    let scan_started = std::time::Instant::now();
    let walk_deadline = scope
        .walk_deadline_override
        .unwrap_or_else(|| scan_started + CODEX_SCAN_BUDGET);
    let scan_deadline = scope.parse_deadline_override.unwrap_or(walk_deadline);
    codex_scan_diag_write(serde_json::json!({
        "phase": "walk_start",
        "elapsed_ms": 0,
    }));
    let roots: Vec<PathBuf> = scope
        .roots_override
        .map(|r| r.to_vec())
        .unwrap_or_else(paths::codex_jsonl_roots);
    let mut all_paths = Vec::new();
    let mut scan_timed_out = false;
    for root in &roots {
        let (mut files, timed_out) = jsonl_files_under_budget(root, walk_deadline);
        scan_timed_out |= timed_out;
        all_paths.append(&mut files);
    }
    all_paths.sort();

    let mut records: Vec<UsageRecord> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    // ── Phase 0: per-file fork metadata (own session / parent / created_at) ──
    // session_meta sits near the head of each rollout, so only the first
    // chunk of every file is read here. Metas stay INDEX-ALIGNED with
    // all_paths: deadline-exceeded files push None (skipped in later phases).
    // (A plain Vec<..> here once desynced from all_paths and Phase 2 indexed
    // metas[index] out of bounds → panic=abort killed the whole app.)
    let mut metas: Vec<Option<CodexFileMeta>> = Vec::with_capacity(all_paths.len());
    for path in &all_paths {
        let key = path.to_string_lossy().to_string();
        live_paths.insert(key.clone());
        let cached_meta = cache.files.get(&key).filter(|e| {
            let Some((size, mtime)) = file_meta(path) else {
                return false;
            };
            entry_is_fresh(entry_meta(e), size, mtime)
        });
        if let Some(entry) = cached_meta {
            let session = entry
                .codex_session
                .clone()
                .unwrap_or_else(|| fallback_session_from_path(path));
            metas.push(Some((
                key,
                session,
                entry.codex_parent_session.clone(),
                entry.codex_created_at,
            )));
            continue;
        }
        if std::time::Instant::now() >= scan_deadline {
            scan_timed_out = true;
            metas.push(None);
            continue;
        }
        let (session, parent, created) = codex_head_meta(path);
        metas.push(Some((key, session, parent, created)));
    }

    // Parent sessions that at least one live file forks from.
    let parent_ids: BTreeSet<String> = metas
        .iter()
        .flatten()
        .filter_map(|(_, _, parent, _)| parent.clone())
        .collect();

    // ── Phase 1: anchors keyed by the file's OWN session id (children look
    // their parent up here). Only files referenced as a parent need anchors.
    let mut anchors_by_session: BTreeMap<String, Vec<CachedAnchor>> = BTreeMap::new();
    for (index, meta) in metas.iter().enumerate() {
        let Some((key, own_session, _, _)) = meta else {
            continue;
        };
        if !parent_ids.contains(own_session) {
            continue;
        }
        let path = &all_paths[index];
        if let Some(entry) = cache.files.get(key) {
            if entry_is_fresh_by_meta(entry, path) {
                anchors_by_session.insert(own_session.clone(), entry.codex_anchors.clone());
                continue;
            }
        }
        // Uncached parent file: extract its anchors by streaming.
        if std::time::Instant::now() >= scan_deadline {
            scan_timed_out = true;
            continue;
        }
        let anchors = codex_scan_anchors(path);
        anchors_by_session.insert(own_session.clone(), anchors);
    }

    // ── Phase 2 ordering: newest sessions first ──────────────────────────
    // Lexicographic order puts today's rollouts at the tail of a multi-hour
    // re-parse queue (slow volume + budget-bounded passes), so today's usage
    // would stay missing from the stats for hours. Order by the session
    // created_at already extracted in Phase 0 — zero extra I/O; sessions
    // without a stamp sort last.
    {
        let mut order: Vec<usize> = (0..all_paths.len()).collect();
        order.sort_by(|&a, &b| {
            let ta = metas[a]
                .as_ref()
                .and_then(|m| m.3)
                .unwrap_or(f64::NEG_INFINITY);
            let tb = metas[b]
                .as_ref()
                .and_then(|m| m.3)
                .unwrap_or(f64::NEG_INFINITY);
            tb.partial_cmp(&ta).unwrap_or(std::cmp::Ordering::Equal)
        });
        all_paths = order.iter().map(|&i| all_paths[i].clone()).collect();
        metas = order.iter().map(|&i| metas[i].clone()).collect();
    }

    // ── Phase 2: per-file scan + delta (fork files inherit the parent anchor) ──
    codex_scan_diag_write(serde_json::json!({
        "phase": "parse",
        "files_total": all_paths.len(),
        "elapsed_ms": scan_started.elapsed().as_millis(),
    }));
    for (index, path) in all_paths.iter().enumerate() {
        let Some((key, own_session, parent, created_at)) = &metas[index] else {
            continue;
        };
        let key = key.clone();
        let (own_session, parent, created_at) = (own_session.clone(), parent.clone(), *created_at);
        if index % 5 == 0 || index + 1 == all_paths.len() {
            codex_scan_diag_write(serde_json::json!({
                "phase": "parse",
                "files_total": all_paths.len(),
                "file_index": index,
                "current": key,
                "cached": cache.files.get(&key).map(|e| !e.records.is_empty()).unwrap_or(false),
                "size": file_meta(path).map(|m| m.0).unwrap_or(0),
                "elapsed_ms": scan_started.elapsed().as_millis(),
            }));
        }

        let model_rev_ok = cache
            .files
            .get(&key)
            .map(|e| e.model_rev == Some(CODEX_MODEL_REV))
            .unwrap_or(false);
        if model_rev_ok {
            if let Some(cached) = cached_records(cache, &key, "Codex", path) {
                records.extend(cached);
                // Publish this cached file's anchors for its children.
                if parent_ids.contains(&own_session) {
                    if let Some(entry) = cache.files.get(&key) {
                        anchors_by_session.insert(own_session.clone(), entry.codex_anchors.clone());
                    }
                }
                continue;
            }
        }

        // ── Incremental tail read (0.1.12) ────────────────────────────────
        // A file that merely APPENDED since its last parse only needs its
        // tail: multi-GB active rollouts on slow storage made every full
        // re-scan take 15+ minutes, keeping Codex in scan-timeout
        // degradation forever. Continuity: a virtual head event carrying the
        // cached tail anchor's cumulative makes codex_delta_records resume
        // deltas exactly after the last parsed event.
        let mut incremental_done = false;
        'incremental: {
            let Some(entry) = cache.files.get(&key).cloned() else {
                break 'incremental;
            };
            if entry.records.is_empty() || entry.scan_offset == 0 {
                break 'incremental; // never fully parsed — full read below
            }
            // Model-attribution rev mismatch: the cached records predate the
            // current attribution rules — force a full re-parse below.
            if entry.model_rev != Some(CODEX_MODEL_REV) {
                break 'incremental;
            }
            let Some((new_size, _new_mtime)) = file_meta(path) else {
                break 'incremental;
            };
            if new_size < entry.scan_offset {
                break 'incremental; // rewritten in place — full read
            }
            let session_id_inc = entry
                .codex_session
                .clone()
                .unwrap_or_else(|| fallback_session_from_path(path));
            let Some(anchor) = entry.codex_anchors.last().cloned().filter(|a| a.total > 0) else {
                break 'incremental;
            };
            if new_size == entry.scan_offset {
                // Nothing appended; refresh mtime so freshness passes.
                if let Some(e) = cache.files.get_mut(&key) {
                    if let Some((_, mtime)) = file_meta(path) {
                        e.mtime = mtime;
                    }
                }
                incremental_done = true;
                break 'incremental;
            }
            let Ok(mut file) = fs::File::open(path) else {
                break 'incremental;
            };
            use std::io::{Seek, SeekFrom};
            if file.seek(SeekFrom::Start(entry.scan_offset)).is_err() {
                break 'incremental;
            }
            let reader = std::io::BufReader::new(file);
            let mut final_model_inc = String::from("unknown");
            let mut events_inc: Vec<CodexTokenEvent> = Vec::new();
            let mut new_format_inc: Vec<UsageRecord> = Vec::new();
            let fallback_day_inc =
                day_string_from_epoch(file_meta(path).map(|m| m.1).unwrap_or(0.0));
            use std::io::BufRead;
            for line in reader.lines().map_while(Result::ok) {
                if !line.contains("token_count")
                    && !line.contains("session_meta")
                    && !line.contains("turn_context")
                    && !line.contains("token_usage_record")
                {
                    continue;
                }
                let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                let ev_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let payload = obj.get("payload");
                if ev_type == "turn_context" {
                    if let Some(m) = payload
                        .and_then(|p| p.get("model"))
                        .and_then(|v| v.as_str())
                    {
                        if !m.is_empty() {
                            final_model_inc = model_key(m);
                        }
                    }
                }
                if ev_type == "token_usage_record" {
                    if let Some(rec) =
                        codex_new_format_record(&obj, &final_model_inc, &fallback_day_inc)
                    {
                        new_format_inc.push(rec);
                    }
                    continue;
                }
                if ev_type != "event_msg" {
                    continue;
                }
                let inner_type = payload.and_then(|p| p.get("type")).and_then(|v| v.as_str());
                if inner_type != Some("token_count") {
                    continue;
                }
                let info = payload.and_then(|p| p.get("info"));
                let cumulative = info
                    .and_then(|i| i.get("total_token_usage"))
                    .map(|v| normalize_usage(Some(v)));
                let last = info
                    .and_then(|i| i.get("last_token_usage"))
                    .map(|v| normalize_usage(Some(v)));
                let model_context_window = info
                    .and_then(|i| i.get("model_context_window"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                events_inc.push(CodexTokenEvent {
                    timestamp: obj
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    model: final_model_inc.clone(),
                    cumulative,
                    last,
                    model_context_window,
                });
            }
            // Virtual head event: cumulative == cached tail anchor → the
            // delta loop matches it as the resume point (start_index 1).
            let head = CodexTokenEvent {
                timestamp: None,
                model: String::new(),
                cumulative: Some(anchor.usage()),
                last: None,
                model_context_window: 0,
            };
            let mut seq = vec![head];
            seq.extend(events_inc.iter().cloned());
            let mut seen_inc: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            let new_records = codex_delta_records(
                &session_id_inc,
                &seq,
                Some(&anchor.usage()),
                None,
                None,
                &mut seen_inc,
            );
            // Old cache records + new tail records → this pass AND the cache.
            let mut all_records: Vec<UsageRecord> = entry
                .records
                .iter()
                .map(|c| UsageRecord {
                    date: c.date.clone(),
                    timestamp: c.timestamp.clone(),
                    tool: c.tool.clone(),
                    model: c.model.clone(),
                    usage: c.usage.clone(),
                    cost_usd: c.cost_usd,
                })
                .collect();
            all_records.extend(new_records.iter().cloned());
            // New-format tail rows carry no model of their own; rows still
            // unknown after the tail state machine continue the session's
            // last known model from the cached records.
            if new_format_inc.iter().any(|r| r.model == "unknown") {
                let last_known = entry
                    .records
                    .iter()
                    .rev()
                    .map(|c| c.model.clone())
                    .find(|m| !m.is_empty() && m != "unknown");
                if let Some(lk) = last_known {
                    for r in new_format_inc.iter_mut() {
                        if r.model == "unknown" {
                            r.model = lk.clone();
                        }
                    }
                }
            }
            all_records.extend(new_format_inc.iter().cloned());
            records.extend(all_records.iter().cloned());
            // Anchors: cached set + thinned new events + the read-tail anchor
            // (guaranteed last so the next increment resumes exactly here).
            let tail_anchor = events_inc.iter().rev().find_map(|e| {
                let u = e.cumulative.as_ref()?;
                if u.total_tokens <= 0 {
                    return None;
                }
                let ts = e
                    .timestamp
                    .as_deref()
                    .and_then(parse_iso)
                    .map(|dt| dt.timestamp() as f64)?;
                Some(CachedAnchor::from_usage(ts, u))
            });
            let mut merged_anchors = entry.codex_anchors.clone();
            merged_anchors.extend(thin_anchors(
                events_inc
                    .iter()
                    .filter_map(|e| {
                        let u = e.cumulative.as_ref()?;
                        if u.total_tokens <= 0 {
                            return None;
                        }
                        let ts = e
                            .timestamp
                            .as_deref()
                            .and_then(parse_iso)
                            .map(|dt| dt.timestamp() as f64)?;
                        Some(CachedAnchor::from_usage(ts, u))
                    })
                    .collect(),
            ));
            if let Some(t) = tail_anchor {
                merged_anchors.push(t);
            }
            update_codex_cache(
                cache,
                &key,
                path,
                &all_records,
                &session_id_inc,
                entry.codex_parent_session.clone(),
                entry.codex_created_at,
                merged_anchors,
            );
            incremental_done = true;
        }
        if incremental_done {
            continue;
        }

        // ── Token accounting calibration rev8 (upstream v0.1.44 1b54e5c) ──
        // A token_count event carries BOTH a cumulative usage
        // (`info.total_token_usage`, monotonically growing per session) and
        // the last turn's snapshot (`info.last_token_usage`). Summing either
        // directly double-counts O(N^2); the per-event usage is the DELTA
        // between consecutive cumulative values (with sentinel / reset
        // handling). Fork files inherit the parent's anchor so the inherited
        // history is counted exactly once.
        if std::time::Instant::now() >= scan_deadline {
            scan_timed_out = true;
            continue;
        }
        let mut session_id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut final_model = String::from("unknown");
        let mut events: Vec<CodexTokenEvent> = Vec::new();
        let mut new_format_records: Vec<UsageRecord> = Vec::new();
        let fallback_day = day_string_from_epoch(file_meta(path).map(|m| m.1).unwrap_or(0.0));

        // Stream the file line-by-line instead of read_to_string, so a 9.7 GB
        // rollout never sits fully in memory (memory optimization, port of
        // macOS commit 170f655). Single line is parsed + dropped each iteration.
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        for line in reader.lines().map_while(Result::ok) {
            // Cheap marker pre-filter, like the Swift scan.
            if !line.contains("token_count")
                && !line.contains("session_meta")
                && !line.contains("turn_context")
                && !line.contains("token_usage_record")
            {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let ev_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let payload = obj.get("payload");

            if ev_type == "session_meta" {
                if let Some(id) = payload.and_then(|p| p.get("id")).and_then(|v| v.as_str()) {
                    if !id.is_empty() {
                        session_id = id.to_string();
                    }
                }
            }
            if ev_type == "turn_context" {
                if let Some(m) = payload
                    .and_then(|p| p.get("model"))
                    .and_then(|v| v.as_str())
                {
                    if !m.is_empty() {
                        final_model = model_key(m);
                    }
                }
            }
            if ev_type == "token_usage_record" {
                // New-format rollouts: per-response usage, no cumulative —
                // each line is its own record (model from the state machine).
                if let Some(rec) = codex_new_format_record(&obj, &final_model, &fallback_day) {
                    new_format_records.push(rec);
                }
                continue;
            }
            if ev_type != "event_msg" {
                continue;
            }
            let inner_type = payload.and_then(|p| p.get("type")).and_then(|v| v.as_str());
            if inner_type != Some("token_count") {
                continue;
            }
            let info = payload.and_then(|p| p.get("info"));
            let cumulative = info
                .and_then(|i| i.get("total_token_usage"))
                .map(|v| normalize_usage(Some(v)));
            let last = info
                .and_then(|i| i.get("last_token_usage"))
                .map(|v| normalize_usage(Some(v)));
            let model_context_window = info
                .and_then(|i| i.get("model_context_window"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            events.push(CodexTokenEvent {
                timestamp: obj
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                model: final_model.clone(),
                cumulative,
                last,
                model_context_window,
            });
        }

        // File-level model fallback: usage events recorded before the first
        // `turn_context` (session bootstrap) carry the placeholder "unknown"
        // even though the session's opening model is knowable — the first
        // known model in the file IS that opening model. Attribute them.
        if events.iter().any(|e| e.model == "unknown") {
            let first_known = events
                .iter()
                .find(|e| e.model != "unknown")
                .map(|e| e.model.clone());
            if let Some(opening) = first_known {
                for e in events.iter_mut() {
                    if e.model == "unknown" {
                        e.model = opening.clone();
                    }
                }
            }
        }

        // Resolve the parent anchor: the parent's cumulative value at (or
        // just before) this file's creation time.
        let parent_anchor: Option<TokenUsageCounts> = parent
            .as_ref()
            .and_then(|p| anchors_by_session.get(p))
            .and_then(|anchors| codex_anchor_at_or_before(created_at, anchors));
        let parent_first: Option<TokenUsageCounts> = parent
            .as_ref()
            .and_then(|p| anchors_by_session.get(p))
            .and_then(|anchors| anchors.first())
            .map(|a| a.usage())
            .filter(|u| u.total_tokens > 0);
        let inherited_from = parent_anchor.as_ref().map(|a| a.total_tokens).unwrap_or(0);

        let file_records = codex_delta_records(
            &session_id,
            &events,
            parent_anchor.as_ref(),
            parent_first.as_ref(),
            created_at,
            &mut seen,
        );
        // New-format records: rows recorded before the first `turn_context`
        // carry "unknown" — attribute them to the session's opening model
        // (the first known model anywhere in this file's new-format rows or
        // legacy events, in that order).
        if new_format_records.iter().any(|r| r.model == "unknown") {
            let opening = new_format_records
                .iter()
                .find(|r| r.model != "unknown")
                .map(|r| r.model.clone())
                .or_else(|| {
                    events
                        .iter()
                        .find(|e| e.model != "unknown")
                        .map(|e| e.model.clone())
                });
            if let Some(opening) = opening {
                for r in new_format_records.iter_mut() {
                    if r.model == "unknown" {
                        r.model = opening.clone();
                    }
                }
            }
        }
        let mut all_file_records = file_records;
        all_file_records.extend(new_format_records.iter().cloned());
        records.extend(all_file_records.iter().cloned());

        // Persist with fork metadata + this file's own anchors so future
        // children (and cache hits) can inherit. Anchors are thinned before
        // persisting: a 40k-event rollout would otherwise write 40k
        // cumulative snapshots (~9 MB) into the cache per file — the measured
        // cause of a 2 GB collector-cache.json after five days. The in-run
        // `anchors_by_session` keeps the full set for this run's children.
        let anchors: Vec<CachedAnchor> = events
            .iter()
            .filter_map(|e| {
                let u = e.cumulative.as_ref()?;
                if u.total_tokens <= 0 {
                    return None;
                }
                let ts = e
                    .timestamp
                    .as_deref()
                    .and_then(parse_iso)
                    .map(|dt| dt.timestamp() as f64)?;
                Some(CachedAnchor::from_usage(ts, u))
            })
            .collect();
        // Persist the READ-TAIL anchor as the last entry (after thinning):
        // the incremental tail-read resumes from exactly this cumulative.
        let tail_anchor = events.iter().rev().find_map(|e| {
            let u = e.cumulative.as_ref()?;
            if u.total_tokens <= 0 {
                return None;
            }
            let ts = e
                .timestamp
                .as_deref()
                .and_then(parse_iso)
                .map(|dt| dt.timestamp() as f64)?;
            Some(CachedAnchor::from_usage(ts, u))
        });
        let mut persisted_anchors = thin_anchors(anchors.clone());
        if let Some(t) = tail_anchor {
            persisted_anchors.push(t);
        }
        update_codex_cache(
            cache,
            &key,
            path,
            &all_file_records,
            &session_id,
            parent,
            created_at,
            persisted_anchors,
        );
        // Publish anchors for children of this session.
        if parent_ids.contains(&session_id) {
            anchors_by_session.insert(session_id.clone(), anchors);
        }
        let _ = inherited_from;
    }

    codex_scan_diag_write(serde_json::json!({
        "phase": "done",
        "timed_out": scan_timed_out,
        "files": all_paths.len(),
        "records": records.len(),
        "elapsed_ms": scan_started.elapsed().as_millis(),
    }));
    (records, all_paths.len() as i64, scan_timed_out)
}

/// Fallback session key for cache entries saved before `codex_session`
/// existed: the rollout file stem.
/// Parse one Codex `token_usage_record` line (new-format rollouts — the
/// successor of `event_msg/token_count`, carrying a per-response `usage`
/// object instead of cumulative snapshots). `model` comes from the caller's
/// turn_context state machine; the timestamp lives on the line itself.
/// `fallback_day` (file mtime day) covers lines without a parseable stamp.
fn codex_new_format_record(
    obj: &serde_json::Value,
    model: &str,
    fallback_day: &str,
) -> Option<UsageRecord> {
    let u = obj.pointer("/payload/usage")?;
    let input = u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let cached = u
        .get("cached_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let cache_w = u
        .get("cache_write_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output = u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let reasoning = u
        .get("reasoning_output_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let total = u.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    if input <= 0 && output <= 0 && total <= 0 {
        return None;
    }
    // input already INCLUDES cached_input_tokens (verified on real data:
    // total == input + output), so pass input_includes_cached=true and pin
    // the client-reported total.
    let usage = crate::agent_sources::canonical_counts(
        input,
        output,
        cached,
        cache_w,
        reasoning,
        true,
        Some(total.max(input + output)),
    );
    let stamp = obj
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(parse_iso);
    let (date, ts) = match stamp {
        Some(dt) => (
            day_string_from_epoch(dt.timestamp() as f64),
            Some(iso_string_from_epoch(dt.timestamp() as f64)),
        ),
        None => (fallback_day.to_string(), None),
    };
    Some(UsageRecord {
        date,
        timestamp: ts,
        tool: "Codex".to_string(),
        model: model.to_string(),
        usage,
        cost_usd: None,
    })
}

fn fallback_session_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Read a rollout's head to extract (session id, parent session, created_at).
/// session_meta is the first relevant line in practice; cap the hunt at 256 KB.
fn codex_head_meta(path: &Path) -> (String, Option<String>, Option<f64>) {
    use std::io::Read;
    let session = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let Ok(mut file) = fs::File::open(path) else {
        return (session, None, None);
    };
    let mut buf = Vec::with_capacity(256 * 1024);
    let mut chunk = [0u8; 64 * 1024];
    while buf.len() < 256 * 1024 {
        match file.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text_head = String::from_utf8_lossy(&buf);
    for line in text_head.lines() {
        if !line.contains("session_meta") {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        let payload = obj.get("payload");
        let id = payload
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or(session);
        let parent = codex_parent_session_id(payload);
        let created = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .or_else(|| {
                payload
                    .and_then(|p| p.get("timestamp"))
                    .and_then(|v| v.as_str())
            })
            .and_then(parse_iso)
            .map(|dt| dt.timestamp() as f64);
        return (id, parent, created);
    }
    (session, None, None)
}

/// Upstream `codexParentSessionID`: source.subagent.thread_spawn.parent_thread_id,
/// then payload.parent_thread_id / forked_from_id.
fn codex_parent_session_id(payload: Option<&serde_json::Value>) -> Option<String> {
    let payload = payload?;
    if let Some(parent) = payload
        .get("source")
        .and_then(|s| s.get("subagent"))
        .and_then(|s| s.get("thread_spawn"))
        .and_then(|t| t.get("parent_thread_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        return Some(parent.trim().to_string());
    }
    for key in ["parent_thread_id", "forked_from_id"] {
        if let Some(parent) = payload
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            return Some(parent.trim().to_string());
        }
    }
    None
}

/// Stream a file and collect its cumulative anchors (ts + usage), skipping
/// consecutive duplicates to keep the list compact.
fn codex_scan_anchors(path: &Path) -> Vec<CachedAnchor> {
    use std::io::BufRead;
    let mut anchors: Vec<CachedAnchor> = Vec::new();
    let Ok(file) = fs::File::open(path) else {
        return anchors;
    };
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("token_count") {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if obj.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(info) = obj.get("payload").and_then(|p| p.get("info")) else {
            continue;
        };
        let Some(cumulative) = info.get("total_token_usage") else {
            continue;
        };
        let usage = normalize_usage(Some(cumulative));
        if usage.total_tokens <= 0 {
            continue;
        }
        let Some(ts) = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_iso)
            .map(|dt| dt.timestamp() as f64)
        else {
            continue;
        };
        // Skip consecutive duplicates (same total).
        if anchors.last().map(|a| a.total) == Some(usage.total_tokens) {
            continue;
        }
        anchors.push(CachedAnchor::from_usage(ts, &usage));
    }
    anchors
}

/// Upstream `codexAnchor(atOrBefore:)`: binary search for the newest anchor
/// whose timestamp is <= the child's creation time.
fn codex_anchor_at_or_before(
    created_at: Option<f64>,
    anchors: &[CachedAnchor],
) -> Option<TokenUsageCounts> {
    let created = created_at?;
    let idx = anchors.partition_point(|a| a.ts <= created);
    if idx == 0 {
        return None;
    }
    let anchor = &anchors[idx - 1];
    if anchor.total <= 0 {
        return None;
    }
    Some(anchor.usage())
}

/// Cap on persisted per-file anchors. `codex_anchor_at_or_before` only needs
/// monotone ts coverage for fork-base lookups, so an evenly spaced sample
/// preserves them with bounded staleness while keeping the cache small.
const CODEX_MAX_PERSISTED_ANCHORS: usize = 256;

/// Keep the first, the exact last, and an evenly spaced sample of `anchors`.
fn thin_anchors(anchors: Vec<CachedAnchor>) -> Vec<CachedAnchor> {
    let n = anchors.len();
    if n <= CODEX_MAX_PERSISTED_ANCHORS {
        return anchors;
    }
    let mut kept: Vec<CachedAnchor> = Vec::with_capacity(CODEX_MAX_PERSISTED_ANCHORS + 1);
    let step = (n - 1) as f64 / (CODEX_MAX_PERSISTED_ANCHORS - 1) as f64;
    let mut last_idx = usize::MAX;
    for i in 0..CODEX_MAX_PERSISTED_ANCHORS {
        let idx = ((i as f64) * step).round() as usize;
        if idx == last_idx {
            continue;
        }
        last_idx = idx;
        kept.push(anchors[idx].clone());
    }
    // The exact final cumulative is the best base for children created near
    // the parent's end — always keep it verbatim.
    if kept.last().map(|a| a.ts) != Some(anchors[n - 1].ts) {
        kept.push(anchors[n - 1].clone());
    }
    kept
}

#[cfg(test)]
mod anchor_tests {
    use super::*;

    fn sample(n: usize) -> Vec<CachedAnchor> {
        (0..n)
            .map(|i| CachedAnchor {
                ts: i as f64,
                total: (i + 1) as i64 * 1000,
                input: 0,
                output: 0,
                cache_creation: 0,
                cache_read: 0,
                reasoning: 0,
            })
            .collect()
    }

    #[test]
    fn thin_anchors_small_set_untouched() {
        let a = sample(100);
        let thinned = thin_anchors(a.clone());
        assert_eq!(thinned.len(), 100);
        assert_eq!(thinned, a);
    }

    #[test]
    fn thin_anchors_bounds_and_endpoints() {
        let a = sample(40_000);
        let thinned = thin_anchors(a);
        assert!(thinned.len() <= CODEX_MAX_PERSISTED_ANCHORS + 1);
        // First and exact last survive; ts stays monotonic for binary search.
        assert_eq!(thinned.first().map(|x| x.ts), Some(0.0));
        assert_eq!(thinned.last().map(|x| x.ts), Some(39_999.0));
        assert_eq!(thinned.last().map(|x| x.total), Some(40_000_000));
        for w in thinned.windows(2) {
            assert!(w[0].ts <= w[1].ts);
        }
    }
}

/// Freshness check for a cache entry against live file metadata.
fn entry_is_fresh(entry: (u64, f64), size: u64, mtime: f64) -> bool {
    entry.0 == size && (entry.1 - mtime).abs() < 0.001
}

fn entry_meta(entry: &CachedUsageFile) -> (u64, f64) {
    (entry.size, entry.mtime)
}

fn entry_is_fresh_by_meta(entry: &CachedUsageFile, path: &Path) -> bool {
    let Some((size, mtime)) = file_meta(path) else {
        return false;
    };
    entry_is_fresh(entry_meta(entry), size, mtime)
}

/// Codex-specific cache write carrying fork metadata + anchors.
#[allow(clippy::too_many_arguments)]
fn update_codex_cache(
    cache: &mut CollectorCache,
    key: &str,
    path: &Path,
    records: &[UsageRecord],
    session_id: &str,
    parent: Option<String>,
    created_at: Option<f64>,
    anchors: Vec<CachedAnchor>,
) {
    let Some((size, mtime)) = file_meta(path) else {
        return;
    };
    cache.files.insert(
        key.to_string(),
        CachedUsageFile {
            tool: "Codex".to_string(),
            size,
            mtime,
            records: records.iter().map(CachedRecord::from).collect(),
            codex_session: Some(session_id.to_string()),
            codex_parent_session: parent,
            codex_created_at: created_at,
            codex_anchors: anchors,
            scan_offset: size,
            cursor_est_rev: None,
            model_rev: Some(CODEX_MODEL_REV),
        },
    );
}

// ---------------------------------------------------------------------------
// Codex token accounting calibration rev8 (port of upstream v0.1.44/v0.2.0
// scanCodexSessionFile + codexDeltaRecords + codexIncrementUsage)
// ---------------------------------------------------------------------------

/// One `token_count` event from a Codex rollout, holding both usage flavors.
#[derive(Debug, Clone)]
struct CodexTokenEvent {
    timestamp: Option<String>,
    model: String,
    /// `info.total_token_usage` — session-cumulative (monotonic per epoch).
    cumulative: Option<TokenUsageCounts>,
    /// `info.last_token_usage` — last turn's snapshot.
    last: Option<TokenUsageCounts>,
    /// `info.model_context_window` — used to recognize sentinel values.
    model_context_window: i64,
}

impl CodexTokenEvent {
    fn cumulative_present(&self) -> bool {
        self.cumulative.is_some()
    }
}

/// A first-event cumulative above this (with a valid per-turn snapshot) is
/// treated as an inherited counter from a resume chain, not fresh usage.
const INHERITED_COUNTER_FLOOR_TOKENS: i64 = 10_000_000;

/// Upstream `isCodexBreakdownConsistent`: all parts non-negative, input+output
/// equals total, cache contained in input, reasoning contained in output.
fn codex_breakdown_consistent(u: &TokenUsageCounts, total: i64) -> bool {
    u.input_tokens >= 0
        && u.output_tokens >= 0
        && u.cache_creation_input_tokens >= 0
        && u.cache_read_input_tokens >= 0
        && u.reasoning_output_tokens >= 0
        && u.input_tokens + u.output_tokens == total
        && u.cache_creation_input_tokens + u.cache_read_input_tokens <= u.input_tokens
        && u.reasoning_output_tokens <= u.output_tokens
}

/// Upstream `isCodexContextWindowSentinel`: an all-zero cumulative whose total
/// equals the model context window is a synthetic marker, not usage.
fn codex_is_context_window_sentinel(event: &CodexTokenEvent) -> bool {
    let Some(c) = event.cumulative.as_ref() else {
        return false;
    };
    c.input_tokens == 0
        && c.output_tokens == 0
        && c.cache_creation_input_tokens == 0
        && c.cache_read_input_tokens == 0
        && c.reasoning_output_tokens == 0
        && event.last.as_ref().map(|l| l.total_tokens).unwrap_or(0) == 0
        && event.model_context_window > 0
        && c.total_tokens == event.model_context_window
}

/// Upstream `isCredibleCodexReset`: a drop in the cumulative total counts as a
/// counter reset only if the `last` snapshot matches the new value, or a later
/// event climbs from it while staying below the previous maximum.
fn codex_is_credible_reset(
    index: usize,
    events: &[CodexTokenEvent],
    current: &TokenUsageCounts,
    previous: &TokenUsageCounts,
) -> bool {
    if let Some(last) = events[index].last.as_ref() {
        if last.total_tokens == current.total_tokens
            && codex_breakdown_consistent(last, current.total_tokens)
        {
            return true;
        }
    }
    for candidate in events.iter().skip(index + 1) {
        let Some(next) = candidate.cumulative.as_ref() else {
            continue;
        };
        if next.total_tokens <= 0 {
            continue;
        }
        if next.total_tokens == current.total_tokens {
            continue;
        }
        return next.total_tokens > current.total_tokens
            && next.total_tokens < previous.total_tokens;
    }
    false
}

/// Upstream `codexIncrementUsage`: distribute a delta total across fields.
/// Prefers the `last` snapshot when it exactly matches the delta; else the
/// per-field difference (when monotonic); else a total-only record.
fn codex_increment_usage(
    current: &TokenUsageCounts,
    previous: Option<&TokenUsageCounts>,
    last: Option<&TokenUsageCounts>,
    total: i64,
) -> TokenUsageCounts {
    if let Some(last) = last {
        if last.total_tokens == total && codex_breakdown_consistent(last, total) {
            let mut result = last.clone();
            result.total_tokens = total;
            return result;
        }
    }
    let empty = TokenUsageCounts::default();
    let prev = previous.unwrap_or(&empty);
    let monotonic = current.input_tokens >= prev.input_tokens
        && current.output_tokens >= prev.output_tokens
        && current.cache_creation_input_tokens >= prev.cache_creation_input_tokens
        && current.cache_read_input_tokens >= prev.cache_read_input_tokens
        && current.reasoning_output_tokens >= prev.reasoning_output_tokens;
    if monotonic {
        let result = TokenUsageCounts {
            input_tokens: current.input_tokens - prev.input_tokens,
            output_tokens: current.output_tokens - prev.output_tokens,
            cache_creation_input_tokens: current.cache_creation_input_tokens
                - prev.cache_creation_input_tokens,
            cache_read_input_tokens: current.cache_read_input_tokens - prev.cache_read_input_tokens,
            reasoning_output_tokens: current.reasoning_output_tokens - prev.reasoning_output_tokens,
            total_tokens: total,
        };
        if codex_breakdown_consistent(&result, total) {
            return result;
        }
        return TokenUsageCounts {
            total_tokens: total,
            ..Default::default()
        };
    }
    TokenUsageCounts {
        total_tokens: total,
        ..Default::default()
    }
}

/// Upstream `codexDeltaRecords`: turn one file's token_count events into
/// per-event delta records. Files without the cumulative schema fall back to
/// the legacy per-event `last` estimate (dedup'd). `seen` is the cross-file
/// request-id set, preserved between files of the same collection pass.
fn codex_delta_records(
    session_id: &str,
    events: &[CodexTokenEvent],
    parent_anchor: Option<&TokenUsageCounts>,
    parent_first_cumulative: Option<&TokenUsageCounts>,
    child_created_at: Option<f64>,
    seen: &mut std::collections::BTreeSet<String>,
) -> Vec<UsageRecord> {
    let mut records: Vec<UsageRecord> = Vec::new();
    let has_cumulative_schema = events.iter().any(|e| e.cumulative_present());

    if !has_cumulative_schema {
        // Legacy schema: only `last_token_usage` exists — use it directly
        // (upstream "codex_last_usage_legacy_estimate").
        for event in events {
            let Some(usage) = event.last.as_ref() else {
                continue;
            };
            if usage.total_tokens <= 0 {
                continue;
            }
            let Some(timestamp) = event.timestamp.as_ref() else {
                continue;
            };
            let Some(day) = day_string_from_iso(timestamp) else {
                continue;
            };
            let fingerprint = format!(
                "{}:{}:{}:{}:{}:{}",
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
                usage.reasoning_output_tokens,
                usage.total_tokens
            );
            let request_id = format!("codex:legacy:{session_id}:{timestamp}:{fingerprint}");
            if !seen.insert(request_id) {
                continue;
            }
            let mut usage = usage.clone();
            usage.finalize_total();
            records.push(UsageRecord {
                date: day,
                timestamp: Some(timestamp.clone()),
                tool: "Codex".to_string(),
                model: event.model.clone(),
                usage,
                cost_usd: None,
            });
        }
        return records;
    }

    // Cumulative schema: per-event usage = delta between consecutive totals.
    // A fork file inherits the parent's anchor: events up to (and including)
    // the matching cumulative value are inherited history counted by the
    // parent, so delta-ing resumes from there (upstream codexForkAnchor).
    let mut previous: Option<TokenUsageCounts> = None;
    let mut start_index = 0usize;
    if let Some(anchor) = parent_anchor {
        if anchor.total_tokens > 0 {
            if let Some(anchor_index) = events
                .iter()
                .position(|e| matches!(e.cumulative.as_ref(), Some(c) if c == anchor))
            {
                previous = Some(anchor.clone());
                start_index = anchor_index + 1;
            }
        }
    }
    // Dump fallback: a fork file whose FIRST cumulative equals the parent's
    // first cumulative field-for-field is a transcript dump (compact/resume
    // writes a fresh rollout copying the parent's whole event history; the
    // exact anchor no longer matches because the parent kept growing after
    // the dump). Copied events all carry timestamps <= the dump moment, so
    // delta-ing resumes strictly after `child_created_at`.
    if start_index == 0 {
        if let (Some(parent_first), Some(created)) = (parent_first_cumulative, child_created_at) {
            let is_dump = parent_first.total_tokens > 0
                && matches!(events.first().and_then(|e| e.cumulative.as_ref()),
                            Some(c) if c == parent_first);
            if is_dump {
                let mut last_before: Option<&TokenUsageCounts> = None;
                let mut split = events.len();
                for (index, event) in events.iter().enumerate() {
                    let Some(ts) = event
                        .timestamp
                        .as_deref()
                        .and_then(parse_iso)
                        .map(|dt| dt.timestamp() as f64)
                    else {
                        continue;
                    };
                    if ts > created {
                        split = index;
                        break;
                    }
                    if let Some(c) = event.cumulative.as_ref() {
                        last_before = Some(c);
                    }
                }
                previous = last_before.cloned().or_else(|| parent_anchor.cloned());
                start_index = split;
            }
        }
    }
    let mut epoch = 0usize;
    for (index, event) in events.iter().enumerate().skip(start_index) {
        if !event.cumulative_present() {
            continue;
        }
        let Some(current) = event.cumulative.as_ref() else {
            continue;
        };
        if current.total_tokens <= 0 {
            continue;
        }
        let Some(timestamp) = event.timestamp.as_ref() else {
            continue;
        };
        let Some(day) = day_string_from_iso(timestamp) else {
            continue;
        };

        let (delta_total, is_reset): (i64, bool);
        match previous.as_ref() {
            Some(prev) => {
                if current.total_tokens == prev.total_tokens {
                    continue; // duplicate snapshot
                } else if current.total_tokens > prev.total_tokens {
                    delta_total = current.total_tokens - prev.total_tokens;
                    is_reset = false;
                } else if codex_is_context_window_sentinel(event) {
                    continue; // synthetic context-window marker
                } else if codex_is_credible_reset(index, events, current, prev) {
                    epoch += 1;
                    delta_total = current.total_tokens;
                    is_reset = true;
                } else {
                    continue; // non-credible drop — skip rather than guess
                }
            }
            None => {
                // Inherited-counter guard: a first cumulative in the tens of
                // millions+ with a valid per-turn snapshot is a resumed
                // counter (resume chains point parent_thread_id at the
                // ORIGINAL session, whose anchors long stopped growing, so
                // fork anchoring can't reach it). Count only this turn's
                // real usage instead of re-counting the whole history.
                let last_total = event.last.as_ref().map(|l| l.total_tokens).unwrap_or(0);
                if current.total_tokens > INHERITED_COUNTER_FLOOR_TOKENS && last_total > 0 {
                    delta_total = last_total;
                } else {
                    delta_total = current.total_tokens;
                }
                is_reset = false;
            }
        }
        if delta_total <= 0 {
            continue;
        }
        let usage = codex_increment_usage(
            current,
            if is_reset { None } else { previous.as_ref() },
            event.last.as_ref(),
            delta_total,
        );
        let request_id = format!(
            "codex:cumulative:{session_id}:{epoch}:{}",
            current.total_tokens
        );
        if !seen.insert(request_id) {
            previous = Some(current.clone());
            continue;
        }
        records.push(UsageRecord {
            date: day,
            timestamp: Some(timestamp.clone()),
            tool: "Codex".to_string(),
            model: event.model.clone(),
            usage,
            cost_usd: None,
        });
        previous = Some(current.clone());
    }
    records
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------
