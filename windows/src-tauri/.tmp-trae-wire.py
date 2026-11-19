import io, os
os.chdir(r'F:\zcode\projects\tokenstep\TokenStep\windows\src-tauri')

# lib.rs
p = 'src/lib.rs'
s = io.open(p, encoding='utf-8').read()
s = s.replace('mod token_rank;', 'mod token_rank;\nmod trae;', 1)

p2 = 'src/models.rs'
s2 = io.open(p2, encoding='utf-8').read()
a2 = '''    #[serde(rename = "antigravity_quota_enabled", default = "default_true")]
    pub antigravity_quota_enabled: bool,'''
assert a2 in s2, 'models anchor'
s2 = s2.replace(a2, a2 + '''
    /// Trae / Trae CN subscription quota cards (0.1.13 T6). Default off —
    /// they require a browser sign-in before showing anything.
    #[serde(rename = "trae_cn_quota_enabled", default)]
    pub trae_cn_quota_enabled: bool,
    #[serde(rename = "trae_quota_enabled", default)]
    pub trae_quota_enabled: bool,''', 1)
a3 = '''            antigravity_quota_enabled: true,'''
assert a3 in s2, 'models default anchor'
s2 = s2.replace(a3, a3 + '''
            trae_cn_quota_enabled: false,
            trae_quota_enabled: false,''', 1)
io.open(p2, 'w', encoding='utf-8', newline='').write(s2)
print('models.rs ok')

p3 = 'src/settings.rs'
s3 = io.open(p3, encoding='utf-8').read()
a3 = '''        antigravity_quota_enabled: s.antigravity_quota_enabled,'''
assert a3 in s3, 'settings anchor'
s3 = s3.replace(a3, a3 + '''
        trae_cn_quota_enabled: s.trae_cn_quota_enabled,
        trae_quota_enabled: s.trae_quota_enabled,''', 1)
io.open(p3, 'w', encoding='utf-8', newline='').write(s3)
print('settings.rs ok')

p = 'src/lib.rs'
s = io.open(p, encoding='utf-8').read()
a4 = '''    if s.antigravity_quota_enabled {
        enabled.push(quota::QuotaProvider::Antigravity);
    }'''
assert a4 in s, 'lib gating anchor'
s = s.replace(a4, a4 + '''
    if s.trae_cn_quota_enabled {
        enabled.push(quota::QuotaProvider::TraeCN);
    }
    if s.trae_quota_enabled {
        enabled.push(quota::QuotaProvider::Trae);
    }''', 1)
a5 = '''        "antigravity" => s.antigravity_quota_enabled = enabled,'''
assert a5 in s, 'provider arm anchor'
s = s.replace(a5, a5 + '''
        "trae_cn" => s.trae_cn_quota_enabled = enabled,
        "trae" => s.trae_quota_enabled = enabled,''', 1)
a7 = '''#[tauri::command]
fn refresh(app: tauri::AppHandle) {'''
assert a7 in s, 'refresh anchor'
add7 = '''/// Trae browser sign-in (0.1.13 T6): opens the authorization page with a
/// loopback callback listener; blocks up to 240s waiting for the redirect,
/// then persists the returned JWT pair (DPAPI registry).
#[tauri::command]
fn trae_signin(provider: String) -> Result<serde_json::Value, String> {
    let p = match provider.as_str() {
        "trae_cn" => trae::TraeProvider::CN,
        "trae" => trae::TraeProvider::Intl,
        _ => return Err("unknown provider".into()),
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("回环端口监听失败：{e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("回环端口读取失败：{e}"))?
        .port();
    let url = trae::authorization_url(p, port);
    open(url.as_str()).map_err(|e| format!("无法打开浏览器：{e}"))?;
    let (access, refresh) = trae::wait_loopback_callback(&listener, 240)?;
    trae::save_credentials_pub(p, &access, &refresh);
    Ok(serde_json::json!({ "ok": true, "provider": provider }))
}

/// Trae login state for the settings page.
#[tauri::command]
fn trae_login_state(provider: String) -> serde_json::Value {
    let p = match provider.as_str() {
        "trae_cn" => trae::TraeProvider::CN,
        "trae" => trae::TraeProvider::Intl,
        _ => return serde_json::json!({ "loggedIn": false }),
    };
    let (access, refresh) = trae::load_credentials_pub(p);
    serde_json::json!({ "loggedIn": !access.is_empty() || !refresh.is_empty() })
}

''' + a7
s = s.replace(a7, add7, 1)
a8 = '''            refresh_cursor_usage,'''
assert a8 in s, 'invoke anchor'
s = s.replace(a8, '''            refresh_cursor_usage,
            trae_signin,
            trae_login_state,''', 1)
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('lib.rs ok')

p = 'src/trae.rs'
s = io.open(p, encoding='utf-8').read()
a8 = 'fn load_credentials(provider: TraeProvider) -> (String, String) {'
assert a8 in s, 'trae load anchor'
add8 = '''/// Persist credentials (called by the settings sign-in command).
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
        let mut take = stream.take(16 * 1024);
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
    let page = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<h1>✓ Trae 登录成功\uff0c可关闭此页面</h1>";
    let _ = std::io::Write::write_all(&mut stream, page.as_bytes());
    let _ = std::io::Write::flush(&mut stream);
    parse_callback(&query)
}

''' + a8
s = s.replace(a8, add8, 1)
s = s.replace('fn save_credentials(provider: TraeProvider, access: &str, refresh: &str) {',
              'pub fn save_credentials(provider: TraeProvider, access: &str, refresh: &str) {', 1)
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('trae.rs ok')
