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
        Ok(s) => normalize(migrate_legacy_rank_toggle(s, &data)),
        Err(_) => TokenStepSettings::default(),
    }
}

/// Legacy migration (upstream v0.2.0 decode rule): the retired boolean
/// `show_token_rank` maps onto the new tri-state — `true` → "visible",
/// `false`/absent → "hidden". A saved `agent_work_rank_visibility` always
/// wins; the legacy boolean NEVER silently enables identity reads.
fn migrate_legacy_rank_toggle(s: TokenStepSettings, raw: &str) -> TokenStepSettings {
    let mut s = s;
    if s.agent_work_rank_visibility != "automatic"
        && s.agent_work_rank_visibility != "visible"
        && s.agent_work_rank_visibility != "hidden"
    {
        // The decode default is "hidden", which also covers the fresh-install
        // case; re-check the raw JSON to see if the legacy boolean said true.
        let legacy_true = serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|v| v.get("show_token_rank").and_then(|b| b.as_bool()))
            .unwrap_or(false);
        s.agent_work_rank_visibility =
            if legacy_true { "visible" } else { "hidden" }.to_string();
    }
    s
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
/// - rank visibility restricted to {automatic, visible, hidden}
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
    let valid_visibilities = ["automatic", "visible", "hidden"];
    let visibility = if valid_visibilities.contains(&s.agent_work_rank_visibility.as_str()) {
        s.agent_work_rank_visibility.clone()
    } else {
        "hidden".to_string()
    };
    TokenStepSettings {
        daily_goal_tokens: s.daily_goal_tokens.max(1_000_000),
        refresh_interval_seconds: interval,
        history_days: s.history_days.clamp(7, 365),
        close_to_tray: s.close_to_tray,
        autostart: s.autostart,
        auto_update_enabled: s.auto_update_enabled,
        ask_before_downloading_updates: s.ask_before_downloading_updates,
        require_verified_updates: s.require_verified_updates,
        theme,
        screenshot_dir: s.screenshot_dir,
        language: {
            let valid = ["zhHans", "en", "zhHant"];
            if valid.contains(&s.language.as_str()) { s.language.clone() } else { "zhHans".to_string() }
        },
        skipped_update_version: s.skipped_update_version.filter(|v| !v.trim().is_empty()),
        show_codex_quota: s.show_codex_quota,
        agent_work_rank_visibility: visibility,
        experimental_agent_sources: s
            .experimental_agent_sources
            .filter(|list| !list.is_empty())
            .map(|list| {
                // Keep only known source names (T1 + legacy display ids).
                let known: Vec<String> = list
                    .into_iter()
                    .filter(|id| {
                        crate::agent_sources::ALL_SOURCE_IDS.contains(&id.as_str())
                            || crate::agent_sources::LEGACY_SOURCE_IDS.contains(&id.as_str())
                    })
                    .collect();
                if known.is_empty() {
                    None
                } else {
                    Some(known)
                }
            })
            .flatten(),
        device_sync_enabled: false, // G-S1: no enablement surface until server ships
        merge_today_all_devices: false,
        merge_history_all_devices: false,
        hidden_device_ids: vec![],
        show_experimental_agent_sources: s.show_experimental_agent_sources,
    }
}
