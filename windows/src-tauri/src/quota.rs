//! Unified quota system — a port of upstream v0.2.2 QuotaModels +
//! GLM/Kimi/Grok quota services + QuotaRefreshCoordinator.
//!
//! Card rule (v0.2.1): `should_display = is_available` — an unconfigured or
//! failed provider renders NOTHING, never 0%. Failures keep the last value.

use crate::net;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaProvider {
    Codex,
    Claude,
    Cursor,
    Glm,
    Kimi,
    Grok,
}

impl QuotaProvider {
    pub fn id(&self) -> &'static str {
        match self {
            QuotaProvider::Codex => "codex",
            QuotaProvider::Claude => "claude",
            QuotaProvider::Cursor => "cursor",
            QuotaProvider::Glm => "glm",
            QuotaProvider::Kimi => "kimi",
            QuotaProvider::Grok => "grok",
        }
    }
    pub fn display_name(&self) -> &'static str {
        match self {
            QuotaProvider::Codex => "Codex",
            QuotaProvider::Claude => "Claude Code",
            QuotaProvider::Cursor => "Cursor",
            QuotaProvider::Glm => "GLM",
            QuotaProvider::Kimi => "Kimi",
            QuotaProvider::Grok => "Grok",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind {
    FiveHour,
    SevenDay,
    Session,
    Weekly,
    MonthlyCredits,
    TokenWindow,
    Spend,
    CursorModels,
    OtherModels,
}

impl QuotaWindowKind {
    pub fn title(&self) -> &'static str {
        use QuotaWindowKind::*;
        match self {
            FiveHour => "5 小时",
            SevenDay => "7 天",
            Session => "会话",
            Weekly => "本周",
            MonthlyCredits => "本月额度",
            TokenWindow => "Token 窗",
            Spend => "花费",
            CursorModels => "Cursor 模型",
            OtherModels => "其他模型",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaStatus {
    Available,
    Unavailable,
    NotLoggedIn,
    WrongKeyType,
    NeedsLogin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub kind: QuotaWindowKind,
    #[serde(default)]
    pub used_percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

impl QuotaWindow {
    /// Upstream `remainingPercent`: remaining/total*100 when total>0, else
    /// 100 - used, clamped to 0-100.
    pub fn remaining_percent(&self) -> f64 {
        let v = if self.total.unwrap_or(0.0) > 0.0 {
            self.remaining.unwrap_or(0.0) / self.total.unwrap() * 100.0
        } else {
            100.0 - self.used_percent
        };
        v.clamp(0.0, 100.0)
    }
    pub fn is_low(&self) -> bool {
        self.remaining_percent() < 20.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderQuota {
    pub provider: QuotaProvider,
    pub windows: Vec<QuotaWindow>,
    pub status: QuotaStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ProviderQuota {
    pub fn is_available(&self) -> bool {
        self.status == QuotaStatus::Available && !self.windows.is_empty()
    }
    pub fn should_display(&self) -> bool {
        self.is_available()
    }
    pub fn lowest_remaining_percent(&self) -> Option<f64> {
        self.windows.iter().map(|w| w.remaining_percent()).reduce(f64::min)
    }
    pub fn is_low(&self) -> bool {
        self.windows.iter().any(|w| w.is_low())
    }
}

// ── credential storage (Windows Credential Manager ≈ macOS Keychain) ─────

mod secrets {
    use winreg::enums::*;
    use winreg::RegKey;

    // A lightweight stand-in for the Keychain: DPAPI isn't wired here, so the
    // token lives in HKCU under a dedicated app subkey. Scope-local, per-user,
    // not roamable — acceptable for opt-in quota probes.
    const BASE: &str = r"Software\TokenStep\credentials";

    pub fn save(account: &str, secret: &str) -> Result<(), String> {
        let k = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(BASE)
            .map_err(|e| e.to_string())?
            .0;
        k.set_value(account, &secret).map_err(|e| e.to_string())
    }

    pub fn load(account: &str) -> Option<String> {
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(BASE).ok()?;
        k.get_value(account).ok()
    }

    pub fn clear(account: &str) -> Result<(), String> {
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(BASE, KEY_SET_VALUE)
            .map_err(|e| e.to_string())?;
        let _ = k.delete_value(account);
        Ok(())
    }
}

pub fn save_quota_secret(account: &str, secret: &str) -> Result<(), String> {
    // Upstream guards: kimi rejects sk-, grok rejects plain xai- keys.
    if account.starts_with("kimi") && secret.trim().starts_with("sk-") {
        return Err("不要使用开放平台 key（sk- 前缀），需要 OAuth access_token".into());
    }
    if account.starts_with("grok") && secret.trim().starts_with("xai-") {
        return Err("普通 xAI API key 无效，需要 grok login 的会话 token".into());
    }
    secrets::save(account, secret)
}

pub fn load_quota_secret(account: &str) -> Option<String> {
    secrets::load(account)
}

pub fn clear_quota_secret(account: &str) -> Result<(), String> {
    secrets::clear(account)
}

// ── shared JSON helpers (upstream QuotaJSON.percent) ──────────────────────

fn json_number(v: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(x) = v.get(*k) {
            if let Some(n) = x.as_f64().or_else(|| x.as_i64().map(|i| i as f64)) {
                return Some(n);
            }
            if let Some(s) = x.as_str() {
                if let Ok(n) = s.parse::<f64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn percent_from(v: &serde_json::Value) -> Option<f64> {
    let used = json_number(v, &["used", "usedPercent", "used_percent", "percentUsed", "autoPercentUsed", "apiPercentUsed"])?;
    let total = json_number(v, &["total", "totalLimit", "monthlyLimit"]);
    let remaining = json_number(v, &["remaining", "remainingPercent"]);
    if used <= 1.0 && total.is_none() {
        Some((used * 100.0).clamp(0.0, 100.0)) // ratio
    } else if let Some(t) = total.filter(|t| *t > 0.0) {
        Some((used / t * 100.0).clamp(0.0, 100.0))
    } else if used <= 100.0 {
        Some(used.clamp(0.0, 100.0)) // already percent
    } else if let (Some(t), Some(r)) = (total, remaining) {
        Some(((t - r) / t * 100.0).clamp(0.0, 100.0))
    } else {
        Some(0.0)
    }
}

fn http_json(url: &str, headers: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let client = net::blocking_client()
        .user_agent("TokenStep")
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(url).header("Accept", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req.send().map_err(|_| "请求失败".to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    resp.json().map_err(|_| "解析失败".to_string())
}

// ── GLM ───────────────────────────────────────────────────────────────────

fn glm_token() -> Option<String> {
    for key in ["ZAI_API_KEY", "ZHIPUAI_API_KEY", "GLM_API_KEY"] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    secrets::load("glm-api-key")
}

pub fn fetch_glm() -> ProviderQuota {
    let provider = QuotaProvider::Glm;
    let Some(token) = glm_token() else {
        return unavailable(provider, Some("未配置 GLM API Key"), QuotaStatus::Unavailable);
    };
    if token.starts_with("payg") || token.starts_with("sk-pay") {
        return unavailable(provider, Some("当前 key 非订阅计划"), QuotaStatus::WrongKeyType);
    }
    let auth = format!("Bearer {}", token);
    let urls = [
        "https://open.bigmodel.cn/api/paas/v4/usage",
        "https://open.bigmodel.cn/api/biz/v1/subscription/usage",
        "https://api.z.ai/api/coding/usage",
    ];
    for url in urls {
        if let Ok(value) = http_json(url, &[("Authorization", auth.clone())]) {
            let data = value.get("data").unwrap_or(&value).clone();
            let mut windows = Vec::new();
            for (keys, kind) in [
                (vec!["token_window", "tokenWindow"], QuotaWindowKind::TokenWindow),
                (vec!["daily", "day"], QuotaWindowKind::FiveHour),
                (vec!["monthly", "month", "subscription"], QuotaWindowKind::MonthlyCredits),
            ] {
                for k in &keys {
                    if let Some(node) = data.get(*k) {
                        if let Some(p) = percent_from(node) {
                            windows.push(QuotaWindow {
                                kind,
                                used_percent: p,
                                remaining: json_number(node, &["remaining"]),
                                total: json_number(node, &["total"]),
                                resets_at: None,
                            });
                        }
                        break;
                    }
                }
            }
            if !windows.is_empty() {
                return ProviderQuota { provider, windows, status: QuotaStatus::Available, message: None };
            }
        }
    }
    unavailable(provider, Some("GLM 额度暂不可用"), QuotaStatus::Unavailable)
}

// ── Kimi ──────────────────────────────────────────────────────────────────

fn kimi_token() -> Option<String> {
    // ~/.kimi/{auth,credentials,oauth,config}.json → access_token-ish fields.
    let base = crate::paths::home_dir().join(".kimi");
    for name in ["auth.json", "credentials.json", "oauth.json", "config.json"] {
        let path = base.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        for key in ["access_token", "accessToken", "token", "oauth_token"] {
            if let Some(tok) = v.get(key).and_then(|x| x.as_str()) {
                let tok = tok.trim();
                if !tok.is_empty() && !tok.starts_with("sk-") {
                    return Some(tok.to_string());
                }
            }
        }
    }
    secrets::load("kimi-access-token")
}

pub fn fetch_kimi() -> ProviderQuota {
    let provider = QuotaProvider::Kimi;
    let Some(token) = kimi_token() else {
        return unavailable(provider, Some("未登录 Kimi"), QuotaStatus::NotLoggedIn);
    };
    let auth = format!("Bearer {}", token);
    for url in ["https://api.kimi.com/coding/usage", "https://www.kimi.com/api/coding/usage"] {
        if let Ok(value) = http_json(url, &[("Authorization", auth.clone())]) {
            let data = value.get("data").unwrap_or(&value).clone();
            let mut windows = Vec::new();
            for (keys, kind) in [
                (vec!["session", "session_usage"], QuotaWindowKind::Session),
                (vec!["weekly", "week", "weekly_usage"], QuotaWindowKind::Weekly),
            ] {
                for k in &keys {
                    if let Some(node) = data.get(*k) {
                        if let Some(p) = percent_from(node) {
                            windows.push(QuotaWindow {
                                kind,
                                used_percent: p,
                                remaining: json_number(node, &["remaining"]),
                                total: json_number(node, &["total"]),
                                resets_at: None,
                            });
                        }
                        break;
                    }
                }
            }
            if windows.is_empty() {
                if let Some(p) = percent_from(&data) {
                    windows.push(QuotaWindow {
                        kind: QuotaWindowKind::Weekly,
                        used_percent: p,
                        remaining: json_number(&data, &["remaining"]),
                        total: json_number(&data, &["total"]),
                        resets_at: None,
                    });
                }
            }
            if !windows.is_empty() {
                return ProviderQuota { provider, windows, status: QuotaStatus::Available, message: None };
            }
        }
    }
    unavailable(provider, Some("Kimi 额度暂不可用"), QuotaStatus::Unavailable)
}

// ── Grok ──────────────────────────────────────────────────────────────────

fn grok_token_and_user() -> Option<(String, String)> {
    if let Some(tok) = secrets::load("grok-access-token") {
        if !tok.trim().is_empty() {
            let user = find_json_user_id(&crate::paths::home_dir().join(".grok").join("auth.json"))
                .unwrap_or_default();
            return Some((tok, user));
        }
    }
    let path = crate::paths::home_dir().join(".grok").join("auth.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return None };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return None };
    for key in ["access_token", "accessToken", "sessionToken", "token", "key"] {
        if let Some(tok) = v.get(key).and_then(|x| x.as_str()) {
            let tok = tok.trim();
            let looks_session = !tok.starts_with("xai-") && (tok.starts_with("eyJ") || tok.len() >= 40);
            if looks_session {
                let user = find_json_user_id(&path).unwrap_or_default();
                return Some((tok.to_string(), user));
            }
            if tok.starts_with("xai-") {
                return None; // plain xAI key is explicitly rejected
            }
        }
    }
    None
}

fn find_json_user_id(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    fn walk(v: &serde_json::Value) -> Option<String> {
        for key in ["user_id", "userId", "principal_id", "principalId"] {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
        for (_k, child) in v.as_object()? {
            if let Some(found) = walk(child) {
                return Some(found);
            }
        }
        None
    }
    walk(&v)
}

pub fn fetch_grok() -> ProviderQuota {
    let provider = QuotaProvider::Grok;
    let Some((token, user)) = grok_token_and_user() else {
        return unavailable(provider, Some("需要 grok login"), QuotaStatus::NeedsLogin);
    };
    let auth = format!("Bearer {}", token);
    let mut windows: BTreeMap<QuotaWindowKind, QuotaWindow> = BTreeMap::new();
    for url in [
        "https://cli-chat-proxy.grok.com/v1/billing?format=credits",
        "https://cli-chat-proxy.grok.com/v1/billing",
    ] {
        let headers: Vec<(&str, String)> = vec![
            ("Authorization", auth.clone()),
            ("X-XAI-Token-Auth", "xai-grok-cli".to_string()),
            ("x-userid", user.clone()),
        ];
        let Ok(value) = http_json(url, &headers) else { continue };
        let data = value.get("data").unwrap_or(&value);
        if let Some(usage) = data.get("creditUsagePercent").and_then(|v| v.as_f64()) {
            let is_weekly = data
                .get("currentPeriod")
                .and_then(|p| p.get("type"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_uppercase().contains("WEEK"))
                .unwrap_or(false);
            let kind = if is_weekly { QuotaWindowKind::Weekly } else { QuotaWindowKind::MonthlyCredits };
            windows.insert(kind, QuotaWindow {
                kind,
                used_percent: usage.clamp(0.0, 100.0),
                remaining: None,
                total: None,
                resets_at: None,
            });
        } else if let (Some(used), Some(limit)) = (
            json_number(data, &["used"]),
            json_number(data, &["monthlyLimit"]),
        ) {
            if limit > 0.0 {
                windows.insert(QuotaWindowKind::MonthlyCredits, QuotaWindow {
                    kind: QuotaWindowKind::MonthlyCredits,
                    used_percent: (used / limit * 100.0).clamp(0.0, 100.0),
                    remaining: Some(limit - used),
                    total: Some(limit),
                    resets_at: None,
                });
            }
        }
    }
    if windows.is_empty() {
        return unavailable(provider, Some("Grok 额度暂不可用"), QuotaStatus::Unavailable);
    }
    ProviderQuota {
        provider,
        windows: windows.into_values().collect(),
        status: QuotaStatus::Available,
        message: None,
    }
}

// ── Cursor quota (usage-summary two-tier) ────────────────────────────────

pub fn fetch_cursor() -> ProviderQuota {
    let provider = QuotaProvider::Cursor;
    let Some(cred) = crate::cursor_usage::read_credential() else {
        return unavailable(provider, Some("未登录 Cursor"), QuotaStatus::NotLoggedIn);
    };
    let cookie = format!("WorkosCursorSessionToken={}::{}", cred.user_id, cred.access_token);
    let client = match net::blocking_client()
        .user_agent("TokenStep")
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return unavailable(provider, Some("Cursor 额度暂不可用"), QuotaStatus::Unavailable),
    };
    let parse = |value: &serde_json::Value| -> Option<Vec<QuotaWindow>> {
        let usage = value.get("individualUsage")?;
        let node = usage.get("plan")?;
        let mut windows = Vec::new();
        let auto = json_number(node, &["autoPercentUsed"]);
        let api = json_number(node, &["apiPercentUsed"]);
        if let Some(a) = auto {
            windows.push(QuotaWindow {
                kind: QuotaWindowKind::CursorModels,
                used_percent: a.clamp(0.0, 100.0),
                remaining: None, total: None, resets_at: None,
            });
        }
        if let Some(a) = api {
            windows.push(QuotaWindow {
                kind: QuotaWindowKind::OtherModels,
                used_percent: a.clamp(0.0, 100.0),
                remaining: None, total: None, resets_at: None,
            });
        }
        if windows.is_empty() {
            // legacy fallback keys
            let a = json_number(node, &["autoModelUsagePercent", "premiumRequestsUsedPercent"]);
            if let Some(a) = a {
                windows.push(QuotaWindow {
                    kind: QuotaWindowKind::CursorModels,
                    used_percent: a.clamp(0.0, 100.0),
                    remaining: None, total: None, resets_at: None,
                });
            }
        }
        let resets = json_number(node, &["billingCycleEnd", "resetsAt"])
            .map(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
            .flatten()
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string());
        if let Some(r) = resets {
            for w in &mut windows {
                w.resets_at = Some(r.clone());
            }
        }
        Some(windows)
    };
    let value: serde_json::Value = match client
        .get("https://cursor.com/api/usage-summary")
        .header("Accept", "application/json")
        .header("Cookie", &cookie)
        .send()
    {
        Ok(r) if r.status().is_success() => r.json().unwrap_or_default(),
        _ => serde_json::Value::default(),
    };
    if let Some(windows) = parse(&value) {
        if !windows.is_empty() {
            return ProviderQuota { provider, windows, status: QuotaStatus::Available, message: None };
        }
    }
    // legacy endpoint
    let legacy = client
        .get(format!("https://cursor.com/api/usage?user={}", cred.user_id))
        .header("Accept", "application/json")
        .header("Cookie", &cookie)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|mut r| r.json::<serde_json::Value>().ok());
    if let Some(v) = legacy {
        if let Some(windows) = parse(&v) {
            if !windows.is_empty() {
                return ProviderQuota { provider, windows, status: QuotaStatus::Available, message: None };
            }
        }
    }
    unavailable(provider, Some("Cursor 额度暂不可用"), QuotaStatus::Unavailable)
}

// ── coordinator ───────────────────────────────────────────────────────────

fn unavailable(provider: QuotaProvider, message: Option<&str>, status: QuotaStatus) -> ProviderQuota {
    ProviderQuota {
        provider,
        windows: vec![],
        status,
        message: message.map(String::from),
    }
}

/// Fetch all enabled providers concurrently; the whole batch gets a 12s
/// ceiling (upstream QuotaRefreshCoordinator). Each provider failure maps to
/// a classified status via its error text.
pub fn fetch_providers(providers: &[QuotaProvider]) -> BTreeMap<String, ProviderQuota> {
    let mut out = BTreeMap::new();
    let (tx, rx) = std::sync::mpsc::channel::<ProviderQuota>();
    let mut handles = Vec::new();
    for p in providers {
        let tx = tx.clone();
        let p = *p;
        handles.push(std::thread::spawn(move || {
            let quota = match p {
                QuotaProvider::Codex | QuotaProvider::Claude => {
                    // handled by the existing codex/claude services upstream;
                    // on Windows these stay on their dedicated cards.
                    unavailable(p, None, QuotaStatus::Unavailable)
                }
                QuotaProvider::Cursor => fetch_cursor(),
                QuotaProvider::Glm => fetch_glm(),
                QuotaProvider::Kimi => fetch_kimi(),
                QuotaProvider::Grok => fetch_grok(),
            };
            let _ = tx.send(quota);
        }));
    }
    drop(tx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    while let Ok(quota) = rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
        out.insert(quota.provider.id().to_string(), quota);
    }
    for p in providers {
        out.entry(p.id().to_string())
            .or_insert_with(|| unavailable(*p, Some("暂不可用"), QuotaStatus::Unavailable));
    }
    out
}
