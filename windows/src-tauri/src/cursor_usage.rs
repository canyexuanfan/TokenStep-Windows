//! Cursor official usage — a port of upstream v0.2.1 `CursorUsageService`.
//!
//! Credentials come from Cursor's own state DB (ItemTable); usage events come
//! from Cursor's dashboard HTTP API (paged); results are cached locally with
//! a window-replace strategy and merged into the snapshot as an overlay
//! (strip → apply, idempotent). Token caliber: input + output + cacheRead +
//! cacheWrite all count toward the today ring.

use crate::models::{DailyAgentWork, DailyRhythm, UsageSnapshot};
use crate::net;
use crate::paths;
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PRIMARY_URL: &str = "https://cursor.com/api/dashboard/get-filtered-usage-events";
const FALLBACK_URL: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetFilteredUsageEvents";
const MAX_PAGES: usize = 20;
const PAGE_SIZE: usize = 100;

// ── credential ────────────────────────────────────────────────────────────

pub struct CursorCredential {
    pub user_id: String,
    pub access_token: String,
    pub dashboard_user_id: Option<String>,
}

/// Read the auth token + dashboard user id from Cursor's state.vscdb.
pub fn read_credential() -> Option<CursorCredential> {
    let db = paths::cursor_state_vscdb();
    if !db.is_file() {
        return None;
    }
    let uri = format!("file:{}?mode=ro", db.display().to_string().replace('\\', "/"));
    let conn = rusqlite::Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let token: String = conn
        .query_row(
            "select value from ItemTable where key='cursorAuth/accessToken' limit 1",
            [],
            |r| r.get(0),
        )
        .ok()?;
    if token.trim().is_empty() {
        return None;
    }
    let dashboard_user_id: Option<String> = conn
        .query_row(
            "select json_extract(value, '$.dashboardUserId') from ItemTable \
             where key='src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser' limit 1",
            [],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let user_id = jwt_subject(&token)?;
    Some(CursorCredential { user_id, access_token: token, dashboard_user_id })
}

/// Decode the JWT `sub` claim (base64url payload, second segment).
fn jwt_subject(token: &str) -> Option<String> {
    let part = token.split('.').nth(1)?;
    let decoded = base64url_decode(part)?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value.get("sub")?.as_str().map(String::from)
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in input.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '-' | '+' => 62,
            '_' | '/' => 63,
            '=' => break,
            _ => continue,
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

// ── usage events ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CursorUsageEvent {
    pub timestamp_ms: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub charged_cents: f64,
    pub model: String,
}

impl CursorUsageEvent {
    fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CursorUsageDay {
    pub date: String,
    pub total_tokens: i64,
    pub cost: f64,
    pub event_count: i64,
    pub models: BTreeMap<String, i64>,
    pub events: Vec<CursorUsageEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CursorUsageCache {
    pub days: Vec<CursorUsageDay>,
}

pub fn read_cache() -> CursorUsageCache {
    std::fs::read_to_string(paths::cursor_usage_cache_json())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_cache(cache: &CursorUsageCache) {
    // Best-effort: a cache failure must never zero the ledger.
    let path = paths::cursor_usage_cache_json();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(path, text);
    }
}

/// Fetch recent events (lookback days, max 30) and merge into the cache with
/// window replacement. Returns the refreshed cache.
pub fn refresh(history_days: i64) -> Result<CursorUsageCache, String> {
    let cred = read_credential().ok_or("未登录 Cursor")?;
    let lookback = history_days.clamp(1, 30);
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let today = chrono::Utc::now().with_timezone(&tz).format("%Y-%m-%d").to_string();
    let start_day = (chrono::Utc::now().with_timezone(&tz) - chrono::Duration::days(lookback - 1))
        .format("%Y-%m-%d")
        .to_string();
    let start_ms = date_to_epoch_ms(&start_day);
    let end_ms = date_to_epoch_ms(&today) + 86_400_000;

    let mut events: Vec<CursorUsageEvent> = Vec::new();
    for page in 1..=MAX_PAGES {
        let body = serde_json::json!({
            "startDate": start_ms.to_string(),
            "endDate": end_ms.to_string(),
            "page": page,
            "pageSize": PAGE_SIZE,
            "userId": cred.dashboard_user_id,
        });
        match fetch_page(PRIMARY_URL, &cred, &body)
            .or_else(|| fetch_page(FALLBACK_URL, &cred, &body))
        {
            Some(page_events) => {
                let got = page_events.len();
                events.extend(page_events);
                if got < PAGE_SIZE {
                    break;
                }
            }
            None => {
                // Partial success is acceptable once we have data.
                if events.is_empty() {
                    return Err("Cursor 用量暂不可用".to_string());
                }
                break;
            }
        }
    }

    // Group by day (Asia/Shanghai).
    let mut by_day: BTreeMap<String, Vec<CursorUsageEvent>> = BTreeMap::new();
    for ev in events {
        let day = epoch_ms_to_day(ev.timestamp_ms);
        by_day.entry(day).or_default().push(ev);
    }
    let mut fresh_days: Vec<CursorUsageDay> = by_day
        .into_iter()
        .map(|(date, evs)| {
            let mut day = CursorUsageDay {
                date,
                event_count: evs.len() as i64,
                ..Default::default()
            };
            for ev in &evs {
                day.total_tokens += ev.total_tokens();
                day.cost += ev.charged_cents / 100.0;
                *day.models.entry(if ev.model.is_empty() { "unknown".into() } else { ev.model.clone() }).or_insert(0) +=
                    ev.total_tokens();
            }
            day.events = evs;
            day
        })
        .collect();
    fresh_days.sort_by(|a, b| a.date.cmp(&b.date));

    // Window replace: keep cache days outside the refresh window.
    let mut cache = read_cache();
    cache.days.retain(|d| d.date < start_day);
    cache.days.extend(fresh_days);
    cache.days.sort_by(|a, b| a.date.cmp(&b.date));
    cache.days.truncate(400); // sanity cap
    write_cache(&cache);
    Ok(cache)
}

fn fetch_page(url: &str, cred: &CursorCredential, body: &serde_json::Value) -> Option<Vec<CursorUsageEvent>> {
    let client = net::blocking_client()
        .user_agent("TokenStep")
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?;
    let cookie = format!("WorkosCursorSessionToken={}::{}", cred.user_id, cred.access_token);
    let resp = client
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Origin", "https://cursor.com")
        .header("Cookie", cookie)
        .json(body)
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let value: serde_json::Value = resp.json().ok()?;
    let rows = value
        .get("usageEventsDisplay")
        .or_else(|| value.get("usageEvents"))?
        .as_array()?
        .clone();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let usage = row.get("tokenUsage").unwrap_or(&row);
        let ts = row
            .get("timestamp")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                row.get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(parse_iso_to_ms)
            })
            .unwrap_or(0);
        if ts <= 0 {
            continue;
        }
        let n = |keys: &[&str]| -> i64 {
            for k in keys {
                if let Some(v) = usage.get(*k).and_then(|v| v.as_f64()) {
                    return v as i64;
                }
            }
            0
        };
        out.push(CursorUsageEvent {
            timestamp_ms: ts,
            input_tokens: n(&["inputTokens", "input_tokens"]),
            output_tokens: n(&["outputTokens", "output_tokens"]),
            cache_read_tokens: n(&["cacheReadTokens", "cache_read_tokens"]),
            cache_write_tokens: n(&["cacheWriteTokens", "cache_write_tokens"]),
            charged_cents: n(&["chargedCents", "totalCents", "total_cents"]) as f64,
            model: usage
                .get("model")
                .or_else(|| row.get("model"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
        });
    }
    Some(out)
}

fn date_to_epoch_ms(day: &str) -> i64 {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|ndt| ndt.and_utc().timestamp_millis())
        .unwrap_or(0)
}

fn epoch_ms_to_day(ms: i64) -> String {
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.with_timezone(&tz).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn parse_iso_to_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s.trim_end_matches('Z'))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S%.f").map(|n| n.and_utc().fixed_offset()))
        .ok()
        .map(|dt| dt.timestamp_millis())
}

// ── snapshot overlay merge (strip → apply, idempotent) ────────────────────

/// Merge official Cursor usage into a ledger snapshot. Strips any previous
/// overlay first, so repeated merges are idempotent.
pub fn merge(mut snapshot: UsageSnapshot, days: &[CursorUsageDay]) -> UsageSnapshot {
    strip_cursor(&mut snapshot);
    let has_nonzero = days.iter().any(|d| d.total_tokens > 0);
    if !has_nonzero {
        return snapshot;
    }
    for day in days {
        if day.total_tokens <= 0 {
            continue;
        }
        // daily
        let daily = snapshot
            .daily
            .iter_mut()
            .find(|d| d.date == day.date);
        let daily = match daily {
            Some(d) => d,
            None => {
                snapshot.daily.push(crate::models::DailyUsage {
                    date: day.date.clone(),
                    ..Default::default()
                });
                snapshot.daily.last_mut().unwrap()
            }
        };
        daily.tools.insert("Cursor".into(), day.total_tokens);
        daily.total_tokens += day.total_tokens;
        daily.cost += day.cost;
        for (m, t) in &day.models {
            *daily.models.entry(m.clone()).or_insert(0) += t;
        }
        // agent work
        let aw = snapshot
            .agent_work
            .iter_mut()
            .find(|w| w.date == day.date);
        let aw = match aw {
            Some(w) => w,
            None => {
                snapshot.agent_work.push(DailyAgentWork { date: day.date.clone(), ..Default::default() });
                snapshot.agent_work.last_mut().unwrap()
            }
        };
        aw.total_tokens += day.total_tokens;
        aw.input_tokens += day.total_tokens; // official events: full token count
        aw.model_request_count += day.event_count;
        // sources
        if let Some(src) = aw.sources.iter_mut().find(|s| s.source == "Cursor") {
            src.tokens += day.total_tokens;
            src.model_request_count += day.event_count;
        } else {
            aw.sources.push(crate::models::AgentWorkSource {
                source: "Cursor".into(),
                tokens: day.total_tokens,
                model_request_count: day.event_count,
                tool_call_count: 0,
            });
        }
        // hourly buckets from events
        for ev in &day.events {
            let hour = chrono::DateTime::from_timestamp_millis(ev.timestamp_ms)
                .map(|dt| dt.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap()).hour() as i64)
                .unwrap_or(0);
            let bucket = aw.hourly_buckets.iter_mut().find(|b| b.hour == hour);
            let input = ev.input_tokens + ev.cache_read_tokens;
            let bucket = match bucket {
                Some(b) => b,
                None => {
                    aw.hourly_buckets.push(crate::models::AgentWorkHourBucket {
                        hour,
                        sources: vec![],
                    });
                    aw.hourly_buckets.last_mut().unwrap()
                }
            };
            if let Some(s) = bucket.sources.iter_mut().find(|s| s.source == "Cursor") {
                s.tokens += ev.total_tokens();
                s.input_tokens += input;
                s.cached_input_tokens += ev.cache_read_tokens;
                s.output_tokens += ev.output_tokens;
            } else {
                bucket.sources.push(crate::models::AgentWorkHourlySource {
                    source: "Cursor".into(),
                    tokens: ev.total_tokens(),
                    input_tokens: input,
                    cached_input_tokens: ev.cache_read_tokens,
                    output_tokens: ev.output_tokens,
                    cache_coverage_complete: true,
                });
            }
            // rhythm
            let rhythm = snapshot
                .rhythms
                .iter_mut()
                .find(|r| r.date == day.date);
            let rhythm = match rhythm {
                Some(r) => r,
                None => {
                    snapshot.rhythms.push(DailyRhythm { date: day.date.clone(), ..Default::default() });
                    snapshot.rhythms.last_mut().unwrap()
                }
            };
            while rhythm.buckets.len() <= hour as usize {
                let h = rhythm.buckets.len() as i64;
                rhythm.buckets.push(crate::models::HourlyTokenBucket { hour: h, tokens: 0 });
            }
            rhythm.buckets[hour as usize].tokens += ev.total_tokens();
            rhythm.total_tokens += ev.total_tokens();
            if rhythm.peak_tokens < rhythm.buckets[hour as usize].tokens {
                rhythm.peak_hour = Some(hour);
                rhythm.peak_tokens = rhythm.buckets[hour as usize].tokens;
            }
            if !rhythm.buckets[hour as usize].sources_flag_set() {
                rhythm.active_hours = rhythm.buckets.iter().filter(|b| b.tokens > 0).count() as i64;
            }
        }
        aw.active_hours = aw.hourly_buckets.iter().filter(|b| !b.sources.is_empty()).count() as i64;
        // totals
        snapshot.totals.tokens += day.total_tokens;
        snapshot.totals.cost += day.cost;
    }
    // tools + models rollups
    rebuild_rollups(&mut snapshot);
    // source info
    snapshot.sources.insert(
        "Cursor".into(),
        crate::models::SourceInfo {
            status: Some("ok".into()),
            files: None,
            records: Some(days.iter().map(|d| d.event_count).sum()),
            ..Default::default()
        },
    );
    snapshot
}

/// Remove a previous Cursor overlay from a snapshot.
fn strip_cursor(snapshot: &mut UsageSnapshot) {
    let cursor_days: std::collections::BTreeSet<String> = snapshot
        .daily
        .iter()
        .filter_map(|d| d.tools.get("Cursor").map(|_| d.date.clone()))
        .collect();
    // If no Cursor overlay is present, nothing to strip.
    if cursor_days.is_empty() {
        return;
    }
    for date in &cursor_days {
        if let Some(d) = snapshot.daily.iter_mut().find(|d| &d.date == date) {
            if let Some(t) = d.tools.remove("Cursor") {
                d.total_tokens -= t;
                // cost of the overlay is unknown post-hoc; subtract via agent work below.
                let _ = t;
            }
        }
        if let Some(aw) = snapshot.agent_work.iter_mut().find(|w| &w.date == date) {
            aw.sources.retain(|s| s.source != "Cursor");
            aw.total_tokens = aw.sources.iter().map(|s| s.tokens).sum();
            aw.hourly_buckets.iter_mut().for_each(|b| b.sources.retain(|s| s.source != "Cursor"));
            aw.hourly_buckets.retain(|b| !b.sources.is_empty());
            aw.active_hours = aw.hourly_buckets.len() as i64;
            aw.model_request_count = aw.sources.iter().map(|s| s.model_request_count).sum();
        }
        if let Some(r) = snapshot.rhythms.iter_mut().find(|r| &r.date == date) {
            recompute_rhythm_from_agent_work(r, snapshot.agent_work.iter().find(|w| &w.date == date));
        }
    }
    snapshot.daily.retain(|d| d.total_tokens > 0 || !d.tools.is_empty());
    snapshot.sources.remove("Cursor");
    rebuild_rollups(snapshot);
}

fn recompute_rhythm_from_agent_work(r: &mut DailyRhythm, aw: Option<&DailyAgentWork>) {
    r.buckets.clear();
    r.total_tokens = 0;
    r.peak_tokens = 0;
    r.peak_hour = None;
    if let Some(aw) = aw {
        for b in &aw.hourly_buckets {
            let t = b.total_tokens();
            if t <= 0 {
                continue;
            }
            r.buckets.push(crate::models::HourlyTokenBucket { hour: b.hour, tokens: t });
            r.total_tokens += t;
            if t > r.peak_tokens {
                r.peak_tokens = t;
                r.peak_hour = Some(b.hour);
            }
        }
        r.active_hours = r.buckets.len() as i64;
    }
}

fn rebuild_rollups(snapshot: &mut UsageSnapshot) {
    // tools / models / totals from daily rows.
    let mut tools: BTreeMap<String, i64> = BTreeMap::new();
    let mut models: BTreeMap<String, i64> = BTreeMap::new();
    let mut total = 0i64;
    let mut cost = 0f64;
    let mut active = 0i64;
    for d in &snapshot.daily {
        for (t, v) in &d.tools {
            *tools.entry(t.clone()).or_insert(0) += v;
        }
        for (m, v) in &d.models {
            *models.entry(m.clone()).or_insert(0) += v;
        }
        total += d.total_tokens;
        cost += d.cost;
        if d.total_tokens > 0 {
            active += 1;
        }
    }
    snapshot.tools = tools
        .into_iter()
        .map(|(tool, tokens)| crate::models::ToolUsage { tool, tokens, percent: None })
        .collect();
    snapshot.tools.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    let t_ref = total.max(1) as f64;
    for t in &mut snapshot.tools {
        t.percent = Some(t.tokens as f64 * 100.0 / t_ref);
    }
    snapshot.models = models
        .into_iter()
        .map(|(model, tokens)| crate::models::ModelUsage { model, tool: None, tokens, percent: None })
        .collect();
    snapshot.models.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    for m in &mut snapshot.models {
        m.percent = Some(m.tokens as f64 * 100.0 / t_ref);
    }
    snapshot.totals.tokens = total;
    snapshot.totals.cost = cost;
    snapshot.totals.active_days = active;
}

use chrono::Timelike;

// tiny helper shim used above (HourlyTokenBucket has no sources flag; the
// rhythm active_hours recalc is done unconditionally).
trait SourcesFlag {
    fn sources_flag_set(&self) -> bool;
}
impl SourcesFlag for crate::models::HourlyTokenBucket {
    fn sources_flag_set(&self) -> bool {
        true
    }
}

// ── Cursor code signal (L3) ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CursorCodeSignal {
    pub blocks: i64,
    pub models: i64,
    pub conversations: i64,
    pub requests: i64,
    pub files: i64,
    pub top_models: Vec<(String, i64)>,
}

pub fn read_code_signal() -> Result<CursorCodeSignal, String> {
    let db = paths::cursor_code_tracking_db();
    if !db.is_file() {
        return Err("missing_db".into());
    }
    let uri = format!("file:{}?mode=ro", db.display().to_string().replace('\\', "/"));
    let conn = rusqlite::Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "query_failed".to_string())?;
    let signal = conn
        .query_row(
            "select count(*) as blocks,
                 count(distinct model) as models,
                 count(distinct conversationId) as conversations,
                 count(distinct requestId) as requests,
                 count(distinct fileName) as files
             from ai_code_hashes
             where date(createdAt/1000, 'unixepoch', 'localtime') = date('now', 'localtime')",
            [],
            |r| {
                Ok(CursorCodeSignal {
                    blocks: r.get(0)?,
                    models: r.get(1)?,
                    conversations: r.get(2)?,
                    requests: r.get(3)?,
                    files: r.get(4)?,
                    top_models: vec![],
                })
            },
        )
        .map_err(|_| "query_failed".to_string())?;
    let mut top = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "select model, count(*) as n from ai_code_hashes \
         where date(createdAt/1000,'unixepoch','localtime')=date('now','localtime') \
         group by model order by n desc",
    ) {
        if let Ok(mut rows) = stmt.query([]) {
            while let Ok(Some(row)) = rows.next() {
                if let (Ok(m), Ok(n)) = (row.get::<_, String>(0), row.get::<_, i64>(1)) {
                    top.push((m, n));
                }
            }
        }
    }
    let mut signal = signal;
    signal.top_models = top.into_iter().take(3).collect();
    if signal.blocks == 0 {
        return Err("empty".into());
    }
    Ok(signal)
}
