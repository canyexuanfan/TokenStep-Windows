# TokenStep for Windows

A Windows port of [TokenStep](../README.md), the menu-bar app that turns AI
token usage into a daily "step ring". This edition is built with **Tauri 2 +
Rust** and lives in the **system tray** (the Windows equivalent of the macOS
menu bar).

It reads usage metadata **locally** from:
- **Codex** — `~/.codex/sessions/**/*.jsonl`, `~/.codex/archived_sessions/*.jsonl`,
  and the `threads` table in `~/.codex/state_5.sqlite` (fallback).
- **Claude Code** — `~/.claude/projects/**/*.jsonl`.

It never uploads code, prompts, or conversation content. See
[`docs/ROADMAP.md`](docs/ROADMAP.md) for the privacy model and planned work.

## Requirements

- Windows 10/11 (x64), with **WebView2 Runtime** (preinstalled on most systems;
  the installer will fetch it if missing).
- Rust (stable, `x86_64-pc-windows-msvc`) + the MSVC build tools (Visual Studio
  2022 Build Tools' "Desktop development with C++" workload provides
  `link.exe` / `cl.exe`).
- The **Tauri CLI** (the build script installs it if absent).

## Run in development

```bat
cd src-tauri
cargo install tauri-cli --version "^2.0"
cargo tauri dev
```

The app starts in the tray. Click the tray icon to open the dashboard, or
right-click for the menu (Open dashboard / Refresh / Quit).

## Build a release installer

```bat
scripts\build-release.bat
```

This produces an NSIS installer under
`src-tauri\target\release\bundle\nsis\TokenStep_0.1.0_x64-setup.exe`.

## Project layout

```
windows/
├── src-tauri/
│   ├── Cargo.toml          tauri, rusqlite, serde, chrono, glob, dirs
│   ├── tauri.conf.json     window / tray / bundle config
│   ├── resources/          pricing.json (bundled alongside the exe)
│   ├── icons/              .ico + .png set (generated from the macOS icon)
│   └── src/
│       ├── main.rs         thin binary bootstrap
│       ├── lib.rs          tray, window, commands, refresh timer
│       ├── collector.rs    core: Codex (SQLite + JSONL) + Claude Code
│       ├── pricing.rs      spend estimation (pricing.json + Swift fallbacks)
│       ├── models.rs       snapshot / settings models (usage.json-compatible)
│       ├── paths.rs        %APPDATA%\TokenStep layout
│       └── settings.rs     settings load / save / normalize
├── ui/
│   ├── shared/app.css      palette + components (port of Components.swift)
│   ├── shared/app.js       formatters + helpers (port of Formatters.swift)
│   └── dashboard/index.html  Today / History / Stats / Privacy / Settings
├── scripts/build-release.bat
└── docs/ROADMAP.md
```

## Data locations

| Path | Purpose |
|------|---------|
| `%APPDATA%\TokenStep\data\usage.json` | generated snapshot |
| `%APPDATA%\TokenStep\config\settings.json` | goal + refresh interval |
| `%APPDATA%\TokenStep\cache\collector-cache.json` | per-file parsed cache |

## Verification

The Rust collector was validated against the original Python collector on a
real machine (~9.7 GB Codex JSONL + 43 Claude Code files): token totals,
active days, per-tool splits, and record counts match. See `docs/ROADMAP.md`
for performance notes on large histories.

## Attribution & License

This Windows edition is a port of the original **TokenStep** for macOS,
authored by **Chaoqiang Huang (黄叔)**. The Windows port (Tauri 2 + Rust,
collector logic, UI) is by **十七°**.

- Licensed under the **MIT License** — see [`LICENSE`](LICENSE).
- All credit for the original concept, design, and macOS implementation
  goes to the original author. This port reuses the original logo artwork
  and `config/pricing.json` and is distributed in accordance with the MIT
  terms (copyright notice retained).
- The `usage.json` output format is intentionally compatible with the
  macOS app's.

