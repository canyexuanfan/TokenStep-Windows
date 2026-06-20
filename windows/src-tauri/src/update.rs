//! Update check — a lightweight "is there a newer release on GitHub?" probe,
//! plus the download / silent-install / restart flow.
//!
//! This mirrors the spirit of the macOS `UpdateService`:
//!   1. hit the GitHub Releases "latest" endpoint, compare `tag_name` to the
//!      running version, and surface the result;
//!   2. on a hit, download the NSIS installer to a temp file with progress;
//!   3. run it with `/S` (silent, `installMode: currentUser` so no UAC), then
//!      exit the current process so the installer can overwrite it and relaunch.
//!
//! We deliberately avoid `tauri-plugin-updater`: it requires a pubkey/signed
//! manifest system that conflicts with our self-signed exe. The self-hosted
//! download + NSIS path is simpler and fully controllable.
//!
//! Network is best-effort: any failure (offline, rate-limited, bad JSON) just
//! reports "no update", so a flaky connection never blocks the app.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::Duration;

/// Where the Windows port lives on GitHub. Used for both the API URL and the
/// Releases page a user is sent to.
pub const REPO_OWNER: &str = "canyexuanfan";
pub const REPO_NAME: &str = "TokenStep-Windows";

/// Result surfaced to the frontend. All fields are sent over the `update-available`
/// event (or returned from the `check_for_update` command).
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    /// True only when a strictly-newer release was found.
    pub has_update: bool,
    /// The running app's version (e.g. "0.1.0"), read from Cargo at compile time.
    pub current_version: String,
    /// The latest release's version tag (e.g. "0.2.0"), with any `v` prefix stripped.
    pub latest_version: String,
    /// URL of the latest release's page (opened in the browser on click).
    pub release_url: String,
    /// Optional release notes (the release `body`), shown in the settings card.
    pub release_notes: Option<String>,
    /// Direct download URL of the preferred NSIS installer asset, when present.
    /// Null for releases that ship only a portable exe (can't silent-install).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_url: Option<String>,
    /// Filename of the installer asset (e.g. `TokenStep_0.1.2_x64-setup.exe`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_name: Option<String>,
    /// Size of the installer asset in bytes, for the progress bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_size: Option<u64>,
}

impl Default for UpdateCheck {
    fn default() -> Self {
        UpdateCheck {
            has_update: false,
            current_version: current_version().to_string(),
            latest_version: current_version().to_string(),
            release_url: releases_page_url(),
            release_notes: None,
            asset_url: None,
            asset_name: None,
            asset_size: None,
        }
    }
}

/// The running version, sourced from Cargo.toml (compile-time constant).
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The human-readable Releases page (not the API endpoint).
pub fn releases_page_url() -> String {
    format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest")
}

/// The GitHub API endpoint for the latest (non-prerelease, non-draft) release.
fn latest_release_api_url() -> String {
    format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest")
}

/// Shape of the GitHub releases/latest response. We only pull the few fields we
/// need; serde ignores the rest.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

/// One downloadable asset attached to a release.
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    /// Browser-facing download URL (follows redirects to the CDN).
    browser_download_url: String,
    size: u64,
}

/// Pick the best asset for silent install. We prefer the NSIS installer
/// (`*_x64-setup.exe`) because it supports the `/S` silent flag and installs to
/// a fixed path so the old exe can be overwritten. The portable
/// `TokenStep_v*.exe` is *not* auto-installable (no fixed path, can't replace
/// the running image), so we leave it out — those releases can't self-update.
fn pick_install_asset<'a>(assets: &'a [GitHubAsset]) -> Option<&'a GitHubAsset> {
    let mut best: Option<&GitHubAsset> = None;
    for a in assets {
        let name = a.name.to_ascii_lowercase();
        if !name.ends_with(".exe") {
            continue;
        }
        // NSIS installer: matches TokenStep_<ver>_x64-setup.exe
        let is_setup = name.contains("setup") || name.contains("-setup");
        // Skip the portable versioned exe (TokenStep_v*.exe) — can't self-install.
        let is_portable = name.starts_with("tokenstep_v");
        if is_setup && !is_portable {
            // Prefer the one with "x64-setup" if multiple exist.
            if best.is_none() || name.contains("x64-setup") {
                best = Some(a);
            }
        }
    }
    best
}

/// Check GitHub for a newer release. Never panics / never errors out to the
/// caller — on any failure it returns an `UpdateCheck` with `has_update: false`.
///
/// Honors the persisted `skipped_update_version` setting: if the user dismissed
/// this exact version, we report "no update" so they aren't nagged.
pub fn check() -> UpdateCheck {
    let current = current_version();
    let client = match reqwest::blocking::Client::builder()
        .user_agent(format!("TokenStep/{current}"))
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(_) => return UpdateCheck::default(),
    };

    let resp = match client
        .get(latest_release_api_url())
        .header("Accept", "application/vnd.github+json")
        .send()
    {
        Ok(r) => r,
        Err(_) => return UpdateCheck::default(),
    };
    if !resp.status().is_success() {
        return UpdateCheck::default();
    }
    let release: GitHubRelease = match resp.json() {
        Ok(r) => r,
        Err(_) => return UpdateCheck::default(),
    };

    // Skip draft/prerelease releases — the /latest endpoint already excludes
    // these, but defend in depth in case the endpoint changes.
    if release.draft.unwrap_or(false) || release.prerelease.unwrap_or(false) {
        return UpdateCheck::default();
    }

    let latest = strip_v_prefix(&release.tag_name);
    let mut has_update = is_newer(&latest, current);

    // Honor "skip this version": if the user dismissed this exact version,
    // don't nag them again (until an even newer release appears).
    if has_update {
        if let Some(skipped) = crate::settings::load().skipped_update_version {
            if strip_v_prefix(&skipped) == latest {
                has_update = false;
            }
        }
    }

    let (asset_url, asset_name, asset_size) = match pick_install_asset(&release.assets) {
        Some(a) => (Some(a.browser_download_url.clone()), Some(a.name.clone()), Some(a.size)),
        None => (None, None, None),
    };

    UpdateCheck {
        has_update,
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        release_url: release.html_url,
        release_notes: release.body,
        asset_url,
        asset_name,
        asset_size,
    }
}

/// Strip a leading `v`/`V` so "v0.2.0" and "0.2.0" compare equal.
fn strip_v_prefix(tag: &str) -> String {
    tag.trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .to_string()
}

/// Strict "is `latest` newer than `current`" by numeric segment comparison.
/// Non-numeric segments (e.g. a "beta" suffix) degrade to 0, mirroring the
/// macOS Version struct's behavior. Missing segments on either side are 0.
fn is_newer(latest: &str, current: &str) -> bool {
    cmp_segments(latest, current) == std::cmp::Ordering::Greater
}

fn cmp_segments(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = a.split('.').map(parse_seg).collect::<Vec<_>>();
    let pb = b.split('.').map(parse_seg).collect::<Vec<_>>();
    let len = pa.len().max(pb.len());
    for i in 0..len {
        let va = pa.get(i).copied().unwrap_or(0);
        let vb = pb.get(i).copied().unwrap_or(0);
        match va.cmp(&vb) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

/// Parse one version segment as a leading integer; "1beta" → 1, "x" → 0.
fn parse_seg(s: &str) -> u64 {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

// ──────────────────────────────────────────────────────────────────────────
// Download + silent install + restart
// ──────────────────────────────────────────────────────────────────────────

/// Payload emitted as the `update-download-progress` event while downloading.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    /// 0.0–100.0
    pub percent: f64,
}

/// Where we stash the downloaded installer: `%TEMP%\TokenStep_update_<ver>.exe`.
/// Reuses the same path across runs so partial downloads overwrite cleanly.
fn installer_temp_path(version: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("TokenStep_update_{version}.exe"));
    p
}

/// Download the installer asset to a temp file, emitting progress events.
/// On success returns the temp file path; on failure returns None.
///
/// `asset_name` is currently informational (reserved for a future SHA-256
/// verification hook) but kept in the signature so the caller's contract is
/// stable.
///
/// Runs on a background thread (caller is responsible for spawning), so the
/// blocking reads here never freeze the UI.
pub fn download<F>(asset_url: &str, _asset_name: &str, expected_size: u64, version: &str, emit: F)
    -> Option<std::path::PathBuf>
where
    F: Fn(DownloadProgress),
{
    let client = match reqwest::blocking::Client::builder()
        .user_agent(format!("TokenStep/{}", current_version()))
        // Generous timeout: the installer is a few MB and may be on a slow link.
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return None;
        }
    };

    let resp = match client.get(asset_url).send() {
        Ok(r) => r,
        Err(_) => return None,
    };
    if !resp.status().is_success() {
        return None;
    }

    // Prefer the server's Content-Length when available; fall back to the
    // asset_size we parsed from the API.
    let total = resp.content_length().unwrap_or(expected_size);

    let dest = installer_temp_path(version);
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut file = match std::fs::File::create(&dest) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let mut reader = resp;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,            // EOF
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                let _ = std::fs::remove_file(&dest);
                return None;
            }
        };
        if file.write_all(&buf[..n]).is_err() {
            let _ = std::fs::remove_file(&dest);
            return None;
        }
        downloaded += n as u64;

        // Throttle progress events to ~1 per 256KB so we don't flood the IPC.
        if downloaded.saturating_sub(last_emit) >= 256 * 1024 || downloaded == total {
            last_emit = downloaded;
            let percent = if total > 0 {
                (downloaded as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            emit(DownloadProgress { downloaded, total, percent });
        }
    }
    // Flush + sync so the bytes are on disk before we run it.
    let _ = file.flush();
    let _ = file.sync_all();
    drop(file);

    // Integrity check: actual size must match expected (truncated download?).
    if expected_size > 0 {
        if let Ok(meta) = std::fs::metadata(&dest) {
            if meta.len() != expected_size {
                let _ = std::fs::remove_file(&dest);
                return None;
            }
        }
    }

    Some(dest)
}

/// Run the downloaded NSIS installer silently, then relaunch the app.
///
/// The reliable Windows self-update pattern (used by VSCode/Electron updaters):
/// a small `.bat` helper is written to `%TEMP%`, detached, then this process
/// exits. The bat waits for us to die, runs the NSIS installer with `/S`
/// (silent, `installMode: currentUser` so no UAC), then starts the freshly
/// installed exe and self-deletes.
///
/// Why a bat and not `MUI_FINISHPAGE_RUN`? Because `/S` skips ALL NSIS UI
/// pages including the finish page, so any finish-page relaunch hook is
/// ignored. The bat is the only mechanism that works in silent mode.
///
/// `installed_exe` is the absolute path the installer will write the new exe
/// to (the currentUser install location). We relaunch exactly that path.
///
/// This function does not return: it terminates the process.
pub fn install_and_restart(installer: &std::path::Path, installed_exe: &std::path::Path) -> ! {
    let pid = std::process::id();
    let installer = escape_bat_path(&installer.to_string_lossy());
    let exe = escape_bat_path(&installed_exe.to_string_lossy());
    let our_pid = pid.to_string();

    // The helper bat: wait for our PID to vanish → run installer → relaunch →
    // self-delete. `start ""` launches the exe detached so the bat can exit.
    let bat = format!(
        "@echo off\r\n\
         setlocal\r\n\
         :: Wait for the old TokenStep process (PID {pid}) to exit.\r\n\
         :wait\r\n\
         tasklist /fi \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul\r\n\
         if not errorlevel 1 (\r\n\
             timeout /t 1 /nobreak >nul\r\n\
             goto wait\r\n\
         )\r\n\
         :: Run the NSIS installer silently. /S = no UI. currentUser = no UAC.\r\n\
         \"{installer}\" /S\r\n\
         :: Give the installer a moment to release file handles.\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         :: Relaunch the freshly installed TokenStep, detached.\r\n\
         if exist \"{exe}\" (\r\n\
             start \"\" \"{exe}\"\r\n\
         )\r\n\
         :: Self-delete this helper bat.\r\n\
         (goto) 2>nul | del \"%~f0\"\r\n",
        pid = our_pid,
        installer = installer,
        exe = exe,
    );

    // Write the helper to %TEMP%\TokenStep_update_helper.bat.
    let mut helper = std::env::temp_dir();
    helper.push("TokenStep_update_helper.bat");
    if let Ok(mut f) = std::fs::File::create(&helper) {
        let _ = f.write_all(bat.as_bytes());
        let _ = f.flush();
        drop(f);

        // Detach the helper via cmd /c so it survives our exit.
        // CREATE_NO_WINDOW keeps it from flashing a console.
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "/B", ""])
            .arg(&helper)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        // Brief grace period so the helper is running before we exit.
        std::thread::sleep(Duration::from_millis(500));
    }

    // Exit — the helper takes over from here.
    std::process::exit(0);
}

/// Escape a Windows path for safe embedding inside a .bat double-quoted arg.
/// We only need to neutralize `"` and `%`; backslashes are fine in paths.
fn escape_bat_path(s: &str) -> String {
    s.replace('"', "").replace('%', "%%")
}

/// Entry point invoked from a background thread in lib.rs: orchestrates the
/// download → install → exit sequence, emitting progress along the way.
///
/// `installed_exe` is the absolute path the NSIS installer writes the new exe
/// to (the currentUser install location); the relaunch helper starts that path
/// after the installer finishes.
///
/// Returns `false` if the download failed (so the caller can emit an error
/// event and let the user retry). Returns `true` only by never returning at
/// all — on success it runs the installer and exits the process.
pub fn run_self_update<F>(asset_url: &str, asset_name: &str, asset_size: u64, version: &str, installed_exe: &std::path::Path, emit: F) -> bool
where
    F: Fn(DownloadProgress),
{
    match download(asset_url, asset_name, asset_size, version, emit) {
        Some(path) => install_and_restart(&path, installed_exe), // never returns
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        // v-prefix ignored
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.2.0", "V0.1.0"));
        // uneven lengths
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1", "0.1.1"));
        // non-numeric suffix degrades to 0
        assert!(is_newer("0.2.0-beta", "0.1.0"));
    }

    #[test]
    fn windows_tag_suffix_is_compatible() {
        // Our release tags carry a "-windows" suffix (e.g. v0.1.0-windows) to
        // stay distinct from upstream macOS tags. The suffix must NOT make the
        // tag compare as newer than the running build of the same version:
        // v0.1.0-windows vs running 0.1.0 should be "equal" (no update), while
        // v0.2.0-windows vs 0.1.0 should still report an update.
        assert!(!is_newer("0.1.0-windows", "0.1.0"));
        assert!(is_newer("0.2.0-windows", "0.1.0"));
        assert!(is_newer("0.1.1-windows", "0.1.0"));
    }

    #[test]
    fn strip_prefix() {
        assert_eq!(super::strip_v_prefix("v0.2.0"), "0.2.0");
        assert_eq!(super::strip_v_prefix("V0.2.0"), "0.2.0");
        assert_eq!(super::strip_v_prefix("0.2.0"), "0.2.0");
    }
}
