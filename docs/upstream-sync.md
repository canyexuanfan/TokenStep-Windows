# 上游同步跟踪表（upstream-sync）

> 目的：逐条对照 macOS 上游（Backtthefuture/TokenStep）v0.2.24~v0.2.29 的提交，登记「已移植 / 不移植（理由） / 待移植（里程碑）」。以后每次上游发新版，对照本表增量登记一次（月度例检）。
>
> 对照命令：`git log --oneline v0.2.24..v0.2.29`（upstream remote）。
> 基线：本表创建于 Windows 端 v0.1.13（main `b55466d` 之后）。

## v0.2.24 → v0.2.29（25 条，2026-11-18 对照）

| 上游提交 | 内容 | Windows 端处置 | 理由 / 里程碑 |
|---|---|---|---|
| `92a6844` | feat(quota): Trae 浏览器登录 + 自动刷新 token | **待移植** | T6（v0.1.14）：浏览器回调拉起 + 登录态存 credentials 注册表（DPAPI 封存） |
| `82d646e` | fix(quota): Trae 请求改用 Cloud-IDE-JWT 鉴权 | **待移植** | T6（v0.1.14）：随 92a6844 一并移植 |
| `60677ee` | feat(quota): Trae CN 绝对金额展示 + 订阅额度卡 | **待移植** | T6（v0.1.14）：Trae CN 显示已用/剩余绝对金额 |
| `790148f` | feat(quota): 已用/剩余积分不截断 | **待移植** | T6（v0.1.14）：随 Trae 卡一并移植到额度卡通用渲染 |
| `d270209` | fix(sqlite): WAL 数据库 query_only → immutable 三段降级 | **已移植**（0.1.12） | `src/sqlite_readonly.rs` 共享辅助，Cursor state.vscdb 已接入 |
| `e4877c6` | perf(collector): 去重候选索引化（替代成对桶扫描） | **无需移植**（已确认） | Windows 端去重早已是 BTreeSet 索引结构（0.1.12 CHANGELOG 说明段） |
| `0cb6b42` | fix(release): draft 资产校验器 stapler validate 重试 | 不移植 | macOS 公证（stapler）专属，Windows 无此链路 |
| `e2328b0` | chore(release): Apple 票据瞬时失败重试 | 不移植 | 同上，macOS 公证流程 |
| `45d7ef6` | fix(l10n): 英文表去掉重复 subscription quota key | **已等效处理** | Windows 端 i18n 表结构独立维护，无此重复键（如出现单独修） |
| `f58a10a` / `f978473` / `7f05a4f` / `377275e` / `82f7d15` | chore: prepare TokenStep 0.2.2x（版本准备提交） | 不移植 | 上游版本管理提交，Windows 版本号独立演进 |
| `335f565` / `fc5fdc3` / `6629c7a` / `839182d` / `7a94f84` / `aeb1836` / `d5f7284` / `702031b` / `a4fa55b` / `c771a39` | Merge 提交（PR 合并节点） | 不移植 | 合并节点本身无内容；其承载的实际提交已逐条登记 |
| `2d34b2b` | fix(ci): compact popover fixture 检查的格式化辅助编译修复 | 不移植 | 上游 Swift fixture 检查链路专属；Windows CI 独立（ci-windows.yml） |
| `7a94f84`（承载） | Trae 浏览器登录 PR 合并 | 见 92a6844 | — |

## 汇总

- **已移植 / 已等效**：`d270209`、`e4877c6`、`45d7ef6`
- **待移植（T6，v0.1.14）**：Trae 额度四连 `92a6844` / `82d646e` / `60677ee` / `790148f`
- **不移植（macOS 专属 / 版本管理 / 合并节点）**：其余全部
- **净待办**：Trae 额度卡（唯一功能性缺口）

## 例检流程（每次上游发版后执行）

1. `git fetch upstream --tags`
2. `git log --oneline v<旧tag>..v<新tag>` 列出新提交
3. 逐条归类：已移植（附 Windows 提交号）/ 待移植（挂里程碑）/ 不移植（附理由）
4. 涉及数据源路径变化的提交，优先回归对应采集器（见各 questions 文档的"数据源版本锚点"）
