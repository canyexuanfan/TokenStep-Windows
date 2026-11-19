//! Trae / Trae CN subscription quota (0.1.13 T6 — port of upstream
//! v0.2.25~v0.2.27 commits `92a6844` `82d646e` `60677ee` `790148f`).
//!
//! Flow (mirrors the IDE's own authorization):
//! 1. Settings 「用浏览器登录」 opens `https://www.trae.{cn,ai}/authorization`
//!    with the IDE's query parameter set and a loopback callback URL.
//! 2. The page redirects to `http://127.0.0.1:<port>/authorize?userJwt=…`
//!    (URL-encoded JSON with Token/RefreshToken) or `refreshToken=…`.
//! 3. Quota requests use the IDE's own auth header
//!    `Authorization: Cloud-IDE-JWT <jwt>` — `Bearer` is rejected
//!    (HTTP 401 / code 1001, verified upstream with a real account).
//! 4. JWTs expire quickly; the refresh token is exchanged at
//!    `…/trae/oauth/ExchangeToken` (every refresh rotates the refresh
//!    token — the new pair is persisted back to the DPAPI registry).
//!
//! Providers: Trae CN (trae.cn, CNY credits) and Trae international
//! (trae.ai, USD usage, SG/US deployments tried in order).

use crate::net;
use serde_json::{json, Value};

const CLIENT_ID: &str = "ono9krqynydwx5";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraeProvider {
    CN,
    Intl,
}

impl TraeProvider {
    #[allow(dead_code)] // settings-page provider ids (trae_cn/trae)
    pub fn id(self) -> &'static str {
        match self {
            TraeProvider::CN => "trae_cn",
            TraeProvider::Intl => "trae",
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            TraeProvider::CN => "Trae CN",
            TraeProvider::Intl => "Trae",
        }
    }
    fn authorization_base(self) -> &'static str {
        match self {
            TraeProvider::CN => "https://www.trae.cn/authorization",
            TraeProvider::Intl => "https://www.trae.ai/authorization",
        }
    }
    fn exchange_path(self) -> &'static str {
        match self {
            TraeProvider::CN => "https://api.trae.cn/cloudide/api/v3/trae/oauth/ExchangeToken",
            TraeProvider::Intl => {
                "https://growsg-normal.trae.ai/cloudide/api/v3/trae/oauth/ExchangeToken"
            }
        }
    }
    fn plugin_version(self) -> &'static str {
        match self {
            TraeProvider::CN => "2.3.24254",
            TraeProvider::Intl => "2.3.62834",
        }
    }
    fn app_version(self) -> &'static str {
        match self {
            TraeProvider::CN => "3.3.65",
            TraeProvider::Intl => "3.5.51",
        }
    }
    fn token_account(self) -> &'static str {
        match self {
            TraeProvider::CN => "trae-cn-access-token",
            TraeProvider::Intl => "trae-access-token",
        }
    }
    fn refresh_account(self) -> &'static str {
        match self {
            TraeProvider::CN => "trae-cn-refresh-token",
            TraeProvider::Intl => "trae-refresh-token",
        }
    }
}

/// Stable per-install identifiers for the authorization query (upstream
/// derives them from device identifiers; a random id persisted in the
/// credentials registry serves the same purpose).
fn device_identifier(kind: &str) -> String {
    let account = format!("trae-device-id-{kind}");
    if let Some(existing) = crate::quota::load_quota_secret(&account) {
        if !existing.is_empty() {
            return existing;
        }
    }
    let id = crate::collector::now_iso().replace(['-', ':', '+', 'T'], "");
    let _ = crate::quota::save_quota_secret(&account, &id);
    id
}

/// Build the authorization page URL for `port` (loopback listener).
pub fn authorization_url(provider: TraeProvider, port: u16) -> String {
    let machine = device_identifier("machine");
    let device = device_identifier("device");
    let trace = crate::collector::now_iso().replace(['-', ':', '+', 'T'], "");
    let callback = format!("http://127.0.0.1:{port}/authorize");
    let params = [
        ("login_version", "1"),
        ("auth_from", "solo"),
        ("login_channel", "native_ide"),
        ("plugin_version", provider.plugin_version()),
        ("auth_type", "local"),
        ("client_id", CLIENT_ID),
        ("redirect", "0"),
        ("login_trace_id", trace.as_str()),
        ("auth_callback_url", callback.as_str()),
        ("machine_id", machine.as_str()),
        ("device_id", device.as_str()),
        ("x_device_id", device.as_str()),
        ("x_machine_id", machine.as_str()),
        ("x_device_type", "windows"),
        ("x_os_version", std::env::consts::OS),
        ("x_env", ""),
        ("x_app_version", provider.app_version()),
        ("x_app_type", "stable"),
        ("hide_saas_login", "false"),
    ];
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencode(v)))
        .collect();
    format!("{}?{}", provider.authorization_base(), query.join("&"))
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() - 1 + 1 {
            if i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Parse the callback query into (access token, refresh token) or an error
/// message. `userJwt` is a URL-encoded JSON object with mixed-case fields.
pub fn parse_callback(query: &str) -> Result<(String, String), String> {
    let mut params: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            params.insert(urldecode(k), urldecode(v));
        }
    }
    if let Some(err) = params.get("error").filter(|e| !e.is_empty()) {
        let detail = params.get("error_description").unwrap_or(err);
        return Err(detail.clone());
    }
    let token = params.get("userJwt").cloned().unwrap_or_default();
    if !token.is_empty() {
        let parsed: serde_json::Value =
            serde_json::from_str(&token).map_err(|_| "登录页返回的令牌无法解析".to_string())?;
        let pick = |keys: &[&str]| -> String {
            for k in keys {
                if let Some(v) = parsed.get(*k).and_then(|v| v.as_str()) {
                    if !v.is_empty() {
                        return v.to_string();
                    }
                }
            }
            String::new()
        };
        let access = pick(&["Token", "token"]);
        let refresh = pick(&["RefreshToken", "refreshToken"]);
        if access.is_empty() && refresh.is_empty() {
            return Err("登录页没有返回令牌".into());
        }
        return Ok((access, refresh));
    }
    if let Some(refresh) = params.get("refreshToken").filter(|r| !r.is_empty()) {
        return Ok((String::new(), refresh.clone()));
    }
    Err("登录页没有返回令牌".into())
}

fn looks_like_jwt(token: &str) -> bool {
    let segments: Vec<&str> = token.split('.').collect();
    segments.len() == 3
        && segments[0].starts_with("eyJ")
        && !segments[1].is_empty()
        && !segments[2].is_empty()
}

/// Exchange the refresh token for a fresh JWT pair (Trae rotates the refresh
/// token on every exchange). Returns (access, refresh).
pub fn exchange_refresh_token(
    provider: TraeProvider,
    refresh_token: &str,
) -> Result<(String, String), String> {
    let client = net::blocking_client()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| "网络客户端初始化失败".to_string())?;
    let body = json!({
        "ClientID": CLIENT_ID,
        "RefreshToken": refresh_token,
        "ClientSecret": "-",
        "UserID": "",
    });
    let resp = client
        .post(provider.exchange_path())
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|_| "刷新登录态失败（网络）".to_string())?;
    let value: serde_json::Value = resp
        .json()
        .map_err(|_| "登录已过期，请重新用浏览器登录".to_string())?;
    let result = value
        .get("Result")
        .or_else(|| value.get("result"))
        .unwrap_or(&value);
    let token = result
        .get("Token")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if token.is_empty() {
        return Err("登录已过期，请重新用浏览器登录".into());
    }
    let refresh = result
        .get("RefreshToken")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok((token.to_string(), refresh))
}

/// Stored credentials: (access token, refresh token) from the DPAPI registry.
/// Persist credentials (called by the settings sign-in command).
pub fn save_credentials_pub(provider: TraeProvider, access: &str, refresh: &str) {
    save_credentials(provider, access, refresh);
}

/// Load stored credentials (login-state probe for the settings page).
pub fn load_credentials_pub(provider: TraeProvider) -> (String, String) {
    load_credentials(provider)
}

/// Blocking loopback wait: accepts ONE request on the listener, reads the
/// request target, replies with a minimal success page, then parses the
/// callback query (userJwt JSON / refreshToken / error).
pub fn wait_loopback_callback(
    listener: &std::net::TcpListener,
    timeout_secs: u64,
) -> Result<(String, String), String> {
    listener
        .set_nonblocking(false)
        .map_err(|e| format!("监听器设置失败：{e}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let (mut stream, _) = listener
        .accept()
        .map_err(|_| "登录超时：未收到浏览器回调".to_string())?;
    if std::time::Instant::now() > deadline {
        return Err("登录超时：未收到浏览器回调".into());
    }
    let mut buf = Vec::new();
    {
        use std::io::Read;
        let mut take = (&mut stream).take(16 * 1024);
        let _ = take.read_to_end(&mut buf);
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let target = text
        .split("GET ")
        .nth(1)
        .and_then(|t| t.split(" HTTP").next())
        .unwrap_or("")
        .to_string();
    let query = target.split('?').nth(1).unwrap_or_default();
    let page = "HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Connection: close

<h1>✓ Trae 登录成功，可关闭此页面</h1>";
    let _ = std::io::Write::write_all(&mut stream, page.as_bytes());
    let _ = std::io::Write::flush(&mut stream);
    parse_callback(query)
}

fn load_credentials(provider: TraeProvider) -> (String, String) {
    let access = crate::quota::load_quota_secret(provider.token_account()).unwrap_or_default();
    let refresh = crate::quota::load_quota_secret(provider.refresh_account()).unwrap_or_default();
    (access, refresh)
}

pub fn save_credentials(provider: TraeProvider, access: &str, refresh: &str) {
    if !access.is_empty() {
        let _ = crate::quota::save_quota_secret(provider.token_account(), access);
    }
    if !refresh.is_empty() {
        let _ = crate::quota::save_quota_secret(provider.refresh_account(), refresh);
    }
}

fn json_num(v: &Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(x) = v.get(*k) {
            if let Some(n) = x.as_f64() {
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

fn epoch_string(v: &Value) -> Option<String> {
    let mut secs = json_num(v, &["end_time", "expire_time"])?;
    if secs <= 0.0 {
        return None;
    }
    if secs > 1_000_000_000_000.0 {
        secs /= 1000.0;
    }
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0)?;
    Some(dt.format("%Y-%m-%d").to_string())
}

fn pack_objects(payload: &Value) -> Vec<&Value> {
    payload
        .get("user_entitlement_pack_list")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Ensure a valid access token: use the cached one if it is a well-formed
/// JWT, otherwise exchange the refresh token (and persist the new pair).
fn ensure_access_token(provider: TraeProvider) -> Result<String, String> {
    let (access, refresh) = load_credentials(provider);
    if looks_like_jwt(&access) {
        return Ok(access);
    }
    if refresh.is_empty() {
        return Err("未登录：请在设置中用浏览器登录 Trae".into());
    }
    let (new_access, new_refresh) = exchange_refresh_token(provider, &refresh)?;
    save_credentials(provider, &new_access, &new_refresh);
    Ok(new_access)
}

/// Trae CN quota: CNY credits from `ide_user_ent_usage` (credits_limit -1 =
/// unlimited → no percentage window, the card omits it like upstream).
pub fn fetch_trae_cn() -> crate::quota::ProviderQuota {
    fetch_one(TraeProvider::CN)
}

/// Trae international quota: USD usage from `user_current_entitlement_list`
/// (SG deployment first, then US).
pub fn fetch_trae_intl() -> crate::quota::ProviderQuota {
    fetch_one(TraeProvider::Intl)
}

fn qp(provider: TraeProvider) -> crate::quota::QuotaProvider {
    match provider {
        TraeProvider::CN => crate::quota::QuotaProvider::TraeCN,
        TraeProvider::Intl => crate::quota::QuotaProvider::Trae,
    }
}

fn fetch_one(provider: TraeProvider) -> crate::quota::ProviderQuota {
    let display = provider.display_name();
    let access = match ensure_access_token(provider) {
        Ok(t) => t,
        Err(e) => {
            return crate::quota::unavailable_for(
                qp(provider),
                &e,
                crate::quota::QuotaStatus::Unavailable,
            )
        }
    };
    let client = match net::blocking_client()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return crate::quota::unavailable_for(
                qp(provider),
                "网络客户端初始化失败",
                crate::quota::QuotaStatus::Unavailable,
            )
        }
    };
    let (url, body) = match provider {
        TraeProvider::CN => (
            "https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage",
            json!({"require_usage": true, "req_source": 0}).to_string(),
        ),
        TraeProvider::Intl => (
            "https://api-sg-central.trae.ai/trae/api/v1/pay/user_current_entitlement_list",
            "{}".to_string(),
        ),
    };
    let mut payload = match authorized_payload_checked(&client, &access, url, &body) {
        Ok(p) => p,
        Err(e) => {
            return crate::quota::unavailable_for(
                qp(provider),
                &e,
                crate::quota::QuotaStatus::Unavailable,
            )
        }
    };
    if provider == TraeProvider::Intl {
        // SG may return an empty pack list — try the US deployment once.
        if pack_objects(&payload).is_empty() {
            if let Ok(p) = authorized_payload_checked(
                &client,
                &access,
                "https://api-us-east.trae.ai/trae/api/v1/pay/user_current_entitlement_list",
                "{}",
            ) {
                payload = p;
            }
        }
    }
    let windows = quota_windows(provider, &payload);
    if windows.is_empty() {
        return crate::quota::unavailable_for(
            qp(provider),
            &format!("{display} 额度暂不可用"),
            crate::quota::QuotaStatus::Unavailable,
        );
    }
    crate::quota::ProviderQuota {
        provider: qp(provider),
        windows,
        status: crate::quota::QuotaStatus::Available,
        message: None,
    }
}

fn authorized_payload_checked(
    client: &reqwest::blocking::Client,
    token: &str,
    url: &str,
    body: &str,
) -> Result<Value, String> {
    if !looks_like_jwt(token) {
        return Err("令牌格式不正确（需要浏览器登录返回的 JWT）".into());
    }
    let resp = client
        .post(url)
        .header("Authorization", format!("Cloud-IDE-JWT {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|_| "额度请求失败（网络）".to_string())?;
    let status = resp.status().as_u16();
    let value: serde_json::Value = resp.json().unwrap_or(json!({}));
    let api_code = value.get("code").and_then(|v| v.as_f64()).map(|f| f as i64);
    if !(200..300).contains(&status) {
        if status == 401 || api_code == Some(1001) {
            return Err("登录态已过期，请重新用浏览器登录".into());
        }
        return Err(format!("额度请求失败（HTTP {status}）"));
    }
    if api_code == Some(1001) {
        return Err("登录态已过期，请重新用浏览器登录".into());
    }
    Ok(value)
}

fn quota_windows(provider: TraeProvider, payload: &Value) -> Vec<crate::quota::QuotaWindow> {
    let packs = pack_objects(payload);
    match provider {
        TraeProvider::CN => {
            let mut limit = 0.0f64;
            let mut used = 0.0f64;
            let mut unlimited = false;
            let mut soonest: Option<String> = None;
            for pack in &packs {
                let base = pack.get("entitlement_base_info").unwrap_or(&Value::Null);
                let quota = base.get("quota").unwrap_or(&Value::Null);
                let Some(credits_limit) = json_num(quota, &["credits_limit"]) else {
                    continue;
                };
                let credits_used = pack
                    .get("usage")
                    .and_then(|u| json_num(u, &["credits_amount"]))
                    .unwrap_or(0.0);
                if credits_limit == -1.0 {
                    unlimited = true;
                } else if credits_limit >= 0.0 {
                    limit += credits_limit;
                    used += credits_used.max(0.0);
                }
                for v in [
                    base.get("end_time"),
                    base.get("expire_time"),
                    pack.get("end_time"),
                ] {
                    if let Some(s) = v.and_then(epoch_string) {
                        if soonest.is_none() || s < *soonest.as_ref().unwrap() {
                            soonest = Some(s);
                        }
                    }
                }
            }
            if unlimited || limit <= 0.0 {
                return vec![];
            }
            let used_percent = (used / limit * 100.0).clamp(0.0, 100.0);
            vec![crate::quota::QuotaWindow {
                kind: crate::quota::QuotaWindowKind::MonthlyCredits,
                used_percent,
                remaining: Some((limit - used).max(0.0)),
                total: Some(limit),
                resets_at: soonest,
            }]
        }
        TraeProvider::Intl => {
            let Some(pack) = packs.iter().find(|p| {
                p.get("usage").is_some() || p.pointer("/entitlement_base_info/quota").is_some()
            }) else {
                return vec![];
            };
            let base = pack.get("entitlement_base_info").unwrap_or(&Value::Null);
            let quota = base.get("quota").unwrap_or(&Value::Null);
            let total = json_num(quota, &["basic_usage_limit"]).unwrap_or(0.0)
                + json_num(quota, &["bonus_usage_limit"]).unwrap_or(0.0);
            let usage = pack.get("usage").unwrap_or(&Value::Null);
            let used = json_num(usage, &["basic_usage_amount"]).unwrap_or(0.0)
                + json_num(usage, &["bonus_usage_amount"]).unwrap_or(0.0);
            if total <= 0.0 {
                return vec![];
            }
            vec![crate::quota::QuotaWindow {
                kind: crate::quota::QuotaWindowKind::Spend,
                used_percent: (used / total * 100.0).clamp(0.0, 100.0),
                remaining: Some((total - used).max(0.0)),
                total: Some(total),
                resets_at: base.get("end_time").and_then(epoch_string),
            }]
        }
    }
}
