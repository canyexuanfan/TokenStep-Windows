//! Shared SQLite read-only open helper with the WAL fallback ladder.
//!
//! Port of upstream v0.2.26 SQLiteReadonly.swift (d270209): a plain read-only
//! connection cannot open a WAL database once the owning app has exited and
//! removed its `-shm` sidecar — SQLite needs to create that file and refuses
//! on a read-only handle. Callers therefore fall back to a writable handle
//! restricted with `PRAGMA query_only`, and only then to `immutable`, which
//! skips the WAL entirely and can miss live rows.
//!
//! Consumers: Cursor's `state.vscdb` (cursor_usage.rs) and WorkBuddy's
//! `workbuddy.db` (quota.rs) — both are owned by live Electron-style apps.

use rusqlite::Connection;
use std::path::Path;

enum OpenAttempt {
    ReadOnly,
    QueryOnly,
    Immutable,
}

const ATTEMPTS: [OpenAttempt; 3] = [OpenAttempt::ReadOnly, OpenAttempt::QueryOnly, OpenAttempt::Immutable];

fn uri_for(path: &Path, mode: &str) -> String {
    format!("file:{}?mode={}", path.display().to_string().replace('\\', "/"), mode)
}

/// Open `path` read-only with the fallback ladder. Ordered by fidelity:
/// `queryOnly` must precede `immutable` — a live WAL database rejects
/// `immutable` (its reads miss unmerged rows) while a cleanly closed WAL
/// database rejects plain `readOnly`.
pub fn open_readonly(path: &Path) -> Option<Connection> {
    for attempt in ATTEMPTS {
        let conn = match attempt {
            OpenAttempt::ReadOnly => Connection::open_with_flags(
                uri_for(path, "ro"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok(),
            OpenAttempt::QueryOnly => {
                // A writable handle lets SQLite create the -shm sidecar;
                // query_only keeps the connection from modifying anything.
                Connection::open(path)
                    .ok()
                    .and_then(|conn| {
                        conn.pragma_update(None, "query_only", "ON").ok()?;
                        Some(conn)
                    })
            }
            OpenAttempt::Immutable => Connection::open_with_flags(
                uri_for(path, "immutable"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok(),
        };
        if conn.is_some() {
            return conn;
        }
    }
    None
}
