// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Single-instance guard (port of macOS SingleInstanceGuard.swift, commit 9d6bfcc).
//
// DISABLED for now: the raw-FFI mutex approach caused the app to silently
// exit(0) on some launches (GetLastError semantics + wide-string name issues),
// making the app appear "missing" from the tray. Re-enable once rewritten with
// a robust, well-tested approach (e.g. a named pipe or the `windows` crate's
// typed CreateMutex). The macOS version's value (avoiding duplicate menu-bar
// instances) is lower on Windows where the tray naturally dedupes.
#[cfg(windows)]
fn ensure_single_instance() {
    // No-op for now.
}

#[cfg(not(windows))]
fn ensure_single_instance() {}

fn main() {
    ensure_single_instance();
    tokenstep_lib::run()
}
