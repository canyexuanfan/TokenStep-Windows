//! Cursor fetch diagnostics: replicate fetch_page exactly and print every
//! failure detail (status/body/TLS error), for the overlay 401 investigation.
//!
//! Run with: cargo run --release --example cursor_diag

use tokenstep_lib as app;

fn main() {
    // Pull the credential through the real path (safeStorage decrypt).
    let Some(cred) = app::cursor_credential_for_check() else {
        println!("credential: FAILED to read/decrypt");
        return;
    };
    println!(
        "credential: ok (user_id len {}, access_token len {})",
        cred.user_id.len(),
        cred.access_token.len()
    );

    let client = match reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => { println!("client build err: {e}"); return; }
    };

    let body = serde_json::json!({
        "startDate": 1790000000000i64,
        "endDate": 1799999999999i64,
    })
    .to_string();

    for (name, url, extra_headers) in [
        (
            "api2 Connect-POST Bearer",
            "https://api2.cursor.sh/aiserver.v1.DashboardService/GetFilteredUsageEvents",
            vec![
                ("Connect-Protocol-Version", "1"),
                ("Authorization", &format!("Bearer {}", cred.access_token)),
            ],
        ),
        (
            "cursor.com Connect-POST Bearer",
            "https://cursor.com/api/dashboard/get-filtered-usage-events",
            vec![
                ("Connect-Protocol-Version", "1"),
                ("Authorization", &format!("Bearer {}", cred.access_token)),
                ("Origin", "https://cursor.com"),
            ],
        ),
        (
            "api2 cookie",
            "https://api2.cursor.sh/aiserver.v1.DashboardService/GetFilteredUsageEvents",
            vec![(
                "Cookie",
                &format!("WorkosCursorSessionToken={}::{}", cred.user_id, cred.access_token),
            )],
        ),
        (
            "usage-summary cookie",
            "https://cursor.com/api/usage-summary",
            vec![(
                "Cookie",
                &format!("WorkosCursorSessionToken={}::{}", cred.user_id, cred.access_token),
            )],
        ),
    ] {
        let mut req = client
            .post(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");
        for (k, v) in &extra_headers {
            req = req.header(*k, *v);
        }
        let result = req.body(body.clone()).send();
        match result {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                println!(
                    "[{name}] HTTP {status} body[:200]={}",
                    &text[..text.len().min(200)]
                );
            }
            Err(e) => println!("[{name}] transport ERR: {e}"),
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}
