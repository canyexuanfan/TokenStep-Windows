# 更新日志 | Changelog

本文件记录 TokenStep 的版本变更。Windows 移植版的版本号独立于原 macOS 版。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，遵循 [SemVer](https://semver.org/lang/zh-CN/)。

[English](#english) | 中文

## [未发布] / Unreleased

（开发中。参见 [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md)。）

## [0.1.0] - Windows 移植首发

TokenStep Windows 版的首个公开发布版本。基于原 macOS 版（TokenStepSwift）移植至 **Tauri 2 + Rust**，驻留在 Windows 系统托盘。

### 新增

- Windows 系统托盘应用，点击打开仪表盘（今日 / 历史 / 统计 / 隐私 / 设置），右键打开菜单（打开仪表盘 / 刷新 / 退出）。
- **Codex** 用量采集：优先读取 JSONL rollout（逐轮 token 计数），无数据时回退 `state_5.sqlite` 的 `threads` 表。
- **Claude Code** 用量采集：读取 `~/.claude/projects/**/*.jsonl` 的 `usage` 元数据。
- 本地「消耗金额」估算，基于内置定价表（`resources/pricing.json`，与 macOS 版兼容）。
- 按天 / 按工具 / 按模型的聚合统计，以及活跃天数。
- 文件级缓存（按 size + mtime），避免重复解析大型 JSONL。
- 数据快照格式 `usage.json` 与 macOS 版保持兼容。
- NSIS 安装包构建脚本（`scripts/build-release.bat`）与自签名脚本（`scripts/sign.bat`）。
- 时区固定为 Asia/Shanghai。

### 已知限制

参见 [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md)：

- 首次扫描约 9.7 GB 的 Codex JSONL 约需数分钟（之后走缓存）。
- 托盘图标为手工渲染的 32×32 圆环，图标上不显示数字。
- 安装包使用自签名证书，首次运行会触发 SmartScreen 警告。

### 致谢

基于原 macOS **TokenStep**（作者 Chaoqiang Huang / 黄叔）移植。

[未发布]: https://github.com/canyexuanfan/TokenStep-Windows/compare/v0.1.0-windows...HEAD

---

## English

This file tracks TokenStep releases. The Windows port is versioned independently of the original macOS app.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), adheres to [SemVer](https://semver.org/).

## [Unreleased]

(In development. See [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md).)

## [0.1.0] - Initial Windows port

First public release of TokenStep for Windows. A port of the original macOS app (TokenStepSwift) to **Tauri 2 + Rust**, living in the Windows system tray.

### Added

- Windows system-tray app; click the tray icon to open the dashboard (Today / History / Stats / Privacy / Settings), right-click for the menu (Open / Refresh / Quit).
- **Codex** usage collection: reads JSONL rollouts (per-turn token counts) first, falling back to the `threads` table in `state_5.sqlite`.
- **Claude Code** usage collection: reads `usage` metadata from `~/.claude/projects/**/*.jsonl`.
- Local "cost" estimate based on a bundled price table (`resources/pricing.json`, compatible with the macOS version).
- Aggregations by day / tool / model, plus active-days count.
- File-level caching (by size + mtime) to avoid re-parsing large JSONL files.
- `usage.json` snapshot format kept compatible with the macOS app.
- NSIS installer build script (`scripts/build-release.bat`) and self-signing script (`scripts/sign.bat`).
- Timezone hardcoded to Asia/Shanghai.

### Known limitations

See [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md):

- First scan of ~9.7 GB of Codex JSONL takes a few minutes (cached afterward).
- Tray icon is a hand-rendered 32×32 ring with no count overlay.
- Installer is self-signed; first run triggers a SmartScreen warning.

### Acknowledgements

Ported from the original macOS **TokenStep** by Chaoqiang Huang.

[Unreleased]: https://github.com/canyexuanfan/TokenStep-Windows/compare/v0.1.0-windows...HEAD
