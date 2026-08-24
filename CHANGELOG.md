# 更新日志 | Changelog

本文件记录 TokenStep 的版本变更。Windows 移植版的版本号独立于原 macOS 版。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，遵循 [SemVer](https://semver.org/lang/zh-CN/)。

[English](#english) | 中文

## [未发布] / Unreleased

（开发中。参见 [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md)。）

## [0.1.9] - 今日页参考设计布局

### 变更

- 今日页主内容按 GPT-Image-2 参考设计重新编排为进度、节奏、活动、分布、额度与摘要卡片布局。
- 今日页最近 30 天区域使用按客户端分色的堆叠柱状图；历史页活动墙保持不变。
- 今日页保留并纳入设计图的圈数进度、累计/活跃/达标指标条、Agent 消耗榜和 Agent 工作强度卡。
- 顶部导航、截图、GitHub 入口、订阅额度、Agent 榜单与本地隐私逻辑保持原有功能链。
- 已完成一次实际浏览器渲染验收：顶部壳层、三行卡片几何、合并指标条、活动卡按天入口和额度占位均按参考图校准。
- 圈数环和圈数进度卡统一使用当前圈进度；完成一圈后进入下一圈时不会被总量百分比错误截成 100%。
- 保持共享 `app.js` 组件实现不变，仅调整 Today 页面呈现层、翻译补充和 CSS 布局。

### 0.1.9 后续视觉修正

- 消耗榜改为参考图的三列领奖台布局，支持真实榜单数据、总量聚合和空数据等待态。
- 订阅额度卡不再因异步读取时机暂时消失，provider 返回后沿用真实额度窗口渲染。
- 费用估算按参考图优化为今日/本月双金额、较昨日变化和预算底栏；未改后端计算接口。

### 0.1.9 稳定性修复（缓存 2GB + 设置页打不开）

- **采集器缓存无界增长修复**：v10 把每个 Codex 事件的累计快照全部写入 `codex_anchors`，5 天膨胀到 2.08 GB（31.6 万条记录仅占约 135MB，其余 1.9GB 全是快照；单文件可达数万条、同秒重复）。现在持久化前抽稀为最多 256 个锚点（首尾保留 + 等距采样，`thin_anchors`，含单元测试）；缓存版本升至 v11 自动作废旧文件；`load_cache` 增加 256 字节头部版本探测（版本不符不再解析整个文件）；`save_cache` 改紧凑序列化。
- **设置页打不开修复**：tab 重构后 SECTIONS 不含 `settings`，`render()` 对 `section.title` 的无保护访问在点 ⚙ 时抛 TypeError，设置视图永远渲染不出来（自 0.1.8 tab 重构起潜伏；Codex 重构版曾因把 settings 混入 SECTIONS 而短暂掩盖）。现在非导航视图安全跳过标题更新，设置页 12 张卡与 5 个主题色块恢复可用。
- 隐私页「本地数据文件」路径文案修正：`data\usage.json`（实际不存在）→ 真实的 `cache\collector-cache.json`。
- 新增 `windows/scripts/cdp-verify.ps1`：通过 WebView2 远程调试协议（CDP）对真实运行的应用做端到端验证（等待真实数据 → 今日页 → 历史/隐私 tab → 设置页 → 真实切换主题并还原 → 缓存体积/内存），替代易误判的 mock harness。
- 计量口径说明：经字段构成核实，Codex 日均 12~28 亿 token 为真实用量（input+cache_read 主导，长上下文每轮全量重发），非计量 bug。

## [0.1.8] - 上游 v0.2.1/v0.2.2 同步：Cursor 官方用量 + 多供应商额度 + 模型仪表盘

### 新增

- **Cursor 官方用量进环**（v0.2.1）：读取 Cursor 登录态（state.vscdb），分页拉取官方用量事件，本地缓存窗口替换，幂等 overlay 合并进今日环/来源/模型/节奏/Agent 强度（token 口径 input+output+cacheRead+cacheWrite）。默认关闭（设置 → Cursor）。
- **统一订阅额度体系**（v0.2.2）：一张「订阅额度」卡聚合 Codex / Claude / Cursor / GLM / Kimi / Grok 六供应商；剩余百分比、低额橙色（<20%）、告急置顶；未配置/读取失败整卡隐藏绝不显示 0%；15 分钟 TTL。GLM（API Key）/ Kimi（OAuth token）/ Grok（grok login 会话）三探针默认关闭，凭证存本机（HKCU 专键，对应上游钥匙串语义）。
- **今日模型消耗表**（v0.2.2 model usage dashboard）：全宽表格（模型/进度条/Token/占比/金额估算），<0.1% 尾行过滤（前两行保底）、最多 5 行 + 「还有 N 个」、FNV 色槽、明细与总量不一致提示。
- **Agent 强度卡改版**（对齐 v0.2.2）：三大数字（模型请求/工具调用/缓存命中）+ KV（输入/缓存读取/输出）。
- **Cursor 代码信号卡**（L3，默认关闭）：今日 Cursor 代码产出计数（块/文件/模型/会话），不进 Token 统计。

### 移除（对齐上游 v0.2.2）

- 7 个 T1 实验源（Gemini CLI/Qwen/Kimi/OpenCode/Amp/Droid/Grok Build）退出采集管线（上游同）。
- **项目维度整体移除**：今日项目卡、分享卡/概览项目面板、统计页按项目列（上游 v0.2.2 已删）。
- 隐私页新鲜度五态图例（上游徽章事实性退役）。

## [0.1.7] - 零遗漏收尾 + 侧边栏 GitHub 入口

### 新增

- **侧边栏 GitHub 入口**：GitHub 按钮（打开仓库页）+ 一键 Star（学习 opencodex 的 sidebar 模式）。星标走本机 `gh` 登录，程序不存储任何 GitHub token；未登录时兜底打开仓库页。
- **分享成绩单卡「今日/昨日路线」项目面板**：前 3 个项目目录 + token + 百分比（对齐上游 G-B1，此前漏移植）。
- **Agent 工作强度卡完全体**：今日/近 7 天切换（7 个日历日缺日补零、x/168、分时日均）；来源过滤（全部/Codex/Hermes/其他，codex via 归一）；缓存命中率折线（右侧 100%/0% 轴、覆盖不完整断线）。
- **历史页改版**：「近 8 个月活动墙」头卡 + 活跃日胶囊；明细表 4 列（日期/Token/金额/主力工具色点）。
- **设置页**：「自动刷新」+ 当前节奏状态行。

### 修复

- **节奏分享卡数据错位**：标题一直写「昨日 AI 节奏」但数据取的是今日——改为真·昨日数据。
- **今日概览截图**补「今日路线」项目面板。

## [0.1.6] - 上游 v0.2.0 全量移植 + Codex 计量修复 + 对齐审查

同步上游 macOS v0.1.46~v0.2.0（47 文件 +8612 行），并修复 Windows 版 Codex 计量的五层根因。

### 新增

- **7 个实验 Agent 源**（Gemini CLI / Qwen Code / Kimi Code / OpenCode / Amp / Droid / Grok Build）：逐源开关、自动纳入已安装源、统一 token 口径换算。
- **项目维度（B1-lite）**：「今日项目」卡（进度条 + 工具摘要）、统计页「按项目」列、五类源的工作目录脱敏提取。
- **新鲜度模型（六态）**：页头徽章 + 隐私页状态图例 + per-channel 尝试记录持久化；未使用的源正确归类为"缺席"而非"失败"。
- **能耗策略**：后台采集地板（交流 15 分钟 / 电池·节电 30 分钟）、前台 tick 上限 60 秒。
- **Agent 消耗榜**（替换生财榜）：zhenganhuo.com 接口 + Token Rank 本地身份自动识别 + 三态可见性（默认隐藏 = 零身份读取零网络请求）+ 点击跳转 + 头部「N 人 · X 分钟前」。
- **ZCode session-join**：SQL 条件拼接（provider_total_tokens 列探测 + session 表 join 取项目目录）。

### 修复

- **Codex 计量五层根因**（今日 58.35 亿 → 1.24 亿，全历史 11383 亿 → 402 亿）：
  1. O(N²) 累加 → 完整移植上游 rev8 delta 校准（哨兵值/可信重置/字段分摊/requestID 去重）；
  2. compact 转储重复 → fork anchor + dump 检测按创建时刻切分；
  3. resume 继承计数器虚高 847 亿 → 首事件继承保护（超上游的治本修复）；
  4. Codex 缓存被 Claude 的保存清空 → 共享缓存单次读写；
  5. 特定文件采集卡死 → 上述修复连带消除（扫描恢复 5 分钟）。
- **启动白屏**：新增函数误插在 viewSettings 作用域内导致 ReferenceError。
- **Agent 榜 token 显示 0**：序列化字段名误用 camelCase；models 字段实为 map。
- **Codex 额度无法识别登录账号**：npm 的 codex.cmd 需经 cmd.exe /c 执行；stdin 管道提前关闭导致 app-server 在响应前退出；补 12 秒读取超时。
- **额度卡对齐**：显示剩余百分比（原为已用）、相对倒计时（原为绝对时间）、从未读到的供应商整卡隐藏。
- **"部分来源失败"误报**：missing_proxy_rows / fallback_threads / discovered_no_usage 三个未使用态被误判为失败。

### 已知限制

- 多级 fork 链（父文件已删除）未持久化 orphan anchors，个别孤儿 resume 文件可能仍有少量继承残留。
- 设备同步仅有设置字段占位（对齐上游 v0.2.0 的死代码状态）。

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

## [0.1.9] - Today page redesign + stability fixes

### Changed

- Today page re-laid-out per the GPT-Image-2 reference design (progress, rhythm, activity, breakdown, quota, rank, cost cards).
- Rank card: three-column podium layout with real leaderboard data and empty-waiting state; quota card keeps real provider windows; cost card shows today/this-month amounts with day-over-day delta.

### Fixed

- Collector cache unbounded growth: v10 persisted every Codex cumulative snapshot into `codex_anchors`, reaching 2.08 GB in five days. Anchors are now thinned to at most 256 per file (first/last kept verbatim, plus an even sample); cache version bumped to v11 so legacy files are discarded without being parsed; `load_cache` probes the version from the first 256 bytes before parsing; `save_cache` writes compact JSON.
- Settings page could not open: after the header-tab restructure `SECTIONS` no longer contained `settings`, and the unguarded `section.title` lookup in `render()` threw a TypeError before the settings branch ran. Non-nav views now skip the title update; all 12 settings cards and 5 theme swatches are reachable again.
- Privacy page "local data files" path now shows the real `cache\collector-cache.json` instead of a nonexistent `data\usage.json`.

### Added

- `windows/scripts/cdp-verify.ps1`: end-to-end verification of the real running app over the WebView2 remote-debugging protocol (waits for live data, checks today/history/privacy tabs, opens settings, switches theme and restores, reports cache size and memory).

## [0.1.8] - Upstream v0.2.1/v0.2.2: Cursor official usage + multi-provider quotas + model dashboard

### Added

- **Cursor official usage in the ring** (v0.2.1): reads the local Cursor login (state.vscdb), pages the official usage API, caches with window replacement, and merges idempotently into today's ring/sources/models/rhythm/agent-work. Off by default.
- **Unified subscription quotas** (v0.2.2): one card aggregating Codex / Claude / Cursor / GLM / Kimi / Grok with remaining percents, low-warning orange (<20%), lowest-first ordering; unconfigured providers stay hidden — never 0%. 15-minute TTL. GLM/Kimi/Grok probes default off; credentials stay local (registry key ≈ upstream Keychain).
- **Today's model usage table** (v0.2.2): full-width table with progress bars, shares and cost estimates; <0.1% tail rows filtered, max 5 rows + "N more", FNV color slots, total-mismatch hint.
- **Agent intensity card, simplified** (v0.2.2): three big numbers (requests / tool calls / cache hit) + input/cache-read/output rows.
- **Cursor code signal card** (L3, off by default): today's Cursor code output counts; not counted as tokens.

### Removed (upstream v0.2.2 parity)

- The 7 T1 experimental sources leave the collection pipeline (as upstream).
- **Project dimension removed**: today's projects card, share/overview project panels, the by-project stats column.
- Privacy-page freshness legend (the badge is retired upstream).

## [0.1.7] - Zero-gap polish + sidebar GitHub entry

### Added

- **Sidebar GitHub entry**: repo button + one-click star (after opencodex's sidebar pattern). Starring uses the local `gh` login — no GitHub token is ever stored by the app; logged-out users fall back to the repo page.
- **Share-card "Today's route" projects panel**: top-3 project folders with tokens and percents (upstream G-B1, previously missed).
- **Agent Work card, complete**: Today / last-7-days toggle (calendar-day zeros, x/168, hourly average); source filter (All / Codex / Hermes / Other, "codex via" folding); cache-hit-rate line with a right 100%/0% axis.
- **History page redesign**: "Last 8 months" wall header + active-days capsule; 4-column detail table (date / tokens / cost / dominant tool with color dot).
- **Settings**: "Auto refresh" card with a current-pace status line.

### Fixed

- **Rhythm share card data mismatch**: the title always said "Yesterday's AI rhythm" while the data came from today — now genuinely yesterday's.
- **Today-overview screenshot** gains the "Today's route" projects panel.

## [0.1.6] - Upstream v0.2.0 full port + Codex accounting fix + alignment audit

Syncs upstream macOS v0.1.46~v0.2.0 (47 files, +8612 lines) and fixes five layered root causes in the Windows Codex accounting.

### Added

- **7 experimental agent sources** (Gemini CLI / Qwen Code / Kimi Code / OpenCode / Amp / Droid / Grok Build): per-source toggles, auto-enrollment of installed sources, unified token-caliber conversion.
- **Project dimension (B1-lite)**: Today's-projects card, a by-project stats column, sanitized working-directory extraction across five source families.
- **Freshness model (six states)**: header badge, privacy-page legend, per-channel attempt persistence; unused sources classified as absent, not failed.
- **Energy policy**: background collection floor (AC 15 min / battery 30 min), foreground tick capped at 60 s.
- **Agent usage rank** (replaces the scys board): zhenganhuo.com API + Token Rank local-identity auto-detect + tri-state visibility (hidden by default = zero identity reads, zero network) + click-through + "N users · X min ago" header.
- **ZCode session-join**: conditionally assembled SQL (provider_total_tokens probe + session join for the project directory).

### Fixed

- **Five layered Codex accounting root causes** (today 5.835B → 124M, all-time 113.8B → 4.02B):
  1. O(N^2) summation → full rev8 delta port (sentinels, credible resets, field allocation, requestID dedupe);
  2. compact-dump duplication → fork anchor + dump detection split at creation time;
  3. resume counter inheritance inflating by 84.7B → first-event inherited-counter guard (a fix beyond upstream);
  4. Codex cache wiped by Claude's save → one shared cache per pass;
  5. collection hang on certain files → eliminated by the above (full scan back to 5 minutes).
- **Startup white screen**: newly added functions were nested inside viewSettings' scope (ReferenceError).
- **Rank tokens showing 0**: camelCase serialization names; models is a map, not a list.
- **Codex quota not detecting the signed-in account**: npm's codex.cmd needs cmd.exe /c; the stdin pipe closed before the app-server could respond; added a 12s read timeout.
- **Quota card alignment**: remaining percent (was used), relative countdown (was absolute), never-succeeded providers hidden entirely.
- **"Some sources failed" false positives**: missing_proxy_rows / fallback_threads / discovered_no_usage (unused states) misclassified as failures.

### Known limitations

- Multi-level fork chains whose parent file was deleted keep no orphan anchors; a few orphan resume files may still carry small inherited remainders.
- Device sync is a settings-field placeholder only (mirrors upstream v0.2.0's dead-code state).

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
