//! Background-refresh energy policy — a port of upstream macOS
//! `EnergyRefreshPolicy.swift` (v0.2.0, commit f2088ab). On macOS the power
//! source comes from IOKit; on Windows we read `GetSystemPowerStatus`.

/// Background collection floor while on AC power.
pub const AC_BACKGROUND_FLOOR_SECS: i64 = 15 * 60;
/// Background collection floor while on battery (or battery-saver).
pub const BATTERY_BACKGROUND_FLOOR_SECS: i64 = 30 * 60;
/// Quota (rate-limit) refresh TTL.
pub const QUOTA_TTL_SECS: i64 = 15 * 60;
/// Leaderboard refresh TTL.
#[allow(dead_code)] // surfaced to the UI refresh cadence later
pub const RANK_TTL_SECS: i64 = 30 * 60;
/// Minimum spacing between automatic (non-forced) retries.
pub const MINIMUM_AUTOMATIC_RETRY_TTL_SECS: i64 = 60;
/// Foreground tick cap: while a window is visible, tick at most this fast.
pub const MAXIMUM_FOREGROUND_TICK_SECS: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
}

/// Windows power state via `GetSystemPowerStatus`:
/// `ACLineStatus`: 0 = offline (battery), 1 = online (AC), 255 = unknown.
/// `BatteryFlag & 8` = battery saver. Unknown treats as AC (less thrash).
pub fn power_source() -> PowerSource {
    #[repr(C)]
    struct SystemPowerStatus {
        ac_line_status: u8,
        battery_flag: u8,
        battery_life_percent: u8,
        reserved: u8,
        battery_life_time: u32,
        battery_full_life_time: u32,
    }
    extern "system" {
        fn GetSystemPowerStatus(lpsps: *mut SystemPowerStatus) -> i32;
    }
    let mut status = SystemPowerStatus {
        ac_line_status: 0,
        battery_flag: 0,
        battery_life_percent: 0,
        reserved: 0,
        battery_life_time: 0,
        battery_full_life_time: 0,
    };
    // SAFETY: local struct out-param, Win32 API.
    let ok = unsafe { GetSystemPowerStatus(&mut status) } != 0;
    if !ok {
        return PowerSource::Ac;
    }
    let on_battery = status.ac_line_status == 0;
    let battery_saver = status.battery_flag & 0x8 != 0;
    if on_battery || battery_saver {
        PowerSource::Battery
    } else {
        PowerSource::Ac
    }
}

/// Effective background interval: the user's requested interval, but never
/// below the power-source floor (upstream `backgroundInterval`).
pub fn background_interval(requested_secs: i64, source: PowerSource) -> i64 {
    let floor = match source {
        PowerSource::Ac => AC_BACKGROUND_FLOOR_SECS,
        PowerSource::Battery => BATTERY_BACKGROUND_FLOOR_SECS,
    };
    requested_secs.max(floor)
}

/// Whether a foreground surface should trigger a real collection now
/// (upstream `shouldRefreshForForeground`): refresh only when the last
/// snapshot is older than the requested interval.
#[allow(dead_code)]
pub fn should_refresh_for_foreground(
    last_generated_epoch: Option<i64>,
    last_observed_epoch: Option<i64>,
    requested_secs: i64,
    now_epoch: i64,
) -> bool {
    let anchor = last_generated_epoch.max(last_observed_epoch);
    match anchor {
        None => true,
        Some(t) => now_epoch - t >= requested_secs,
    }
}

/// TTL used to short-circuit "is this channel fresh?" checks.
#[allow(dead_code)]
pub fn is_fresh(last_attempt_epoch: Option<i64>, ttl_secs: i64, now_epoch: i64) -> bool {
    match last_attempt_epoch {
        None => false,
        Some(t) => now_epoch - t < ttl_secs,
    }
}

/// Automatic (non-forced) retry throttle (upstream `automaticRetryTTL`).
pub fn automatic_retry_ttl(requested_secs: i64) -> i64 {
    requested_secs.max(MINIMUM_AUTOMATIC_RETRY_TTL_SECS)
}

/// Foreground tick interval (upstream `foregroundTickInterval`).
pub fn foreground_tick_interval(requested_secs: i64) -> i64 {
    requested_secs.min(MAXIMUM_FOREGROUND_TICK_SECS)
}
