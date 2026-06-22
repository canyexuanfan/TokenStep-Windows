//! TokenStep Windows app — Tauri 2 entry point.
//!
//! Provides:
//!   - a system-tray icon showing today's progress ring + token count
//!   - a dashboard window (Today / History / Stats / Privacy)
//!   - Tauri commands the frontend calls to fetch data & update settings
//!   - a background refresh timer driven by the user's refresh interval

mod collector;
mod claude_quota;
mod codex_quota;
mod models;
mod paths;
mod pricing;
mod settings;
mod token_rank;
mod update;

use models::{TokenStepSettings, UsageSnapshot};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WebviewWindowBuilder,
};
use tauri_plugin_shell::ShellExt;

/// Shared state holding the latest snapshot + settings, refreshed by the timer
/// and read by the frontend via commands.
#[derive(Default)]
struct AppState {
    snapshot: Mutex<Option<UsageSnapshot>>,
    settings: Mutex<TokenStepSettings>,
    refreshing: Mutex<bool>,
    #[allow(dead_code)]
    last_error: Mutex<Option<String>>,
}

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, Arc<AppState>>) -> UsageSnapshot {
    state
        .snapshot
        .lock()
        .clone()
        .unwrap_or_else(UsageSnapshot::empty)
}

#[tauri::command]
fn get_settings(_state: tauri::State<'_, Arc<AppState>>) -> TokenStepSettings {
    settings::load()
}

#[tauri::command]
fn set_daily_goal(tokens: i64) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.daily_goal_tokens = tokens.max(1_000_000);
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

#[tauri::command]
fn set_refresh_interval(seconds: i64) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.refresh_interval_seconds = match seconds {
        0 | 60 | 300 | 900 => seconds,
        _ => 60,
    };
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

#[tauri::command]
fn is_refreshing(state: tauri::State<'_, Arc<AppState>>) -> bool {
    *state.refreshing.lock()
}

/// Read the Codex rate-limit quota (5h + 7d windows) via the app-server.
#[tauri::command]
fn read_codex_quota() -> codex_quota::CodexQuotaSnapshot {
    codex_quota::read()
}

#[tauri::command]
fn set_close_to_tray(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.close_to_tray = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Reset all settings to defaults (port of macOS "restore defaults" footer).
#[tauri::command]
fn reset_settings() -> Result<TokenStepSettings, String> {
    let s = TokenStepSettings::default();
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Toggle whether updates are checked automatically on launch.
#[tauri::command]
fn set_auto_update_enabled(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.auto_update_enabled = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Toggle whether to prompt before downloading an update.
#[tauri::command]
fn set_ask_before_downloading_updates(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.ask_before_downloading_updates = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Toggle whether to only install verified/signed updates.
#[tauri::command]
fn set_require_verified_updates(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.require_verified_updates = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Toggle launch-on-startup by writing/removing the HKCU Run key. Uses the
/// current executable path so the entry stays correct after updates.
#[tauri::command]
fn set_autostart(enabled: bool) -> Result<TokenStepSettings, String> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE | KEY_READ,
        )
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let value = format!("\"{}\"", exe.display());
    if enabled {
        run.set_value("TokenStep", &value).map_err(|e| e.to_string())?;
    } else {
        // Ignore "value not found" when removing — already off is fine.
        let _ = run.delete_value("TokenStep");
    }
    let mut s = settings::load();
    s.autostart = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

#[tauri::command]
fn set_theme(theme: String) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.theme = theme;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

#[tauri::command]
fn set_language(app: tauri::AppHandle, language: String) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.language = language;
    settings::save(&s).map_err(|e| e.to_string())?;
    // Rebuild the tray menu so the new language takes effect immediately.
    rebuild_tray_menu(&app);
    Ok(settings::load())
}

/// Toggle whether the Codex quota card is shown on the Today view. Mirrors
/// the upstream `showCodexQuota` setting.
#[tauri::command]
fn set_show_codex_quota(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.show_codex_quota = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Read the Claude Code usage quota (5h + 7d windows) via Anthropic's OAuth
/// usage API. Requires the user to have signed in to Claude Code.
#[tauri::command]
fn read_claude_quota() -> codex_quota::CodexQuotaSnapshot {
    claude_quota::read()
}

/// Read the TokenRank leaderboard (today's top users) and locate the user's
/// own entry when a user id is configured.
#[tauri::command]
fn read_token_rank() -> token_rank::TokenRankSnapshot {
    let s = settings::load();
    let user_id = s.token_rank_user_id.unwrap_or_default();
    token_rank::read(&user_id, "total", "today")
}

/// Toggle whether the TokenRank leaderboard card is shown on the Today view.
#[tauri::command]
fn set_show_token_rank(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.show_token_rank = enabled;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Set the user's scys.com TokenRank user id (digits only).
#[tauri::command]
fn set_token_rank_user_id(user_id: String) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.token_rank_user_id = Some(token_rank::clean_user_id(&user_id));
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Save a PNG screenshot SILENTLY (no dialog) to `<exe_dir>/share/` or the
/// configured screenshot_dir. Returns the full saved path.
#[tauri::command]
fn save_screenshot(_app: tauri::AppHandle, data: Vec<u8>) -> Result<String, String> {
    let today = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string();
    // Resolve the save dir: configured, else <exe_dir>/share.
    let dir = {
        let cfg = settings::load();
        if !cfg.screenshot_dir.is_empty() {
            std::path::PathBuf::from(&cfg.screenshot_dir)
        } else {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("share")))
                .unwrap_or_else(|| std::path::PathBuf::from("share"))
        }
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Auto-rename if file exists: TokenStep-today-2026-06-20.png, -2.png, -3.png...
    let mut path = dir.join(format!("TokenStep-today-{}.png", today));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("TokenStep-today-{}-{}.png", today, n));
        n += 1;
    }
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Set the default screenshot save directory (called from Settings UI).
#[tauri::command]
fn set_screenshot_dir(dir: String) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.screenshot_dir = dir;
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(settings::load())
}

/// Pick a folder via native dialog (for the screenshot-dir setting).
#[tauri::command]
fn pick_folder(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app.dialog().file().blocking_pick_folder();
    match path {
        Some(p) => {
            let p = p.into_path().map_err(|e| e.to_string())?;
            Ok(p.to_string_lossy().to_string())
        }
        None => Ok(String::new()),
    }
}

/// Check GitHub for a newer release. Always succeeds — on network failure it
/// returns an `UpdateCheck` with `has_update: false` rather than erroring, so
/// the settings-page "check now" button never shows a hard error.
#[tauri::command]
fn check_for_update() -> update::UpdateCheck {
    update::check()
}

/// Open a URL in the user's default browser (used by the update badge / tray
/// item to send the user to the Releases page). Requires the `shell:allow-open`
/// permission, already granted in capabilities/default.json.
#[tauri::command]
fn open_release_page(app: tauri::AppHandle, url: String) {
    // `open` is deprecated in favor of tauri-plugin-opener, but switching plugins
    // would add a dependency for a one-line feature. Keep it until the plugin is
    // removed or the API hard-breaks.
    #[allow(deprecated)]
    let _ = app.shell().open(url, None);
}

/// Download + silently install + restart to the latest release. Triggered by
/// the update dialog's "安装并重启" button.
///
/// The frontend passes the asset metadata it received from `check_for_update`
/// (so we don't re-hit the API). Progress is streamed back via the
/// `update-download-progress` event; on completion this command never returns
/// (the process exits so the installer can replace the exe).
#[tauri::command]
fn download_and_install_update(
    app: tauri::AppHandle,
    asset_url: String,
    asset_name: String,
    asset_size: u64,
    version: String,
) {
    std::thread::spawn(move || {
        // Clear any "skipped" marker for this version now that the user is
        // actively installing it.
        {
            let mut s = settings::load();
            if s.skipped_update_version.as_deref() == Some(version.as_str()) {
                s.skipped_update_version = None;
                let _ = settings::save(&s);
            }
        }
        // Resolve the installed exe path the NSIS installer writes to
        // (installMode: currentUser → %LOCALAPPDATA%\TokenStep\tokenstep.exe).
        // The relaunch helper starts this path after the silent install.
        let installed_exe = installed_exe_path();
        update::run_self_update(&asset_url, &asset_name, asset_size, &version, &installed_exe, |p| {
            let _ = app.emit("update-download-progress", &p);
        });
        // On success run_self_update exits the process and never reaches here.
        // If it returned, the download failed — tell the frontend so it can
        // show a retry button.
        let _ = app.emit("update-download-error", ());
    });
}

/// Record that the user wants to skip a specific version. The next
/// `check_for_update` will report no update until an even newer release appears.
#[tauri::command]
fn skip_update_version(version: String) {
    let mut s = settings::load();
    s.skipped_update_version = Some(version);
    let _ = settings::save(&s);
}

/// Clear any previously-skipped version (a "reset" button on the settings card).
#[tauri::command]
fn reset_skipped_update() {
    let mut s = settings::load();
    s.skipped_update_version = None;
    let _ = settings::save(&s);
}

/// Run a single collect pass on a background thread, then refresh shared state.
fn run_refresh(app: &tauri::AppHandle) {
    let app = app.clone();
    // Don't pile up overlapping collect passes.
    // (The timer interval is minutes, so contention is unlikely, but guard anyway.)
    let app_state: tauri::State<'_, Arc<AppState>> = app.state();
    {
        let mut refreshing = app_state.refreshing.lock();
        if *refreshing {
            return;
        }
        *refreshing = true;
    }
    // Emit a refresh-started event so the UI can show the spinner.
    let _ = app.emit("refresh-started", ());

    std::thread::spawn(move || {
        let snapshot = collector::collect();
        let state: tauri::State<'_, Arc<AppState>> = app.state();
        *state.snapshot.lock() = Some(snapshot.clone());
        *state.last_error.lock() = None;
        *state.refreshing.lock() = false;
        if let Some(window) = app.get_webview_window("dashboard") {
            let _ = window.emit("snapshot-updated", &snapshot);
        }
        let _ = app.emit("refresh-finished", ());
        // Re-render the tray icon with today's progress.
        let (tokens, progress) = today_progress(state.inner());
        render_tray_icon(&app, tokens, progress, false);
    });
}

#[tauri::command]
fn refresh(app: tauri::AppHandle) {
    run_refresh(&app);
}

/// Update the tray icon (dynamic progress ring) + tooltip after a collection.
///
/// Update the tray icon (dynamic progress ring) + tooltip after a collection.
/// Uses the `png` crate for encoding (the hand-written DEFLATE was rejected).
/// Initial startup shows the static logo; this replaces it once data is ready.
fn render_tray_icon(app: &tauri::AppHandle, tokens: i64, progress: f64, refreshing: bool) {
    let Some(tray) = app.tray_by_id("tokenstep-tray") else {
        return;
    };
    let png = render_progress_ring_png(progress, refreshing);
    let _ = tray.set_icon(Some(tauri::image::Image::new(&png, TRAY_SIZE, TRAY_SIZE)));

    let pct = (progress.clamp(0.0, 1.0) * 100.0).round() as i64;
    let lang = settings::load().language;
    let today_lbl = tray_text("tip_today", &lang);
    let status = if refreshing { tray_text("tip_syncing", &lang) } else { "" };
    let _ = tray.set_tooltip(Some(format!(
        "TokenStep — {} {} ({}){}",
        today_lbl,
        human_tokens(tokens),
        pct,
        status
    )));
}

const TRAY_SIZE: u32 = 32;

/// Rasterize a progress-ring tray icon (32x32 RGBA) and encode it as a
/// standards-compliant PNG via the `png` crate. Design mirrors the macOS
/// StatusBarIconRenderer: grey track ring + green arc sweeping clockwise from
/// the top, + a center dot (grey while refreshing).
fn render_progress_ring_png(progress: f64, refreshing: bool) -> Vec<u8> {
    let s = TRAY_SIZE as usize;
    let mut rgba = vec![0u8; s * s * 4]; // fully transparent

    let cx = s as f64 / 2.0;
    let cy = s as f64 / 2.0;
    let radius = s as f64 * 0.32;
    let lw = s as f64 * 0.11;
    let p = progress.clamp(0.0, 1.0);

    let track = [180u8, 180, 180, 230];
    let green = [45u8, 164, 78, 255];
    let dot_col = if refreshing {
        [150u8, 150, 150, 200]
    } else {
        [45u8, 164, 78, 255]
    };

    for y in 0..s {
        for x in 0..s {
            let xf = x as f64 + 0.5;
            let yf = y as f64 + 0.5;
            let dx = xf - cx;
            let dy = yf - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let i = (y * s + x) * 4;

            if (dist - radius).abs() <= lw / 2.0 {
                let angle = (dy.atan2(dx).to_degrees() + 90.0 + 360.0) % 360.0;
                let sweep = p * 360.0;
                let col = if angle <= sweep { green } else { track };
                rgba[i..i + 4].copy_from_slice(&col);
            }
            let dot_r = s as f64 * 0.085;
            if dist <= dot_r {
                rgba[i..i + 4].copy_from_slice(&dot_col);
            }
        }
    }

    // Encode with the standard `png` crate (compliant DEFLATE) — NOT a
    // hand-written encoder, which produced PNGs the image crate rejected.
    let mut out = Vec::with_capacity(4096);
    let mut encoder = png::Encoder::new(&mut out, TRAY_SIZE, TRAY_SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&rgba).expect("png encode");
    writer.finish().expect("png finish");
    out
}

/// Compact Chinese-style token formatting for the tooltip (万 / 亿).
fn human_tokens(tokens: i64) -> String {
    let v = tokens as f64;
    if v >= 100_000_000.0 {
        format!("{:.2}亿", v / 100_000_000.0)
    } else if v >= 10_000.0 {
        format!("{:.1}万", v / 10_000.0)
    } else {
        format!("{}", tokens)
    }
}

// Brand logo, embedded at compile time from icons/128x128.png (used for the
// window icon / about page; the tray uses a dynamic progress ring instead).
#[allow(dead_code)]
const LOGO_PNG: &[u8] = include_bytes!("../icons/128x128.png");

/// Recompute today's tokens/progress from the cached snapshot + settings.
fn today_progress(state: &Arc<AppState>) -> (i64, f64) {
    let snapshot = state.snapshot.lock().clone().unwrap_or_else(UsageSnapshot::empty);
    let settings = settings::load();
    let tz = chrono::FixedOffset::east_opt(8 * 3600).expect("valid offset");
    let today_key = chrono::Utc::now()
        .with_timezone(&tz)
        .format("%Y-%m-%d")
        .to_string();
    let today = snapshot
        .daily
        .iter()
        .rev()
        .find(|d| d.date == today_key)
        .or_else(|| snapshot.daily.last());
    let tokens = today.map(|d| d.total_tokens).unwrap_or(0);
    let goal = settings.daily_goal_tokens.max(1);
    let progress = tokens as f64 / goal as f64;
    (tokens, progress)
}

/// The absolute path the NSIS installer writes the new exe to.
///
/// `installMode: "currentUser"` (tauri.conf.json) installs to
/// `%LOCALAPPDATA%\<productName>\tokenstep.exe`. This is the path the update
/// relaunch helper must start once the silent install finishes.
///
/// Falls back to the currently-running exe's path if LOCALAPPDATA is unset
/// (e.g. running from a portable copy), so the helper still has a target.
fn installed_exe_path() -> std::path::PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let mut p = std::path::PathBuf::from(local);
        p.push("TokenStep");
        p.push("tokenstep.exe");
        if p.exists() {
            return p;
        }
    }
    // Fallback: the running exe (portable / dev). The installer may not write
    // here, but it's the best guess and the user can still relaunch manually.
    std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("tokenstep.exe"))
}

/// Localized text for the tray menu + tooltip. Rust can't read the JS i18n
/// table, so we keep a tiny mirror here for the 4 menu items + app name +
/// tooltip fragments.
/// Keys: "open", "refresh", "check_update", "quit", "app_name",
///       "tip_today" (e.g. "今日"), "tip_syncing" (e.g. "（同步中）").
fn tray_text(key: &str, lang: &str) -> &'static str {
    // zhHant falls back to zhHans text where it matches, differing only where
    // needed (they're nearly identical for these short labels).
    match (lang, key) {
        ("en", "open") => "Open Dashboard",
        ("en", "refresh") => "Refresh Now",
        ("en", "check_update") => "Check for Updates",
        ("en", "quit") => "Quit TokenStep",
        ("en", "app_name") => "TokenStep",
        ("en", "tip_today") => "Today",
        ("en", "tip_syncing") => " (syncing)",
        ("zhHant", "open") => "開啟儀表板",
        ("zhHant", "refresh") => "立即重新整理",
        ("zhHant", "check_update") => "檢查更新",
        ("zhHant", "quit") => "結束 TokenStep",
        ("zhHant", "app_name") => "TokenStep",
        ("zhHant", "tip_today") => "今日",
        ("zhHant", "tip_syncing") => "（同步中）",
        // zhHans (default) + any unrecognized lang
        (_, "open") => "打开仪表盘",
        (_, "refresh") => "立即刷新",
        (_, "check_update") => "检查更新",
        (_, "quit") => "退出 TokenStep",
        (_, "app_name") => "TokenStep",
        (_, "tip_today") => "今日",
        (_, "tip_syncing") => "（同步中）",
        _ => "",
    }
}

/// Rebuild the tray menu labels from the persisted language setting. Called on
/// startup and whenever `set_language` changes the setting.
fn rebuild_tray_menu(app: &tauri::AppHandle) {
    let lang = settings::load().language;
    let set = |id: &str, text: &str| {
        if let Some(item) = app.menu().and_then(|m| m.get(id)) {
            if let Some(mi) = item.as_menuitem() {
                let _ = mi.set_text(text);
            }
        }
    };
    set("open", tray_text("open", &lang));
    set("refresh", tray_text("refresh", &lang));
    set("check-update", tray_text("check_update", &lang));
    set("quit", tray_text("quit", &lang));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(AppState::default()))
        .setup(|app| {
            // Load settings + run an initial collect so the UI has data.
            let state: tauri::State<'_, Arc<AppState>> = app.state();
            let initial_settings = settings::load();
            let lang = initial_settings.language.clone();
            *state.settings.lock() = initial_settings;

            // Build the tray menu in the user's saved language.
            let open = MenuItem::with_id(app, "open", tray_text("open", &lang), true, None::<&str>)?;
            let refresh_item = MenuItem::with_id(app, "refresh", tray_text("refresh", &lang), true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let check_update_item =
                MenuItem::with_id(app, "check-update", tray_text("check_update", &lang), true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", tray_text("quit", &lang), true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&open, &refresh_item, &sep1, &check_update_item, &quit],
            )?;

            // Tray icon: the bundled app icon (proven to display reliably on
            // Windows). Runtime set_icon with a dynamic ring was unreliable, so
            // we keep the static logo and surface progress via the tooltip.
            let icon = app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| tauri::image::Image::new(LOGO_PNG, 128, 128));
            let _tray = TrayIconBuilder::with_id("tokenstep-tray")
                .icon(icon)
                .tooltip("TokenStep")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        show_dashboard(app);
                    }
                    "refresh" => {
                        run_refresh(app);
                    }
                    "check-update" => {
                        check_update_from_menu(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        show_dashboard(app);
                    }
                })
                .build(app)?;

            // Kick off the first collection + render the real tray icon.
            run_refresh(app.handle());

            // Background update check (startup). Mirrors macOS UpdateService:
            // probe GitHub once on launch; on a hit, emit an event the UI
            // listens for. Failures are swallowed inside update::check().
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let result = update::check();
                if result.has_update {
                    let _ = app_handle.emit("update-available", &result);
                    // Reflect the finding in the tray menu label, so users who
                    // never open the dashboard still see the new version.
                    if let Some(item) = app_handle.menu().and_then(|m| m.get("check-update")) {
                        if let Some(mi) = item.as_menuitem() {
                            let _ = mi.set_text(format!(
                                "发现新版本 v{} →",
                                result.latest_version
                            ));
                        }
                    }
                }
            });

            // Background refresh timer.
            let app_handle = app.handle().clone();
            std::thread::spawn(move || loop {
                let interval = settings::load().refresh_interval_seconds;
                let sleep_secs = if interval > 0 { interval } else { 60 };
                std::thread::sleep(std::time::Duration::from_secs(sleep_secs as u64));
                if settings::load().refresh_interval_seconds > 0 {
                    run_refresh(&app_handle);
                    let st: tauri::State<'_, Arc<AppState>> = app_handle.state();
                    let (tokens, progress) = today_progress(st.inner());
                    render_tray_icon(&app_handle, tokens, progress, false);
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Respect the "close to tray" setting: if true, hide the window
            // (app keeps running in the tray); if false, actually close/quit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = settings::load().close_to_tray;
                if close_to_tray {
                    let _ = window.hide();
                    api.prevent_close();
                }
                // else: let the window close normally → app exits (no tray
                // window left). The tray icon itself stays until process exit.
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_settings,
            set_daily_goal,
            set_refresh_interval,
            set_close_to_tray,
            set_autostart,
            reset_settings,
            set_auto_update_enabled,
            set_ask_before_downloading_updates,
            set_require_verified_updates,
            set_theme,
            set_language,
            set_show_codex_quota,
            save_screenshot,
            set_screenshot_dir,
            pick_folder,
            check_for_update,
            open_release_page,
            download_and_install_update,
            skip_update_version,
            reset_skipped_update,
            is_refreshing,
            read_codex_quota,
            read_claude_quota,
            read_token_rank,
            set_show_token_rank,
            set_token_rank_user_id,
            refresh,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Public shim used by the `check` example to run a collection pass without
/// booting the full Tauri app.
pub fn collect_for_check() -> models::UsageSnapshot {
    collector::collect()
}

/// Tray-menu "检查更新" handler. Runs the check off the UI thread, then:
/// - on a hit: updates the menu label and opens the dashboard so the update
///   dialog can present the download/install flow (no longer jumps to a
///   browser — the in-app dialog handles everything);
/// - on no update: flashes the label to "已是最新版" briefly, then reverts.
///
/// Menu label edits require finding the item via the app's menu; the menu was
/// built with `Menu::with_items` and the item carries id "check-update".
fn check_update_from_menu(app: &tauri::AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || {
        let result = update::check();
        if let Some(item) = app_handle.menu().and_then(|m| m.get("check-update")) {
            if let Some(mi) = item.as_menuitem() {
                if result.has_update {
                    let _ = mi.set_text(format!("发现新版本 v{} →", result.latest_version));
                    // Bring the dashboard forward so the user sees the update
                    // dialog instead of a browser tab.
                    show_dashboard(&app_handle);
                    // Notify the dashboard so it pops the update dialog + badge.
                    let _ = app_handle.emit("update-available", &result);
                } else {
                    let _ = mi.set_text("已是最新版");
                    // Revert the label after a moment so the next open isn't stale.
                    let app2 = app_handle.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        if let Some(item) = app2.menu().and_then(|m| m.get("check-update")) {
                            if let Some(mi) = item.as_menuitem() {
                                let _ = mi.set_text("检查更新");
                            }
                        }
                    });
                }
            }
        }
    });
}

/// Show (or create) the dashboard window.
fn show_dashboard(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("dashboard") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = WebviewWindowBuilder::new(
            app,
            "dashboard",
            tauri::WebviewUrl::App("dashboard/index.html".into()),
        )
        .title("TokenStep")
        .inner_size(1080.0, 740.0)
        .min_inner_size(880.0, 600.0)
        .center()
        .build();
    }
}
