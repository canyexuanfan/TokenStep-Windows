//! TokenStep Windows app — Tauri 2 entry point.
//!
//! Provides:
//!   - a system-tray icon showing today's progress ring + token count
//!   - a dashboard window (Today / History / Stats / Privacy)
//!   - Tauri commands the frontend calls to fetch data & update settings
//!   - a background refresh timer driven by the user's refresh interval

mod collector;
mod models;
mod paths;
mod pricing;
mod settings;

use models::{TokenStepSettings, UsageSnapshot};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WebviewWindowBuilder,
};

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

#[tauri::command]
fn set_close_to_tray(enabled: bool) -> Result<TokenStepSettings, String> {
    let mut s = settings::load();
    s.close_to_tray = enabled;
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
/// The ring is rasterized at runtime and encoded with the standard `png` crate
/// (the earlier hand-written DEFLATE produced PNGs that the image decoder
/// silently rejected, leaving the tray blank). With a standards-compliant PNG,
/// `set_icon` works reliably — matching pystray's behavior on Windows.
fn render_tray_icon(app: &tauri::AppHandle, tokens: i64, progress: f64, refreshing: bool) {
    let Some(tray) = app.tray_by_id("tokenstep-tray") else {
        return;
    };
    let png = render_progress_ring_png(progress, refreshing);
    let _ = tray.set_icon(Some(tauri::image::Image::new(&png, TRAY_SIZE, TRAY_SIZE)));

    let pct = (progress.clamp(0.0, 1.0) * 100.0).round() as i64;
    let status = if refreshing { "（同步中）" } else { "" };
    let _ = tray.set_tooltip(Some(format!(
        "TokenStep — 今日 {} ({}){}",
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(Arc::new(AppState::default()))
        .setup(|app| {
            // Load settings + run an initial collect so the UI has data.
            let state: tauri::State<'_, Arc<AppState>> = app.state();
            *state.settings.lock() = settings::load();

            // Build the tray menu.
            let open = MenuItem::with_id(app, "open", "打开仪表盘", true, None::<&str>)?;
            let refresh_item = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 TokenStep", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &refresh_item, &quit])?;

            // Tray icon: use the bundled app icon (most reliable on Windows).
            // The dynamic progress-ring redrawing via set_icon didn't render
            // in the tray, so we show the static logo + a live tooltip instead.
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
            set_theme,
            is_refreshing,
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
