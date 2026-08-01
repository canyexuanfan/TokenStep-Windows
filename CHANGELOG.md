# 更新日志 | Changelog

本文件记录 TokenStep 的版本变更。Windows 移植版的版本号独立于原 macOS 版。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，遵循 [SemVer](https://semver.org/lang/zh-CN/)。

[English](#english) | 中文

## [未发布] / Unreleased

（开发中。参见 [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md)。）

## [0.1.5] - Agent 工作强度 + 计量校准 rev8 + 实验来源

移植上游 macOS v0.1.44/v0.1.45 的全部新功能，并修复移植引入的启动白屏问题。

### 新增

- **Agent 工作强度卡片**（今日页）：4 个指标格（Agent Token / 有记录小时 / 近 7 日均 / 缓存命中率）+ 24 小时堆叠柱状图（按来源着色）。
- **Token 计量校准 rev8**：`SourceInfo` 新增 `accounting_revision` 字段；Codex 来源标记为 rev8（对齐 macOS），缓存版本 4 → 5；启动时检测旧口径并显示校准通知横幅。
- **实验 Agent 来源采集器**（设置页开关，默认关闭）：读取 ZCode（`~/.zcode/cli/db/db.sqlite` 的 `model_usage` 表）、Hermes（`~/.hermes/state.db` 的 `sessions` 表）、WorkBuddy（`~/.workbuddy` 目录探测）。只读 usage 字段，不读对话正文。
- 新增 `set_experimental_agent_sources` / `get_recalibration_notice` / `dismiss_recalibration_notice` 三个 Tauri 命令。
- `tokenToolColor` 扩展 ZCode（蓝色）与 CC Switch 变体配色；`orderedToolEntries` 偏好列表扩展。

### 修复

- **修复启动白屏**：`agentWorkCardHTML()` 内 `var todayKey = todayKey()` 触发变量提升，遮蔽了全局函数 `todayKey()`，导致 `render()` 抛 `TypeError: todayKey is not a function`，整页白屏。改为内联调用。

### 国际化

- 补齐 Agent 工作强度卡片的 en / zhHant 翻译（`Agent Token`、`近 7 日均`、`昨日来源` 等 3 个此前漏入翻译表的 key）；`今日来源` 英文用词对齐上游（`Today by source` → `Today Sources`）。

### 已知限制

- ZCode / Hermes / WorkBuddy 默认不采集，需在「设置 → 实验 Agent 来源」手动开启（与 macOS 上游一致）。
- 安装包仍使用自签名证书，首次运行会触发 SmartScreen 警告。

### 致谢

基于原 macOS **TokenStep**（作者 Chaoqiang Huang / 黄叔）移植，本次同步上游 v0.1.44/v0.1.45 源码镜像。

## [0.1.4] - 节奏分享卡对齐 macOS

### 新增

- **分享卡改用 Canvas 自绘**：截图导出走 `renderShareDailyCard` / `renderTodayOverview` / `renderRhythmCard` 的 Canvas 渲染管线，与 HTML 显示分离，确保导出图与屏幕一致。
- 节奏分享卡对齐 macOS 原生样式：圆角裁剪、峰值胶囊、高斯发光、footer 署名注明 tokenstep.app 是原作者官网。

### 修复

- 最近 30 天柱状图对齐 macOS `StackedActivityBarsView`：底部对齐（`column-reverse` + 纯比例百分比）、堆叠顺序与导出图统一（`.slice().reverse()`）、目标参考线 CSS 变量名修正（`--mutedFaint` → `--muted-faint`）、空数据占位用 `--track` 色。
- 活动墙默认滚动到最右（显示今天），恢复被误删的 53 周历史宽度。
- Win 端 Opus 定价同步上游 v0.1.43（15/75/18.75/1.5 → 5/25/6.25/0.5）。

### 致谢

同步上游 TokenStepSwift v0.1.43 源码镜像（Opus 定价 + SIGPIPE + Codex 配额健壮性）。

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

## [0.1.5] - Agent Work Intensity + Calibration rev8 + Experimental Sources

Ports all new features from upstream macOS v0.1.44/v0.1.45, and fixes a startup white-screen regression introduced by the port.

### Added

- **Agent Work Intensity card** (Today page): 4 metric tiles (Agent Token / Hours with records / 7-day avg / Cache hit rate) + a 24-hour stacked bar chart colored by source.
- **Token calibration rev8**: `SourceInfo` gains `accounting_revision`; Codex source stamped at rev8 (aligned with macOS); cache version bumped 4 → 5; recalibration notice banner shown on startup when stale calibration is detected.
- **Experimental agent source collectors** (Settings toggle, off by default): reads ZCode (`~/.zcode/cli/db/db.sqlite`, `model_usage`), Hermes (`~/.hermes/state.db`, `sessions`), and WorkBuddy (`~/.workbuddy` directory probe). Usage fields only; no message content.
- Three new Tauri commands: `set_experimental_agent_sources` / `get_recalibration_notice` / `dismiss_recalibration_notice`.
- `tokenToolColor` extended with ZCode (blue) and CC Switch variants; `orderedToolEntries` preferred list extended.

### Fixed

- **Fixed startup white-screen**: `agentWorkCardHTML()` used `var todayKey = todayKey()`, which hoisted a local `todayKey` that shadowed the global `todayKey()` function, throwing `TypeError: todayKey is not a function` inside `render()` and blanking the whole view. Switched to an inline call.

### Internationalization

- Backfilled en / zhHant translations for the Agent Work card (`Agent Token`, `7-day avg`, `Yesterday Sources` were missing from the table); `Today Sources` aligned with upstream wording.

### Known limitations

- ZCode / Hermes / WorkBuddy are not collected by default; enable them under Settings → Experimental Agent Sources (matches macOS upstream).
- Installer remains self-signed; first run triggers a SmartScreen warning.

### Acknowledgements

Ported from the original macOS **TokenStep** by Chaoqiang Huang. This release mirrors upstream v0.1.44/v0.1.45 source.

## [0.1.4] - Rhythm Share Card aligned with macOS

### Added

- **Share card switched to Canvas rendering**: screenshot export now goes through `renderShareDailyCard` / `renderTodayOverview` / `renderRhythmCard` Canvas pipelines, decoupled from on-screen HTML so exported images match the display.
- Rhythm share card aligned with native macOS styling: rounded clipping, peak capsule, gaussian glow, footer credits tokenstep.app as the original author's site.

### Fixed

- Last-30-days bar chart aligned with macOS `StackedActivityBarsView`: bottom alignment (`column-reverse` + pure proportional percentages), stacking order unified with export (`.slice().reverse()`), goal reference line CSS variable fixed (`--mutedFaint` → `--muted-faint`), empty-data placeholder uses `--track` color.
- Contribution wall scrolls to the rightmost (today) by default; restored the 53-week history width that was accidentally truncated.
- Windows Opus pricing synced with upstream v0.1.43 (15/75/18.75/1.5 → 5/25/6.25/0.5).

### Acknowledgements

Mirrored upstream TokenStepSwift v0.1.43 source (Opus pricing + SIGPIPE + Codex quota robustness).

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
