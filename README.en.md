# TokenStep for Windows

**Track your daily AI token usage the way fitness apps track your steps.**

TokenStep is a Windows desktop app that tracks how many tokens you burn in
AI coding agents (Codex, Claude Code, ZCode, MiniMax, DSH, Antigravity and
more) and turns that into a daily goal ring - 100M tokens per lap by
default.

It is a Windows port of the macOS TokenStep by huangshu (Backtthefuture).
MIT licensed.

## Download

Grab the latest installer from Releases. A portable single-exe build is
also attached to every release.

The installer is self-signed; SmartScreen may show "unknown publisher" -
choose More info -> Run anyway.

## What it does

- System-tray resident, tracks today's tokens with a live progress ring
- 16 usage sources across 9 dedicated collectors and 7 generic ones - see
  the Chinese README for the full per-source table
- 8 subscription quota cards (Codex, Claude Code, Cursor, GLM, Kimi, Grok,
  WorkBuddy, Antigravity)
- Share cards (daily scorecard, rhythm, today overview), 30-day trends,
  per-client and per-model breakdowns
- Three languages (zh-Hans / English / zh-Hant), auto-update
- Local-only: usage metadata never leaves this machine

## Build

Windows 10/11 + Rust stable (msvc) + Node.js (optional, for jsdom tests):

```
cd windows/src-tauri
cargo tauri build
```

## License

MIT. The macOS original is by huangshu (Backtthefuture).
