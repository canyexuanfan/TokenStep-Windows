# TokenStep

把 AI token 用量做成“今日 AI 步数”的本地 macOS 菜单栏 App。

第一版只统计本机数据，不上传任何内容。默认每日目标是 1 亿 token，自动刷新 1 分钟一次，成本字段显示为“消耗金额”。

## 当前支持

- Codex：读取 `~/.codex/sessions`、`~/.codex/archived_sessions` 中的 `token_count` 事件。
- Claude Code：读取 `~/.claude/projects/**/*.jsonl` 中的 usage 元数据。
- Gemini：只读取 `~/.gemini/tmp/**/chats/session-*.json` 中明确的聊天 token 统计。

不会读取或输出代码、prompt、对话正文。生成结果只包含日期、工具、模型、token 数量和粗略成本估算。

## macOS App

```bash
./script/build_and_run.sh
```

构建后会生成：

```text
TokenStepSwift/dist/TokenStep.app
```

App 提供：

- 菜单栏进度环 + 今日 token 数。
- 浮层：今日 AI 步数、目标进度、消耗金额、30 天趋势。
- 客户端窗口：今日、历史、统计、隐私。
- 设置：每日目标、自动刷新频率（1/5/15 分钟/手动）、登录后自动启动。

首次运行 SwiftUI 版会默认开启开机自启动；之后完全跟随设置里的开关。

## 数据刷新

```bash
python3 token_usage_monitor.py collect
```

菜单栏 App 运行时会按 `config/settings.json` 的设置自动刷新。当前默认是 1 分钟。

也可以继续使用 launchd 后台任务：

```bash
./install-launchd.sh
```

停止后台刷新：

```bash
./uninstall-launchd.sh
```

## 价格估算

价格在 `config/pricing.json` 里配置。默认值只是粗略估算，用来做趋势对比，不应当作为真实账单。
