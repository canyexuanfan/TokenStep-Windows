//! Settings persistence — a port of `DataService` (load/save/normalize).

use crate::models::TokenStepSettings;
use crate::paths;
use std::fs;

/// Load settings from disk, falling back to defaults on any error.
pub fn load() -> TokenStepSettings {
    let Ok(data) = fs::read_to_string(paths::settings_json()) else {
        return TokenStepSettings::default();
    };
    match serde_json::from_str::<TokenStepSettings>(&data) {
        Ok(s) => normalize(s),
        Err(_) => TokenStepSettings::default(),
    }
}

/// Persist normalized settings to `config/settings.json`.
pub fn save(settings: &TokenStepSettings) -> std::io::Result<()> {
    let normalized = normalize(settings.clone());
    let path = paths::settings_json();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = serde_json::to_string_pretty(&normalized)?;
    out.push('\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, out)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Apply the same clamps as the Swift `normalize(_:)`:
/// - goal >= 1M
/// - interval restricted to {0, 60, 300, 900} (else 60)
/// - history days in [7, 365]
pub fn normalize(s: TokenStepSettings) -> TokenStepSettings {
    let allowed = [0, 60, 300, 900];
    let interval = if allowed.contains(&s.refresh_interval_seconds) {
        s.refresh_interval_seconds
    } else {
        60
    };
    let valid_themes = ["green", "ocean", "violet", "amber", "graphite"];
    let theme = if valid_themes.contains(&s.theme.as_str()) {
        s.theme.clone()
    } else {
        "green".to_string()
    };
    TokenStepSettings {
        daily_goal_tokens: s.daily_goal_tokens.max(1_000_000),
        refresh_interval_seconds: interval,
        history_days: s.history_days.clamp(7, 365),
        close_to_tray: s.close_to_tray,
        theme,
        screenshot_dir: s.screenshot_dir,
        language: {
            let valid = ["zhHans", "en", "zhHant"];
            if valid.contains(&s.language.as_str()) { s.language.clone() } else { "zhHans".to_string() }
        },
    }
}
