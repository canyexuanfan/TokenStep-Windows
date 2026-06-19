# TokenStep

> A Windows system-tray app that tracks local AI coding agent token usage like daily steps.
>
> English | **[中文](README.md)**

A local-first tool that turns AI token usage into a daily "step ring". This repo is the **Windows port** of the original macOS **TokenStep** ([by Chaoqiang Huang](https://github.com/Backtthefuture)), built with **Tauri 2 + Rust** and living in the Windows system tray.

It reads token usage from Codex / Claude Code **locally** and shows today's progress like an activity ring — think of it as:

> A "steps app" for the AI-agent era.

> 🍎 On macOS? Get the native menu-bar version from the [original repo](https://github.com/Backtthefuture/TokenStep). Its Swift source is also kept in this repo under [`TokenStepSwift/`](TokenStepSwift).

## What it does

TokenStep only reads **usage metadata** from your machine. It **never uploads your code, prompts, or conversation content**.

The default daily goal is **100 million tokens**. It shows:

- How many tokens you've used today, and what % of the goal that is
- A trend over recent days
- History (by day / by tool / by model)
- A rough "cost" estimate (computed locally — not a real bill)

## Data sources

| Tool | Source (Windows) |
|------|------------------|
| **Codex** | `~/.codex/sessions/**/*.jsonl`, `~/.codex/archived_sessions/*.jsonl` (primary); the `threads` table in `~/.codex/state_5.sqlite` (fallback) |
| **Claude Code** | `~/.claude/projects/**/*.jsonl` |

Counting rule: if the log provides `total_tokens` directly, use it; otherwise sum input / output / cache_creation / cache_read / reasoning. Note that **cache-read tokens are also counted**, so totals may read higher than some tools report. See [`windows/src-tauri/src/collector.rs`](windows/src-tauri/src/collector.rs).

## Install

### Windows (this project)

1. Go to the [Releases page](https://github.com/canyexuanfan/TokenStep-Windows/releases) and download the latest `TokenStep_x.x.x_x64-setup.exe`.
2. Run the installer (requires the WebView2 Runtime — preinstalled on most Win10/11 systems; auto-fetched if missing).
3. TokenStep appears in the system tray. Click the tray icon to open the dashboard; right-click for the menu.

> ⚠️ The installer is signed with a **self-signed certificate**, so Windows SmartScreen may warn about an "Unknown publisher" on first run. This is expected — click "More info → Run anyway". See [`windows/docs/SIGNING.md`](windows/docs/SIGNING.md).

### macOS (original)

Download the signed & notarized DMG from the [original repo's releases](https://github.com/Backtthefuture/TokenStep/releases/latest).

## Features

- Tray icon shows today's token count and a progress ring.
- Click the tray icon to open the dashboard: Today / History / Stats / Privacy / Settings.
- Configurable daily goal (default 100M tokens/day).
- Auto-refresh (default 1 min).
- Supports Codex and Claude Code.
- **Local-first**: all data stays under `%APPDATA%\TokenStep\`; nothing is uploaded.

## Privacy

- Reads only local usage metadata (date, model, client name, token counts).
- **Uploads nothing by default** — never reads or uploads code, prompts, or conversation text.
- The "cost" figure is a rough local estimate based on a bundled price table, not a real bill.

Full notes in [`docs/PRIVACY.md`](docs/PRIVACY.md) (written for the macOS version; the Windows port follows the same model).

## Build from source (Windows)

**Requirements:** Windows 10/11 (x64), Rust (stable, `x86_64-pc-windows-msvc`) + MSVC build tools, the Tauri CLI.

```bat
cd windows\src-tauri
cargo install tauri-cli --version "^2.0"
cargo tauri dev
```

Build a release installer:

```bat
windows\scripts\build-release.bat
```

Output: `windows\src-tauri\target\release\bundle\nsis\TokenStep_x.x.x_x64-setup.exe`.

For full build, signing, and data-directory details see **[`windows/README.md`](windows/README.md)**.

## Build from source (macOS, original)

```bash
./script/build_and_run.sh --verify
```

See the [original instructions](https://github.com/Backtthefuture/TokenStep) and [`docs/INSTALL.md`](docs/INSTALL.md).

## Contributing

Issues and PRs are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) first. If you find a security vulnerability, follow [SECURITY.md](SECURITY.md) and report it privately — **do not** open a public issue.

## Roadmap

The Windows port's TODOs and known limitations (more agent support, desktop notifications, data export, autostart, etc.) are in [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md).

## License

MIT License. This repo contains both the original macOS implementation (`TokenStepSwift/`) and this Windows port (`windows/`). See [LICENSE](LICENSE).

## Acknowledgements

- The original **TokenStep** (macOS) was created by [**Chaoqiang Huang**](https://github.com/Backtthefuture). This Windows port reuses its design, pricing table, and `usage.json` data format.
- The Windows port (Tauri 2 + Rust) is maintained by [**十七°**](https://github.com/canyexuanfan).
