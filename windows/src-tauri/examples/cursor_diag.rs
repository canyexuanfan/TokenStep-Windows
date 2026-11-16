//! Cursor fetch diagnostics: replicate fetch_page exactly and print every
//! failure detail (status/body/TLS error), for the overlay 401 investigation.
//!
//! Run with: cargo run --release --example cursor_diag

use tokenstep_lib as app;

fn main() {
    let cred = match app::cursor_credential_for_check() {
        Some(c) => c,
        None => { println!("no credential"); return; }
    };
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let body = serde_json::json!({"startDate": 1790000000000i64, "endDate": 1799999999999i64}).to_string();
    let resp = client
        .post("https://api2.cursor.sh/aiserver.v1.DashboardService/GetFilteredUsageEvents")
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("Authorization", format!("Bearer {}", cred.access_token))
        .body(body)
        .send()
        .unwrap();
    let status = resp.status();
    let value: serde_json::Value = resp.json().unwrap_or_default();
    println!("status: {}", status);
    let events = value.pointer("/usageEventsDisplay").and_then(|v| v.as_array());
    match events {
        Some(list) => {
            println!("events: {}", list.len());
            for ev in list {
                let ts = ev.get("timestamp").cloned().unwrap_or_default();
                let model = ev.get("model").cloned().unwrap_or_default();
                let tu = ev.get("tokenUsage").cloned().unwrap_or_default();
                println!("  ts={} model={} tokenUsage={}", ts, model, tu);
            }
        }
        None => println!("no usageEventsDisplay; keys: {:?}", value.as_object().map(|o| o.keys().collect::<Vec<_>>())),
    }
}


