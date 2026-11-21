# 更新日志 | Changelog

本文件记录 TokenStep 的版本变更。Windows 移植版的版本号独立于原 macOS 版。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，遵循 [SemVer](https://semver.org/lang/zh-CN/)。

[English](#english) | 中文

## [0.1.13] - 可靠性与门面（T1~T5）

> 本版按 [`docs/优化路线图-2026-11-18.md`](docs/优化路线图-2026-11-18.md) 阶段一执行。

### 新增

- **Trae / Trae CN 订阅额度卡（T6，移植上游 92a6844/82d646e/60677ee/790148f）**：浏览器登录（回环监听 + IDE 同款授权参数）、Cloud-IDE-JWT 鉴权（Bearer 被拒 401/code1001）、ExchangeToken 自动刷新（refresh token 轮换回写）、CN credits 绝对金额 + 国际版 SG/US entitlement；设置页开关（默认关）+ 登录按钮。
- **二级源接线（T13，D7 修复）**：agent_sources::collect() 此前零调用者——Gemini CLI/Qwen/Kimi/OpenCode/Amp/Droid/Grok Build 七个源从未进入任何面板；现已按设置每源列表接线并入主快照。
- **LiteLLM 定价数据源（T18）**：tools/gen-pricing.py 构建期拉 LiteLLM 公开数据集精简为 36 模型真实分档定价（运行时零网络，PRIVACY.md 承诺不变）；此前落兜底价 1.0 的模型出现真实定价；致谢 LiteLLM/token-eyes 思路。
- **低额度提醒（T9）**：可用且任一窗口已用 ≥80% 时写入托盘 tooltip，不开窗即可见。
- **Release 自动化（T16）**：release.yml——打 v* tag 自动构建并上传 Release 资产（CI 产出不自签，本机签名链路保留）。
- **英文 README（T17）**：README.en.md + 发布说明模板 docs/release-notes-template.md。
- **README 门面（T1）**

### 修复

- **Codex 近两日统计严重失真修复（跨天活跃会话 + 虚高口径双问题）**：用户指出 11-20/11-21 用量计算不对。数据面三层排查定位两个真实缺陷——① **跨天活跃会话漏计**：长会话持续把新用量追加在创建日目录的旧文件里（如 11-17 创建的会话今天仍在跑），旧逻辑只解析"今日目录"，跨天追加行全部漏计（实测 11-21 真实 70M+ vs 旧显示 0.84M）；② **threads 账本虚高**：`state_5.sqlite.threads.tokens_used`（双榜单 reporter 同源）对长会话虚高数百倍（账本记 111.6 亿的会话，rollout 文件仅 42MB——比例检验 3265 tokens/KB 物理不可能），不可作为统计源。修复：`codex_quick_today` 升级为 `codex_quick_recent`——按文件 mtime 抓取最近 48h 活跃的 rollout 文件（含旧目录里的活跃会话，上限 24 文件/256MB），流式解析后**按行内时间戳归日**（真实活动日，而非文件目录日）；解析顺序同时改为会话创建时间降序；threads 补充方案撤销（虚高源）。修复后：11-20 = 83.8M / 11-21 = 128.7M（均为活跃文件真实计费行按真实活动日归集），历史未解析文件由 30 分钟预算的后台扫描渐进补齐。
- **今日 Codex 统计缺失修复**：解析顺序按路径字母序把当日 rollout 排在 7GB 重解析队列最末，且扫描预算结束才落盘、预算成果随进程重启丢失——三重叠加导致当日用量长时间缺席。新增主线程快速路径（现升级为 codex_quick_recent）+ 解析顺序改会话创建时间降序 + 预算维持 30 分钟渐进落盘。
- **Codex 165.8M / Antigravity 42.3M / WorkBuddy 8.0M「未记录模型」回填（model rev 机制）**：三个源各自的模型归属缺陷与修复——① Codex：新格式 `token_usage_record`（新版 CLI 的逐响应用量行）此前被预过滤器整体跳过，现两条解析路径均支持（input 已含 cached，total 以客户端报告值为准）；turn_context 之前记录的用量行按会话开场模型（文件内第一个已知模型）回填。② Antigravity：会话级 Model Selection 状态机本身正确，但旧缓存（该逻辑上线前写入的 unknown 记录）永不失效——新增 `model_rev` 缓存失效标记（通用化自 cursor_est_rev），重解析后归属 "Gemini 4 Hydrogen (High)" 等真实模型。③ WorkBuddy：usage 行缺 model 时按全行 `providerData.model` 就近回填（前向优先、后向兜底）。Codex rev-1 缓存条目系增量路径修复前写入的过渡态毒数据，rev 升 2 强制重解析。
- **Codex 扫描预算 30 分钟 → 4 小时**：sessions 位于符号链接指向的慢速 G 盘（机械盘），全量重解析 7GB/262 文件需数小时；扫描在 detached 线程执行从不阻塞 UI，长预算只意味着"一次扫完存量"而非每个刷新周期重启剩余文件。
- **按模型面板多色化**：`tokenModelColor`（模型名哈希 → 12 色稳定调色板）替代石墨灰兜底，统计页按模型面板与按工具面板同为多色显示；同模型颜色跨视图稳定。
- **MiniMax 25.2 亿「未记录模型」全部归属到真实模型（est rev 2）**：用户指出"不可能没有数据"后重新排查——usage 库 12,531 行 model 为 NULL，但每个会话的明细目录（`~/.minimax/v2/sessions/YYYY/MM/DD/<time>-session_<base64>/`）里 `llm-call.json` 记录会话模型、`messages.jsonl` 每条消息自带 model+usage，实测 100% 可归属。采集器现按 session_id 从会话目录回填（llm-call.json 优先 → messages.jsonl 多数模型），归属结果进 collector-cache（会话级缓存，大文件只在变化时重读）。归属后按模型面板：MiniMax-M3 22.05 亿 / M3.1-Flash 4.95 亿 / M2.7 / M3.1-Flash-Preview，「未记录模型」归零。
- **Cursor 估算重构为计费量级（est rev 3）**：供应商计费按「每次请求重发全部上下文」结算，此前按对话文本一次性估算与官方账单差约 900 倍（真实案例：claude-opus-5-5 官方 54M tokens vs 旧估算 6.1 万）。现从 Cursor 自己写入 composerData 的 `contextTokensUsed`（官方实时上下文记录）× 请求次数（assistant 轮次）估算输入（0.7 = 会话内上下文线性增长中值），输出按 assistant 文本 + tool_use 参数估算；无上下文记录的旧会话退回文本重发下界。模型归属（op-5.5/claude-opus-5-5 等）沿用 composerData modelConfig。历史全部重估（cursor_est_rev 缓存失效标记），全史 252M：grok-4.6 132.1M / op-5.5 18.3M / claude-opus-5-5 14.8M（另两个 claude-opus-5-5 会话的本地明细已被 Cursor 清空为 190 字节存根，其官方用量差值约 39M 本地不可复原）。
- **Cursor tool_use 参数漏计修复（est rev 2）**：agent 转录的工具调用参数是 JSON 对象（`{"command":...}`），此前只取字符串漏掉全部工具参数（单日最多 6.7K tokens）。
- **历史/统计页切换崩溃修复**：按模型面板 tooltip 误引用不存在的 `window.TS.infoIcon`（实为页面局部函数），render 抛 TypeError 中断两个页面切换。
- **按模型面板显示真实模型名（T11/T12）**：此前以 tool 名遮蔽模型名（WorkBuddy 的 4 个模型全显示为 WorkBuddy）；MiniMax 无模型名的 25 亿行诚实标注「未记录模型」；今日页 Cursor 行加「≈」估算徽标。
- **「部分来源失败」误报修复**：Codex 扫描降级状态（数据从缓存回退、完整可用）不再被判为失败，徽标恢复「已同步」。
- **Codex 扫描收敛修复**：扫描预算 30s→30min + 增量尾读 + 目录 mtime 短路缓存，收敛为正常 ok 状态，全来源零失败。
- **分享卡四处显示修复**：圈内大数字自适应字号、总完成度显示真实值（1889%）、来源/模型分解全量展示、新客户端品牌色。
- **今日页底部行自适应布局**：Agent 消耗榜隐藏时消除右侧 ~530px 空列，费用估算卡加宽使「本周预估」金额完整显示。
- **修复启动后崩溃（0xc0000409）**：Codex 扫描超时列表越界——降级处理、绝不崩溃。
- **安装包补回静默覆盖补丁**：覆盖安装不再弹卸载确认页。

### 新增（前批）

- **回归测试安全网（T2）**：为 0.1.12 三个关键修复补齐 `#[test]` 回归——① Codex 扫描超时时 metas 与文件列表索引对齐（启动崩溃 0xc0000409 的根因场景，部分缓存命中 + 部分超时跳过的混合态）② 扫描降级阶梯（memo 回退 → 缓存回退 → degraded 三态状态标记正确）③ 缓存保存点位置（全部采集器之后落盘——晚采集器条目不丢）+ 缓存结构 save→load 往返。测试注入点：`CodexScanScope`（roots / walk deadline / parse deadline 三参数），生产路径零行为变更。
- **上游同步跟踪表（T4）**：新增 [`docs/upstream-sync.md`](docs/upstream-sync.md)，逐条登记上游 v0.2.24~v0.2.29 全部 25 个提交的处置（已移植 / 无需移植 / 不移植+理由 / 待移植 v0.1.14 Trae 额度四连），附月度例检流程。
- **README 门面（T1）**：「当前支持」从 3 家改为 16 家真实清单（专属采集 9 个 + 通用采集 7 个，逐家标注数据源与精度口径：精确/估算）；新增 3 张界面截图（`docs/screenshots/`：分享卡/今日概览/节奏卡）；订阅额度卡补齐 8 家清单。

### 变更

- **CI 升级（T3）**：`ci-windows.yml` 在编译之外新增 `cargo test --lib --release`（阻塞性）；`cargo fmt --check` 与 `cargo clippy --all-targets --all-features -- -D warnings` 从 advisory 升级为**强制**（本地已清零全部告警：24 处 clippy 告警逐条修复 + 全仓 rustfmt）。
- **杂项清账（T5）**：`windows/scripts/badge-check.ps1` 观察窗口延长与轮询条件修正已提交；reporter 私有脚本归位 `reporter/scripts/`（闭源目录，不入库）。

## [0.1.12] - WorkBuddy 额度卡 + WAL 降级 + ZCode/WorkBuddy 统计恢复

（0.1.12 待发内容。参见 [`docs/皮肤包上游对齐规划.md`](docs/皮肤包上游对齐规划.md)。）

### 新增（客户端用量采集补全）

- **Cursor 用量采集（对话文件估算口径）**：解析 `~/.cursor/projects/**/agent-transcripts/**/*.jsonl`，按对话内容估算 token（CJK 0.6/字、其余 1/3.5），日期取自 user 行 `<timestamp>` 标签；官方 API 只记录 Cursor 官方计费模型事件，第三方模型（grok/claude/gemini 等）不产生官方事件，本地对话文件是唯一可算口径。
- **Cursor 按模型归属**：启动时从 `state.vscdb` 的 `cursorDiskKV` composerData 行构建 composerId → modelConfig.modelName 映射（含 selectedModels 回退），会话估算 token 计入其模型（grok-4.6 / claude-opus-5-5 等）。
- **MiniMax 用量采集（全量口径）**：同时读取新旧两个同构 SQLite 存储——`~/.minimax/sqlite.db`（2026-05~07 旧库，token_usage）与 `~/.minimax/v2/sqlite/runtime-state.sqlite`（现行运行时库，local_runtime_token_usage，持续写入）。两库日期区间无缝衔接；跨库用整行指纹（ts+各项 token 数）去重（turn_id 一个回合对应多行，不可用作去重键）。本机全量 29.42 亿 tokens（旧库 3.98 亿 + 运行时库 25.44 亿）；运行时库多数行不记录模型名，显示为 "MiniMax"，显式记录的行归入对应模型（MiniMax-M3 等）。
- **DeepSeek Harness（DSH）用量采集（全量口径）**：主源为 DSH 应用自己的 TokenLens 统计缓存 `~/.dsh/dsh-tokenlens/usage-cache.json`（其统计面板渲染的同一数据源）——按会话文件聚合的 models 分解（providerId/modelId → 按天 + 累计，input 不含 cacheRead），全量 **15.2 亿 tokens 与面板 1.5B 完全一致**，按天柱值与面板近 7 天图一致；计费日志 `.dshw-usage.json`（2026-11-16 因 recharge-fix 重建、缺更早历史）降为回退源。旧实现只读计费日志时仅得 3.79 亿（54 事件），已修正。
- **Codex 采集防冻结（关键修复）**：用户将 `~/.codex/sessions` 迁移为指向 G 盘的 junction（41 GB、仅元数据遍历 >2 分钟），原实现每轮同步扫描会挂死整个刷新流程（首轮快照永不出现）。现 Codex JSONL 扫描在独立线程上运行（私有缓存副本），刷新线程最多等待 20 秒，超时后回退：优先上一次成功扫描结果（memo），无 memo 时直接用共享缓存里已解析的 Codex 历史记录（Codex 不再从统计中消失），来源标记 `*_scan_timeout` 降级；遍历改为日期降序优先（新会话先发现）+ 30 秒预算；扫描不完整时跳过缓存清理（防止误删历史）。
- **今日页「30 天活动热力图」更名「30 天活动条形图」**：该图本就是条形图而非热力图；图中与横轴平行的横虚线是每日目标参考线（带"目标 X"标签，柱子按相对目标的比例分档上色 <50% / 50–90% / ≥90%），保留不动。
- **Antigravity 用量采集**：解析 `~/.gemini/antigravity*/brain/<会话>/.system_generated/logs/chunks/transcript/*.jsonl`（CLI 会话无 chunks 时回退 `logs/transcript{,_full}.jsonl`，按会话只读一份防镜像双计）。MODEL 行自带 input/output/cache_read（input 不含缓存），模型名不在记录里——按会话跟踪「Model Selection」设置变更事件归属（如 Gemini 4 Hydrogen (High)）。本机全量 6952 万 tokens，与独立求和一致。
- **Antigravity 订阅额度卡**：复刻 cockpit 插件的官方配额链路——解密 `Roaming/Antigravity` 密钥存储中 cockpit 封存的账号凭据（Electron safeStorage v10，应用密钥在 `Local State`），按需刷新 access token 后调用 `cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary`，渲染官方周限额 / 5 小时限额窗口。当前试用池账号无有效 license 时如实显示「当前账号无有效订阅许可」；设置 → 订阅额度提供商新增 Antigravity 开关（默认开）。
- **Codex 订阅额度开关默认开启**：此前 `show_codex_quota` 默认关闭且藏在设置深处，订阅额度卡从未出现 Codex（同 Cursor 额度默认开启的处理）；未登录 Codex 时卡片如实显示原因而非隐藏。现有用户配置文件不受影响。

### 修复（Codex 扫描永久降级——让它真正跑完）

- **Codex 扫描收敛修复**：30 秒扫描预算在 G 盘 junction 上永远不够（心跳诊断证实卡在目录遍历本身，实测 >10 分钟），扫描线程每轮中途放弃 → Codex 永久停留在降级状态。三处修复：① 扫描预算放宽到 30 分钟（扫描在后台线程、从不阻塞界面，刷新仍 20 秒即出缓存数据）；② **增量尾读**——追加式 rollout 只解析新增字节（虚拟头事件携带缓存尾锚点续接增量，避免多 GB 文件全量重读）；③ **目录 mtime 短路缓存**（`cache/codex-dirs.json`）——未变化的目录直接复用上次遍历结果，稳态遍历从 10-20 分钟降到秒级。验证：首轮冷扫后 Codex 收敛为 `ok`（176 文件 / 98,877 条，增量每轮补新），全来源零失败。

### 修复（同步徽标误报）

- **「部分来源失败」误报修复**：Codex 的扫描降级状态（`ok_scan_timeout` / `ok_sqlite_scan_timeout` / `cached_scan_timeout`）此前被新鲜度判定归类为"失败"——但这三个状态表示数据已从上次扫描或缓存回退、**可用且完整**（如 G 盘 junction 慢扫期间 Codex 170 亿照常显示），只是新鲜度受限。现归入成功类，徽标恢复「已同步」；仅 `degraded_scan_timeout`（完全无数据，新装机且无缓存）仍算失败。

### 修复（分享卡）

- **分享卡四处显示修复（日报卡/今日概览全页图）**：圈内大数字改自适应字号（「13.27亿」不再溢出圆环）；总完成度去掉 999% 上限、显示真实值（1327%）；「昨日来源/主力模型」不再截断前 4 项、全量展示（面板与画布高度同步自适应）；新增客户端品牌色——DSH 靛蓝、MiniMax 红、WorkBuddy 琥珀、Antigravity 品红、ChatGPT 翠绿（此前新客户端落灰色兜底，「颜色代表客户端」形同虚设）；概览卡分布面板的百分比分母改回全天总量（截断时曾扭曲占比）。

### 修复（启动崩溃 + 缓存保存点）

- **应用启动后崩溃（0xc0000409，关键）**：codex-scan 线程在 30 秒扫描预算耗尽时跳过部分文件的 meta 收集，后续阶段仍按原始文件列表索引 metas → 越界 panic；release `panic = "abort"` 下直接 fail-fast 杀死整个进程（表现为打开后数分钟内崩溃卡退，Codex 会话文件涨到 79+ 后必现）。修复：metas 改为与文件列表索引对齐的 Option 结构（超时文件记 None，后续阶段跳过）——扫描超时只降级、绝不崩溃。
- **采集缓存保存点**：`save_cache` 原在 codex+claude 之后、其余采集器之前，WorkBuddy/Cursor/MiniMax/DSH/Antigravity 的缓存更新从不落盘 → 每轮全量重扫（十几秒）。移到全部采集器之后，此后每轮只做增量。

### 用量准确性审计（独立求和比对）

- ZCode：DB 独立求和 111.16 亿 vs 采集器 111.06 亿（0.09% 边界差，provider_total 回退口径）。
- WorkBuddy：逐 token 精确一致（5,355,045,586）。
- Claude Code：采集器按 response 去重流式重复行（naive 逐行求和会翻倍），以去重后为准。
- Cursor：无官方事件时为纯对话文件估算（官方 overlay 的 daily.tools 用替换而非叠加，不会双计）。

### 新增（上游 v0.2.24~v0.2.29 相关 + WorkBuddy）

- **SQLite 只读三段降级**（上游 v0.2.26 d270209 移植）：新增 `sqlite_readonly.rs` 共享辅助——普通只读 → `PRAGMA query_only` 可写句柄 → `immutable` URI 逐级回退，解决 WAL 数据库在属主应用退出清理 -shm 后无法只读打开的问题；Cursor `state.vscdb` 读取已接入。
- **WorkBuddy 额度卡**：读取本机 `~/.workbuddy/workbuddy.db` 的 `session_usage` 配额池（used/size → 已用百分比 + 剩余/总额），并汇总全部会话 `credit_json` 显示积分消耗合计；设置 → 订阅额度提供商新增 WorkBuddy 开关（默认关）。
- Cursor 额度开关默认开启（此前默认关且开关藏在设置深处）；Cursor 额度失败原因细分（HTTP 状态码/网络错误直接透出，便于区分登录态过期与网络问题）。
- **Cursor 额度通道重写（关键修复）**：新版 Cursor 将登录 token 用 Electron safeStorage（Chromium OSCrypt v10：AES-256-GCM，密钥 DPAPI 包裹于 Local State）加密存储，旧读法拿到密文导致 401。现按 Token Rank 客户端同款配方解密（DPAPI → os_crypt key → AES-GCM），并改用 api2.cursor.sh 的 Connect-Protocol dashboard 端点（Bearer 鉴权），额度卡恢复真实数据（本月花费/调用次数）。401 时提示"登录态已过期，请打开 Cursor 重新登录"。

### 说明

- WorkBuddy 额度模型来自本机 `workbuddy.db`（`session_usage.used/size` 配额池 + `credit_json` 按账号分解消费），无网络请求。
- 上游 v0.2.24~v0.2.29 其余改动（Trae 额度提供商、macOS 公证流程修复等）暂不移植；采集器去重索引化（e4877c6）经对照确认 Windows 端去重已是 BTreeSet 索引结构，无需移植。

## [未发布] / Unreleased

（开发中。参见 [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md)。）

## [0.1.11] - 上游 v0.2.14 榜单身份兼容 + v0.2.7 更新检查韧性

### 修复（上游 v0.2.14 移植）

- **Token Rank 榜单身份 v2/v3 封装格式兼容**：Token Rank CLI 升级后 `~/.token-rank/client-state.json` 从平面状态改为 `{schema_version, payload}` 封装，旧读取逻辑找不到 `user` → 榜单误显示「尚未关联」并隐藏。现在同时兼容平面与 v2/v3 封装格式；遇到未知封装版本**不回退**顶层残留账号（上游同款防泄漏语义）。含 5 个单元测试（平面/v2/v3/未知版本/无 payload）。

### 变更（上游 v0.2.7 移植）

- **更新检查限速回退**：GitHub Releases API 被限速（403）或不可用时，降级请求公开的 `releases/latest` 重定向页（不消耗 API 配额），从最终 URL 解析最新 tag 并重建标准 NSIS 安装包下载地址；该通道未知文件大小，下载进度改用 Content-Length。
- **手动检查 10 秒冷却**：设置页「立即检查」改走 `check_for_update_manual`，10 秒内重复点击直接重放上次结果，不再连打 GitHub API（防止烧配额与转圈卡住）；启动自动检查不受影响。含 release tag 解析单元测试。

## [0.1.13] - Reliability & presentation (T1~T5)

> Executed per stage one of [`docs/优化路线图-2026-11-18.md`](docs/优化路线图-2026-11-18.md).

### Added

- **Regression test safety net (T2)**: `#[test]` coverage for the three 0.1.12 critical fixes — (1) Codex metas/file-list alignment on expired deadline (the 0xc0000409 crash shape: partial cache hits + skipped files), (2) the scan degradation ladder (memo → cache → degraded statuses), (3) the cache save point after ALL collectors + a save→load round trip. Seam: `CodexScanScope` (roots / walk deadline / parse deadline) — zero behavior change in production.
- **Upstream sync table (T4)**: `docs/upstream-sync.md` logs all 25 upstream commits from v0.2.24 to v0.2.29 (ported / not-needed / not-ported with reasons / pending Trae quota quartet for v0.1.14), plus a monthly review process.
- **README facelift (T1)**: the support list went from 3 clients to the real 16 (9 dedicated collectors + 7 generic ones, each with its data source and precision label); three UI screenshots added under `docs/screenshots/`; the 8-provider quota-card lineup is now documented.

### Changed

- **CI upgrade (T3)**: `ci-windows.yml` adds a blocking `cargo test --lib --release` step; `cargo fmt --check` and `cargo clippy -- -D warnings` promoted from advisory to **enforced** (all 24 pre-existing clippy warnings fixed locally + repo-wide rustfmt).
- **Housekeeping (T5)**: `windows/scripts/badge-check.ps1` window/poll fixes committed; the reporter-private script moved to `reporter/scripts/` (closed-source dir, not tracked).

## [0.1.12] - WorkBuddy quota card + SQLite WAL fallback + collector fixes

### Added

- **Cursor usage collector (transcript estimation)**: parses `~/.cursor/projects/**/agent-transcripts/**/*.jsonl` and estimates tokens from conversation text (CJK ≈ 0.6/char, other ≈ 1/3.5); the official API only records Cursor-billed models, so local transcripts are the only source for third-party models.
- **Cursor per-model attribution**: builds a composerId → modelConfig.modelName map from `state.vscdb` composerData rows at startup; each session's estimate lands on its model.
- **MiniMax usage collector (full history)**: reads both same-schema SQLite stores — legacy `~/.minimax/sqlite.db` (May–Jul 2026) and the live `~/.minimax/v2/sqlite/runtime-state.sqlite` — with full-row-fingerprint cross-store dedup (turn_id fans out to many rows per turn). ~2.94B tokens locally; rows without a recorded model display as "MiniMax".
- **DeepSeek Harness (DSH) usage collector (full history)**: primary source is the harness's own TokenLens stats cache `~/.dsh/dsh-tokenlens/usage-cache.json` (the exact source its stats panel renders) — per-session-file model breakdown with per-day + total counts; grand total **1.52B matches the panel's 1.5B** and per-day bars match its 7-day chart. The billing journal (rebuilt 2026-11-16, missing older history) demotes to a fallback. The journal-only implementation previously showed 379M.
- Today-page card renamed 「30 天活动热力图」 → 「30 天活动条形图」 (it is a bar chart, not a heatmap). The horizontal dashed line is the daily-goal reference line (tagged "目标 X"; bars are intensity-graded against it) — kept as is.
- **Antigravity usage collector**: parses `~/.gemini/antigravity*/brain/<session>/.system_generated/logs/chunks/transcript/*.jsonl` (CLI sessions without chunks fall back to `logs/transcript{,_full}.jsonl`, one store per session to avoid mirror double-counting). MODEL records carry input/output/cache_read; the model name is tracked per session from "Model Selection" settings-change events. ~69.5M tokens locally, matching an independent sum.
- **Antigravity subscription-quota card**: replicates the cockpit extension's official quota chain — decrypt the cockpit-sealed account credential from the Antigravity app's secret storage (Electron safeStorage v10), refresh the access token as needed, then call `cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary` and render the official weekly / 5-hour windows. Accounts without a valid license surface an honest message. Settings → Quota providers → Antigravity toggle (default on).
- The Codex subscription-quota toggle now defaults to on (it used to sit off by default, buried in settings, so the quota card never showed Codex — same treatment as the Cursor quota default-on flip). Existing settings files are untouched.
- **WorkBuddy quota card**: reads the local `~/.workbuddy/workbuddy.db` credit pool (used/total, credits spent) — no network requests. Settings → Quota providers → WorkBuddy toggle (default off).
- **WorkBuddy usage collector**: scans the per-project JSONL event streams (`~/.workbuddy/projects/**/*.jsonl`) — per-request tokens now feed the per-client card and totals.
- SQLite read-only three-step fallback (readOnly → query_only → immutable), ported from upstream v0.2.26; Cursor's state.vscdb uses it.

### Fixed

- **Codex scan convergence fix**: the 30-second scan budget could never cover a G-drive junction walk (heartbeat diagnostics showed the scan stuck in the walk phase itself, >10 min), so the scan aborted mid-walk every pass and Codex sat in degradation forever. Three fixes: budget raised to 30 min (background thread — the UI still serves cached data after 20 s); **incremental tail reads** for append-only rollouts (a virtual head event carrying the cached tail anchor resumes deltas exactly, no multi-GB re-reads); and a **directory-mtime short-circuit cache** (`cache/codex-dirs.json`) dropping steady-state walks to seconds. Verified: after one cold pass Codex converges to `ok` (176 files / 98,877 records, incrementals pick up new turns every pass) — all sources green.

- **「部分来源失败」false alarm fix**: Codex scan-degradation states (`ok_scan_timeout` / `ok_sqlite_scan_timeout` / `cached_scan_timeout`) were classified as failures by the freshness badge even though they carry usable, complete data from the last scan or cache fallback (Codex's 17.0B kept showing during slow G-drive scans). They now classify as success — the badge reads 已同步 again; only `degraded_scan_timeout` (zero data) remains a failure.

- **Share cards, four display fixes (daily card + today overview)**: the in-ring token number now auto-fits its font (「13.27亿」no longer overflows the ring); the completion percent drops its 999% cap and shows the real value (1327%); the source/model breakdowns list ALL nonzero entries instead of the top 4 (panel + canvas heights scale accordingly); new clients get distinct brand colors — DSH indigo, MiniMax red, WorkBuddy amber, Antigravity fuchsia, ChatGPT emerald (they used to fall into graphite gray). The overview distribution panel's percent denominator is the full-day total again (truncation used to skew it).

- **App crash right after startup (0xc0000409, critical)**: when the Codex scan budget ran out, skipped files desynced the metadata list from the file list and a later phase indexed out of bounds — with `panic = "abort"` that fail-fasted the whole process. Metas are now index-aligned Options (timed-out files record None; later phases skip them) — a scan timeout degrades, never crashes. The collector cache is also saved after ALL collectors now (previously WorkBuddy/Cursor/MiniMax/DSH/Antigravity updates never hit disk, forcing full rescans every pass).

- **Codex collector freeze**: a 41 GB G-drive junction at `~/.codex/sessions` made the synchronous per-pass scan hang the whole refresh pipeline (first snapshot never arrived). The scan now runs on a detached thread (private cache clone); the refresh waits at most 20 s, then falls back to the last completed scan (memo) — or, before any memo exists, to the Codex records already parsed in the shared cache, so Codex never disappears from the stats (`*_scan_timeout` degraded status). Newest-first walk with a 30 s budget; cache pruning is skipped after partial scans to protect history.

### Changed

- Cursor quota toggle now defaults to on; quota failures surface the HTTP status / network error instead of a bare message.

### Fixed

- ZCode / WorkBuddy usage missing from the per-client card (experimental-sources switch had been reset; WorkBuddy had no collector at all).
- Collector cache version bumped to v12 to discard entries written during the rolled-back incremental-append experiment.


## [0.1.10] - 订阅额度"未配置"修复 + 打包流水线修复

### 修复

- **订阅额度一直显示「未配置」**：根因是新版 `@openai/codex` npm 平台包（如 0.149.1-win32-x64）没有 `bin` 字段、npm 不生成 PATH shim，额度读取器找不到 codex 二进制。现在 `find_codex()` 在 PATH 未命中时回退扫描 `%APPDATA%\npm\node_modules\@openai\codex\vendor\<target>\bin\codex.exe` 与 `%LOCALAPPDATA%\pnpm\codex.exe`，Codex 订阅额度（ChatGPT 登录态）恢复显示。
- **失败原因透出**：Codex / Claude 额度读取失败时不再被静默丢弃——provider 列表会带上 `unavailable` 状态与具体原因（如「未找到 codex 命令」「未登录 Claude Code」），前端区分「未配置」（什么都没开）与「暂不可用 · <原因>」，不再一律显示误导性的「未配置」。
- **额度缓存版本化**：quota 缓存新增 `v` 字段（v2），升级后自动废弃旧版空列表缓存，避免 15 分钟 TTL 内继续显示旧状态。
- Claude 额度卡需官方 `claude login`（OAuth）才会显示；走 API 中转（`ANTHROPIC_AUTH_TOKEN`/`BASE_URL`）时无官方订阅额度可查，卡片将显示原因而非报错，属预期行为。

### 变更

- 今日页紧凑布局：顶栏压缩、卡片间距收紧、top 区固定高度，900px 高度屏幕一屏放下核心卡片。
- 品牌区使用真实 logo.png（原为字母占位）；今日 Token 卡改为圈数摘要布局（当前圈标题 + 已完成 + 每圈目标），环图颜色与贡献色统一（`contributionColor`）。
- `ringSvg()` 新增可选 `progressColor` 参数，不传参保持原渐变，其他调用点不受影响。

### 打包流水线修复（内部）

- `build-release.bat` 内联 PowerShell NSIS 补丁被 cmd 转义打碎（exit 255、签名步骤永远走不到）→ 补丁抽出为独立 `patch-nsis.ps1`（幂等、兼容 LF/CRLF、自动改名 nsis-output.exe）。
- makensis 优先选用 `Bin\makensis.exe`（NSIS 根目录的 91KB `nsis.exe` 是 GUI 桩，控制台模式静默 exit 1）。**受此影响，历代 setup.exe 从未真正带上「跳过重装确认页」补丁，0.1.10 起首次生效**（覆盖安装不再弹确认）。
- `sign.bat` 版本匹配从 `TokenStep_*` 通配改为钉死当前版本，修复字母序（0.1.9 > 0.1.10）导致旧包冒名部署的问题。
- 新增 `cargo run --example quota_diag`：不起 GUI 端到端诊断 Codex/Claude 额度读取链路。

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

### 0.1.11 后续：环境动效 + 上市前 polish（真机实拍 7/7 终验通过）

- **环境动效移植**（上游 v0.2.5 火焰 + v0.2.7 事件视界呼吸的 Web 版）：皮肤包激活时注入两层固定悬浮层（z-index:-1，板面之上、内容之下），奥德赛三章金色缓洗、特洛伊火海火焰明暗闪烁+余烬上浮、事件视界象牙光呼吸；只动 opacity/transform（合成器开销），遵循上游暂停纪律——prefers-reduced-motion 不启动、窗口失焦/最小化冻结、分享卡继续用确定性静态 canvas。
- **polish 修复**（独立审查 agent 终版截图逐张发现的 10 项中修 5 项）：事件视界面板加深（亮带不再直穿图表区）；节奏图峰值 chip 主题化（暗包深底浅字，经典包保持白底黑字节等不变）；"白色环底轨"实为近透明卡片后板面亮区透出——环区加深色 halo；按小时/按天按钮暗包下浅象牙化。其余 5 项（Logo 按包换色、网格线降亮等）记为后续迭代。
- 验证：真机实拍 7 张（今日 4 章包 + 经典 violet 回归 + 设置页/历史页）由独立验收 agent 逐张判定 7/7 通过；FX-CHECK 证明动效层与动画已挂载；实拍脚本改为驱动真实设置路径切肤（内存 applyTheme 不触发重渲染，SVG 内联色会停留在启动主题——排查细节见 questions/）。
- 实拍工具收敛：保留 cdp-verify（全链路）与 verify-skins-e2e（皮肤包专项，真实 UI 驱动+确定性冻结帧），移除 5 个一次性调试脚本。

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
