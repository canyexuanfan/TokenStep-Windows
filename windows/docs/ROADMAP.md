# TokenStep for Windows — Roadmap

This Windows port (Tauri 2 + Rust) re-implements the macOS TokenStep's core:
local-only collection of AI token usage from Codex and Claude Code, a daily
"step ring", history, and stats.

The items below were **confirmed but intentionally deferred** from the first
iteration. They are tracked here for follow-up work.

## Confirmed optimizations (next up)

### 1. More agent support
Extend `collector.rs` to read usage from additional local coding agents:
- **Cursor** — `%USERPROFILE%\.cursor\` logs
- **Gemini CLI** — local rollout/state
- **ZCode** — local usage metadata

Each needs its own parser following the Codex/Claude pattern:
scan candidate paths → normalize token fields → emit `UsageRecord`s.

### 2. Desktop notifications / goal alerts
Use `tauri-plugin-notification` (a Win10/11 toast) to surface:
- daily goal reached
- unusually high spend / usage spikes

Wiring: the refresh thread already recomputes `today_progress`; emit a
notification when crossing the goal threshold or when a per-day total exceeds
a configurable multiple of the rolling average.

### 3. Data export (CSV / JSON)
Add Tauri commands `export_daily_csv(path)` / `export_full_json(path)` writing:
- daily rows (date, per-tool tokens, total, cost)
- full snapshot (already serialized as `usage.json`)

Expose buttons in the **Stats** tab; default save location is the user's
Documents folder.

### 4. Optional autostart toggle (off by default)
Per the original decision, autostart stays **disabled**. When this is built:
- Windows equivalent of macOS `launchd` is the registry `Run` key
  (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) or a shortcut in the
  Startup folder (`shell:startup`).
- Add a checkbox in **Settings** that writes/removes the entry.
- Never force-enable on first run (unlike the macOS app).

## Known limitations / performance

- **First-run scan is slow on large histories.** On a machine with ~9.7 GB of
  Codex JSONL the initial collection took ~8 minutes (release). Subsequent runs
  use the per-file cache (`%APPDATA%\TokenStep\cache\collector-cache.json`)
  and drop to tens of seconds. Possible mitigations to evaluate later:
  - limit JSONL scan to the most recent N days (configurable)
  - incremental SQLite-first collection with JSONL top-up
  - mmap + streaming byte-scan before any JSON parse
- **Timezone is fixed to Asia/Shanghai** to match the macOS/Python defaults.
  System-timezone auto-detection is a candidate follow-up.
- **Tray icon** is a 32x32 hand-rendered progress ring (no font rendering of
  the token count inside the icon). The token count is shown in the dashboard
  and the tray tooltip/menu instead.
- **No single-instance guard / no "quit hides to tray" prompt** — closing the
  dashboard window hides it (the tray keeps the app alive).

## Architecture notes

- `windows/src-tauri/src/collector.rs` — the core, a 1:1 port of the Swift
  `UsageCollector` / Python `token_usage_monitor.py`. SQLite is read in-process
  via `rusqlite` (bundled), replacing the macOS `/usr/bin/sqlite3` subprocess.
- `windows/ui/` — static HTML/CSS/JS; no bundler. Talks to Rust via Tauri
  commands (`get_snapshot`, `get_settings`, `set_daily_goal`,
  `set_refresh_interval`, `refresh`) and events (`snapshot-updated`,
  `refresh-started`, `refresh-finished`).
- Output `usage.json` is **format-compatible** with the macOS app.
