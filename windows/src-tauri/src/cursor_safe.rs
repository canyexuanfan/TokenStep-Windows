//! Cursor credential decryption (v0.1.12): newer Cursor builds encrypt the
//! WorkOS access token with Electron safeStorage (Chromium OSCrypt v10 —
//! AES-256-GCM with a key stored DPAPI-wrapped in `Local State`).
//!
//! Port of the live-verified recipe: read `cursorAuth/accessToken`
//! (`cursor-safe-storage:v1:<base64(v10|nonce|ciphertext|tag)>`), unwrap the
//! OS crypt key from `%APPDATA%\Cursor\Local State` via CryptUnprotectData,
//! then AES-256-GCM decrypt (12-byte nonce, tag appended).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use std::path::PathBuf;

/// `%APPDATA%\Cursor\Local State` — Chromium-side store holding the wrapped
/// OSCrypt key.
fn local_state_path() -> PathBuf {
    let roaming = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(roaming).join("Cursor").join("Local State")
}

/// `Roaming/Antigravity/Local State` — the Antigravity app's OSCrypt store.
fn antigravity_local_state_path() -> PathBuf {
    let roaming = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(roaming)
        .join("Antigravity")
        .join("Local State")
}

fn os_crypt_key_from(local_state: &std::path::Path) -> Option<Vec<u8>> {
    let text = std::fs::read_to_string(local_state).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let wrapped_b64 = json.get("os_crypt")?.get("encrypted_key")?.as_str()?;
    let wrapped = base64::engine::general_purpose::STANDARD
        .decode(wrapped_b64)
        .ok()?;
    if wrapped.starts_with(b"DPAPI") {
        dpapi_unprotect(&wrapped[5..])
    } else {
        None
    }
}

fn dpapi_unprotect(data: &[u8]) -> Option<Vec<u8>> {
    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptUnprotectData(
            pdata_in: *const DataBlob,
            _ppsz_descr: *mut *mut u16,
            _poptional_entropy: *const core::ffi::c_void,
            _reserved: *const core::ffi::c_void,
            _p_prompt: *const core::ffi::c_void,
            dw_flags: u32,
            pdata_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h_mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }
    let blob_in = DataBlob {
        cb_data: data.len() as u32,
        pb_data: data.as_ptr() as *mut u8,
    };
    let mut blob_out = DataBlob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &blob_in,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut blob_out,
        )
    };
    if ok == 0 || blob_out.cb_data == 0 {
        return None;
    }
    let out =
        unsafe { std::slice::from_raw_parts(blob_out.pb_data, blob_out.cb_data as usize) }.to_vec();
    unsafe { LocalFree(blob_out.pb_data as *mut core::ffi::c_void) };
    Some(out)
}

fn os_crypt_key() -> Option<Vec<u8>> {
    os_crypt_key_from(&local_state_path())
}

/// Decrypt `cursor-safe-storage:v1:...` values from Cursor's state.vscdb.
pub fn decrypt_safe_storage(payload: &str) -> Option<String> {
    let marker = "cursor-safe-storage:v1:";
    let raw = base64::engine::general_purpose::STANDARD
        .decode(payload.strip_prefix(marker)?)
        .ok()?;
    if raw.len() < 15 || &raw[..3] != b"v10" {
        return None;
    }
    let key = os_crypt_key()?;
    let aes = Aes256Gcm::new_from_slice(&key).ok()?;
    let nonce = Nonce::from_slice(&raw[3..15]);
    let plaintext = aes.decrypt(nonce, &raw[15..]).ok()?;
    String::from_utf8(plaintext).ok()
}

/// Decrypt an Antigravity-app safeStorage v10 blob (`v10` prefix + 12-byte
/// nonce + ciphertext/tag) with the app key from `Antigravity/Local State`.
/// Used for the cockpit's sealed account credentials (Antigravity quota).
pub fn decrypt_v10_with_antigravity(raw: &[u8]) -> Option<String> {
    if raw.len() < 15 || &raw[..3] != b"v10" {
        return None;
    }
    let key = os_crypt_key_from(&antigravity_local_state_path())?;
    let aes = Aes256Gcm::new_from_slice(&key).ok()?;
    let nonce = Nonce::from_slice(&raw[3..15]);
    let plaintext = aes.decrypt(nonce, &raw[15..]).ok()?;
    String::from_utf8(plaintext).ok()
}
