//! Shared HTTP client construction with proxy discovery.
//!
//! A GUI app launched from the Start Menu carries no shell proxy variables,
//! and reqwest does not consult the Windows system-proxy registry on its own
//! (default-features = false). Without this, direct connections to GitHub's
//! asset CDN stall at 0% on networks where a local proxy (e.g. Clash) is the
//! only workable route. Proxy precedence: environment variables, then the
//! HKCU Internet Settings proxy (only when ProxyEnable = 1), then direct.

use reqwest::Proxy;

/// Resolve the HTTP(S) proxy to use, if any.
pub fn system_proxy() -> Option<String> {
    // 1. Shell environment (dev / terminal launches).
    for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim().to_string();
            if !value.is_empty() {
                return Some(normalize_proxy(&value));
            }
        }
    }
    // 2. Windows system proxy (GUI launches). Only honored when enabled.
    read_windows_registry_proxy().map(|p| normalize_proxy(&p))
}

/// Accept "host:port" or "http://host:port"; return a full URL.
fn normalize_proxy(value: &str) -> String {
    let v = value.trim();
    if v.contains("://") {
        v.to_string()
    } else {
        format!("http://{}", v)
    }
}

/// Read ProxyEnable / ProxyServer from HKCU\...\Internet Settings.
/// ProxyServer may be "host:port" (all protocols) or "http=a;b;https=c"
/// (per-protocol) — prefer the https entry, then http, then the whole value.
fn read_windows_registry_proxy() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(settings) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        else {
            return None;
        };
        let enabled: u32 = settings.get_value("ProxyEnable").unwrap_or(0);
        if enabled == 0 {
            return None;
        }
        let server: String = settings.get_value("ProxyServer").ok()?;
        let server = server.trim().to_string();
        if server.is_empty() {
            return None;
        }
        if server.contains('=') {
            // Per-protocol form: "http=127.0.0.1:7890;https=127.0.0.1:7891".
            for part in server.split(';') {
                let mut kv = part.splitn(2, '=');
                let proto = kv.next().unwrap_or("").trim().to_lowercase();
                let addr = kv.next().unwrap_or("").trim();
                if proto == "https" && !addr.is_empty() {
                    return Some(addr.to_string());
                }
            }
            for part in server.split(';') {
                let mut kv = part.splitn(2, '=');
                let proto = kv.next().unwrap_or("").trim().to_lowercase();
                let addr = kv.next().unwrap_or("").trim();
                if proto == "http" && !addr.is_empty() {
                    return Some(addr.to_string());
                }
            }
            return None;
        }
        Some(server)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Build a blocking client with the discovered proxy applied (if any).
pub fn blocking_client() -> reqwest::blocking::ClientBuilder {
    let mut builder = reqwest::blocking::Client::builder();
    if let Some(proxy) = system_proxy() {
        if let Ok(p) = Proxy::all(&proxy) {
            builder = builder.proxy(p);
        }
    }
    builder
}
