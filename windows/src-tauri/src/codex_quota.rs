//! Codex quota reader — a port of macOS `CodexQuotaService.swift`.
//!
//! Launches `codex app-server --listen stdio://`, sends a JSON-RPC
//! `account/rateLimits/read` request, and parses the 5h / 7d usage windows.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::os::windows::process::CommandExt;

const REQUEST_ID: i64 = 2;

/// Quota snapshot shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexQuotaSnapshot {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_hour_used_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_hour_resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seven_day_used_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seven_day_resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for CodexQuotaSnapshot {
    fn default() -> Self {
        CodexQuotaSnapshot {
            available: false,
            five_hour_used_percent: None,
            five_hour_resets_at: None,
            seven_day_used_percent: None,
            seven_day_resets_at: None,
            error: None,
        }
    }
}

/// Read the current Codex rate-limit quota by talking to the app-server.
pub fn read() -> CodexQuotaSnapshot {
    // Find the codex binary. On Windows it's typically `codex.cmd` from npm.
    let codex = find_codex();
    let codex = match codex {
        Some(c) => c,
        None => {
            return CodexQuotaSnapshot {
                available: false,
                error: Some("未找到 codex 命令".to_string()),
                ..Default::default()
            }
        }
    };

    // Build the command line. Batch scripts (.cmd/.bat — the npm shim) cannot
    // be spawned directly by CreateProcess; they must go through cmd.exe /c.
    // A real .exe is spawned as-is.
    let is_batch = codex.ends_with(".cmd") || codex.ends_with(".bat");
    let mut cmd = if is_batch {
        let mut c = Command::new("cmd.exe");
        c.arg("/c").arg(&codex);
        c
    } else {
        Command::new(&codex)
    };
    // CREATE_NO_WINDOW: this is a GUI app — spawning the npm batch shim would
    // otherwise flash a console window every quota refresh.
    cmd.creation_flags(0x0800_0000);
    let mut child = match cmd
        .arg("app-server")
        .arg("--listen")
        .arg("stdio://")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // stderr goes to the sink: app-server is chatty (config warnings,
        // logs) and an unread pipe buffer would block it before the quota
        // response ever reaches stdout.
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return CodexQuotaSnapshot {
                available: false,
                error: Some(format!("启动 codex app-server 失败: {}", e)),
                ..Default::default()
            }
        }
    };

    // Send initialize + rateLimits/read requests.
    let init_req = r#"{"method":"initialize","id":1,"params":{"clientInfo":{"name":"tokenstep","title":"TokenStep","version":"0.1.0"},"capabilities":null}}"#;
    let quota_req = format!(r#"{{"method":"account/rateLimits/read","id":{}}}"#, REQUEST_ID);

    // Write the handshake, then KEEP the stdin pipe open (as_mut, not take):
    // dropping it signals EOF and the app-server shuts down before the
    // id=2 response is ever written.
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = writeln!(stdin, "{}", init_req);
        let _ = writeln!(stdin, "{}", quota_req);
        let _ = stdin.flush();
    }

    // Read stdout lines until the response with id=REQUEST_ID arrives, with a
    // hard timeout (upstream cuts at 4s; allow 12s for the slow npm-batch +
    // node cold start on Windows). The reader runs on its own thread so a
    // hung app-server can never block the caller forever.
    let stdout = child.stdout.take();
    let result = if let Some(stdout) = stdout {
        let (tx, rx) = std::sync::mpsc::channel::<Option<CodexQuotaSnapshot>>();
        let needle = format!("\"id\":{}", REQUEST_ID);
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut found = None;
            for line_result in reader.lines() {
                let line = match line_result {
                    Ok(l) => l,
                    Err(_) => break,
                };
                if line.contains(&needle) {
                    found = parse_response(&line);
                    break;
                }
            }
            let _ = tx.send(found);
        });
        rx.recv_timeout(std::time::Duration::from_secs(12)).ok().flatten()
    } else {
        None
    };

    // Kill the child process.
    let _ = child.kill();
    let _ = child.wait();

    match result {
        Some(snap) => snap,
        None => CodexQuotaSnapshot {
            available: false,
            error: Some("未读取到 Codex 额度".to_string()),
            ..Default::default()
        },
    }
}

/// Find the codex executable on Windows (try codex.cmd, codex.exe, codex).
fn find_codex() -> Option<String> {
    for name in &["codex.cmd", "codex.exe", "codex"] {
        if which::which(name).is_ok() {
            return Some(name.to_string());
        }
    }
    None
}

fn parse_response(line: &str) -> Option<CodexQuotaSnapshot> {
    let root: serde_json::Value = serde_json::from_str(line).ok()?;
    // Check for error.
    if let Some(err) = root.get("error").and_then(|e| e.get("message")) {
        return Some(CodexQuotaSnapshot {
            available: false,
            error: Some(err.as_str().unwrap_or("unknown error").to_string()),
            ..Default::default()
        });
    }
    let result = root.get("result")?;
    // Prefer "codex" key, fall back to top-level rateLimits.
    let snap = result
        .get("rateLimitsByLimitId")
        .and_then(|m| m.get("codex"))
        .or(result.get("rateLimits"))?;

    // Classify windows by duration (port of upstream `classifiedWindows`):
    // 300 min = 5h, 10080 min = 7d. This is more robust than blindly taking
    // primary/secondary, which assumes a fixed ordering that newer codex
    // versions have been seen to change.
    let primary = snap.get("primary").filter(|v| !v.is_null());
    let secondary = snap.get("secondary").filter(|v| !v.is_null());
    let mut five = None;
    let mut seven = None;
    for w in [primary, secondary].into_iter().flatten() {
        match w.get("windowDurationMins").and_then(|v| v.as_i64()) {
            Some(300) if five.is_none() => five = Some(w),
            Some(10080) if seven.is_none() => seven = Some(w),
            _ => continue,
        }
    }
    // Fallback: if neither window advertised a known duration, fall back to
    // the legacy primary/secondary mapping so we still show something.
    if five.is_none() && seven.is_none() {
        five = primary;
        seven = secondary;
    }

    Some(CodexQuotaSnapshot {
        available: true,
        five_hour_used_percent: five.and_then(|w| w.get("usedPercent")).and_then(|v| v.as_f64()),
        five_hour_resets_at: five
            .and_then(|w| w.get("resetsAt"))
            .and_then(|v| v.as_i64())
            .map(epoch_to_iso),
        seven_day_used_percent: seven.and_then(|w| w.get("usedPercent")).and_then(|v| v.as_f64()),
        seven_day_resets_at: seven
            .and_then(|w| w.get("resetsAt"))
            .and_then(|v| v.as_i64())
            .map(epoch_to_iso),
        error: None,
    })
}

fn epoch_to_iso(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}
