# TokenStep 双榜单上报组件（reporter）PRD

> 版本：v1.0（2026-08-01）
> 状态：已批准，实现中
> 关联：`docs/PRIVACY.md`、`docs/溯源分析-TokenRank-EXE来源.md`、`questions/zhenganhuo-token-rank接入记录.md`

## 一、目标

在 TokenStep 仓库新增**独立的、开源之外的**上报组件（代号 `reporter`），它：
1. 自己从原始日志采集 AI 编程工具的 token 用量
2. 把同一份数据**分别按各自协议**上传到两个榜单：生财(scys.com) + zhenganhuo.com
3. 最终替代现有的 opentoken + token-rank 两个独立客户端，实现「单一工具接两个榜单」

## 二、硬约束（不可违反）

1. **开源主程序绝对纯净**：`windows/src-tauri/` 下不得出现任何 POST/upload/ingest 逻辑。
   - `docs/PRIVACY.md` 第 16 行承诺 "does not upload anything by default"
   - `docs/溯源分析` 把"主 exe 含上报逻辑"明确定义为本项目要避免的反面模式
   - reporter 必须是独立 Cargo 工程，编译出独立 exe，开源仓库不收录其源码
2. **opt-in**：reporter 默认不跑，用户显式配置 + 安装计划任务才运行（PRIVACY.md 第 36 行）
3. **开发期不破坏现有接入**：opentoken(生财) + token-rank(zhenganhuo) 并存验证，reporter 稳定后再下线
4. **凭证隔离**：两榜单 token 存 reporter 私有配置，不进开源 settings.json

## 三、数据源策略

**reporter 自采原始日志**，不依赖 opentoken 输出或 TokenStep usage.json。
原因：zhenganhuo 要 20+ 行为字段（automation_score/tool_calls/reasoning_steps 等），只有原始日志才有，聚合数据无法反推。

### 平台覆盖度（诚实分级）

| 平台 | 可行性 | 数据源 | 字段情况 |
|---|---|---|---|
| codex | ✅ 完整 | `~/.codex/sessions/**/*.jsonl` + `archived_sessions/` | token_count 事件：input/output/cached_input/reasoning/total |
| claude-code | ✅ 完整 | `~/.claude/projects/**/*.jsonl` | message.usage：input/output/cache_creation/cache_read |
| zcode | ✅ 完整 | `~/.zcode/cli/db/db.sqlite` (model_usage 表) | 字段最全最规范，作校验基准 |
| minimax | ✅ 完整 | `~/.minimax/v2/sessions/**/ledger.jsonl` | usage 有 total/input/output/cache_read，但无模型名(占位) |
| cursor | ⚠️ 降级 | `%APPDATA%\Cursor\...\state.vscdb` | **本机 tokenCount 全为 0**，仅时间/会话，无 token 数 |
| workbuddy | ⚠️ 降级 | `~/.workbuddy/workbuddy.db` | 只有 credit 金额，无逐条 token |
| antigravity | ↩️ 复用 | 复用 claude-code 采集 | 无原生数据，避免重复计数 |

## 四、双榜单协议（已调研清楚）

### 生财(scys) - opentoken 协议
- **端点**：`POST {webhook_url}`（token 在 URL 路径里，无 header 签名）
- **认证**：webhook_token（52字符，path style）
- **数据**：
  - v1 `rows`（日级8字段：date/model/tool/input/output/cache_read/cache_write/normalized）
  - v1 `sessions`（日级7字段：date/tool/sessions/messages/user_messages/active_seconds/duration_seconds）
  - v2 `v2_hourly`（UTC小时桶7字段：hour_utc/model/tool/input/output/cache_read/cache_write）
  - v2 `v2_sessions`（会话级8字段：session_key(SHA1)/date/tool/started/ended/messages/user_messages/active_seconds）
- **增量**：本地水位 watermark（防删对话回退榜单）
- **v2 注册**：secret/seq/trust 机制，v2-ping 探活

### zhenganhuo - token-rank 协议
- **端点**：
  - `install-register.php`（注册：用 install-token 换 device_token，7天有效期）
  - `sync-plan.php`（协商：拉服务端已有指纹，算增量）
  - `ingest.php`（上传：Bearer 认证）
  - `sync-status.php` / `me.php`（查询）
- **认证**：`Authorization: Bearer {device_token}`（64hex）
- **数据**：
  - `hourly_model`（上海时区小时桶，**19字段**）
  - `sessions`（会话级，**25字段**，含 automation_score/usage_mode/usage_mode_confidence/classification_version）
- **增量**：`last_synced_snapshot` 指纹协商

### 两边关键差异（适配器要处理）
1. **时区**：生财 UTC 小时桶 vs zhenganhuo 上海小时桶 → 同一事件按各自时区重新分桶
2. **认证**：生财 URL token vs zhenganhuo Bearer header → 两套 HTTP 客户端
3. **粒度**：生财瘦协议(7-8字段) vs zhenganhuo 富协议(19-25字段+防刷) → 从原始事件分别派生
4. **续期**：生财 webhook 长期有效 vs zhenganhuo device_token 7天过期需 install-register 续期

## 五、架构设计

### 工程位置与形态
```
F:\zcode\projects\tokenstep\TokenStep\
├── windows/              # 开源主程序（保持纯净，零上传，git 跟踪）
├── reporter/             # 【新增】闭源上报组件（.gitignore，永不提交）
│   ├── Cargo.toml        # 独立工程，不 import tokenstep_lib
│   ├── src/
│   │   ├── main.rs       # CLI 入口（clap）
│   │   ├── collect/      # 各平台采集器（参考 collector.rs 重写）
│   │   ├── classify.rs   # automation_score/usage_mode 启发式分类器
│   │   ├── sinks/        # 双榜单适配器
│   │   ├── auth.rs       # zhenganhuo device_token 续期
│   │   ├── state.rs      # 双套增量水位
│   │   └── config.rs     # 私有配置读写
│   └── target/release/tokenstep-reporter.exe
```

### 技术栈（与主程序一致，便于复用经验）
- Rust 2021，rust-version 1.77
- `reqwest` 0.12 blocking + rustls-tls（与主程序 token_rank.rs 一致）
- `serde_json` 1、`rusqlite` 0.32 (bundled, READ_ONLY)、`clap`、`chrono` 0.4

### 数据流
```
原始日志 → collect/*.rs 采集 → 统一 RawEvent{ts,session,model,tokens,behavior}
                                      ↓
                              classify.rs 分类(automation_score/usage_mode)
                                      ↓
                              两套聚合（按各自时区/粒度）
                                      ├→ sinks/scys.rs      → 生财(UTC+瘦协议)
                                      └→ sinks/zhenganhuo.rs → zhenganhuo(上海+富协议)
```

## 六、分阶段任务

- **阶段0 工程骨架（0.5天）**：reporter/ Cargo工程 + gitignore + clap CLI + config + RawEvent
- **阶段1 采集器（2-3天）**：codex/claude/zcode/minimax 完整 + cursor/workbuddy 降级
- **阶段2 分类器（1-2天）**：classify.rs 算 automation_score/usage_mode
- **阶段3 生财sink（1天）**：webhook POST v1+v2，UTC，watermark 增量
- **阶段4 zhenganhuo sink（2-3天）**：install-register 续期 + sync-plan 协商 + ingest 19/25字段
- **阶段5 集成验证（1-2天）**：计划任务 + dry-run + 交叉校验
- **阶段6 下线旧客户端**（稳定后）

## 七、风险与对策

| 风险 | 对策 |
|---|---|
| zhenganhuo 分类算法不准被风控 | 保守默认(guided_agent+低置信)起步，逐步调优 |
| device_token 7天过期 | auth.rs 自动检测，提示用户重新生成 install-token |
| codex 28GB 扫描慢 | 文件级增量缓存 |
| rustls 代理偶发 TLS 失败 | 进程级 NO_PROXY（不影响系统代理） |
| 开源/闭源边界模糊 | 独立工程不 import 主程序，主 exe 可审计零上传 |
