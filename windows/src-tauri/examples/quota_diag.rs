//! Standalone quota diagnostic: run the codex/claude quota readers
//! end-to-end (including the npm-vendor binary fallback in find_codex) and
//! print the snapshots, for debugging the 订阅额度 card.
//!
//! Run with: cargo run --example quota_diag

use tokenstep_lib as app;

fn main() {
    let diag = app::quota_diag_for_check();
    let cred = app::antigravity_cred_probe();
    println!("ag_cred: {}", serde_json::to_string_pretty(&cred).unwrap_or_default());
    println!("{}", serde_json::to_string_pretty(&diag).unwrap_or_default());
}
