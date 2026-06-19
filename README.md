# TokenStep

> A Windows system-tray app that tracks local AI coding agent token usage like daily steps.
>
> **[English](README.en.md)** | 中文

把 AI token 用量变成「每日 AI 步数」的本地小工具。本仓库是原 macOS 版 **TokenStep**（作者 [Chaoqiang Huang / 黄叔](https://github.com/Backtthefuture)）的 **Windows 移植版**，基于 **Tauri 2 + Rust** 构建，驻留在 Windows 系统托盘（任务栏右下角）。

它在本机读取 Codex / Claude Code 的 token 用量，并像运动圆环一样显示今天的 AI 使用进度——你可以把它理解成：

> 给 AI Agent 时代准备的「步数 App」。

> 🍎 如果你用的是 macOS，请前往[原仓库](https://github.com/Backtthefuture/TokenStep)获取原生菜单栏版本（源码也保留在本仓库的 [`TokenStepSwift/`](TokenStepSwift) 目录）。

## 它是做什么的？

TokenStep 只在本机统计 token 用量元数据，**不会上传你的代码、prompt 或对话正文**。

默认每日目标是 **1 亿 token**。它会显示：

- 今天用了多少 token、完成了目标的百分之几
- 最近若干天的趋势
- 历史记录（按天 / 按工具 / 按模型）
- 粗略的「消耗金额」估算（本地计算，不等于真实账单）

## 数据来源

| 工具 | 来源（Windows） |
|------|----------------|
| **Codex** | `~/.codex/sessions/**/*.jsonl`、`~/.codex/archived_sessions/*.jsonl`（主）；`~/.codex/state_5.sqlite` 的 `threads` 表（回退） |
| **Claude Code** | `~/.claude/projects/**/*.jsonl` |

统计口径：日志里若直接给了 `total_tokens` 就直接用；否则把 input / output / cache_creation / cache_read / reasoning 五项相加（缓存命中的 token 也会计入总数，因此总量可能偏高）。详见 [`windows/src-tauri/src/collector.rs`](windows/src-tauri/src/collector.rs)。

## 下载安装

### Windows（本项目）

1. 前往 [Releases 页面](https://github.com/canyexuanfan/TokenStep-Windows/releases) 下载最新的 `TokenStep_x.x.x_x64-setup.exe`。
2. 双击运行安装包（需要 WebView2 运行时，Win10/11 一般已预装，缺失时会自动下载）。
3. 安装完成后 TokenStep 会出现在系统托盘；点击托盘图标打开仪表盘，右键打开菜单。

> ⚠️ 目前安装包使用**自签名证书**签名，首次运行时 Windows SmartScreen 可能会弹出「未知发布者」警告。这是正常现象，点击「更多信息 → 仍要运行」即可。详见 [`windows/docs/SIGNING.md`](windows/docs/SIGNING.md)。

### macOS（原版）

请前往 [原作者仓库](https://github.com/Backtthefuture/TokenStep/releases/latest) 下载已签名公证的 DMG。

## 功能

- 系统托盘显示今日 token 数与进度圆环。
- 点击托盘图标打开仪表盘：今日 / 历史 / 统计 / 隐私 / 设置。
- 每日目标可设置，默认每天 1 亿 token。
- 自动刷新（默认 1 分钟）。
- 支持 Codex 与 Claude Code。
- **本地优先（local-first）**：所有数据只存在本机 `%APPDATA%\TokenStep\`，不联网上传内容。

## 隐私说明

- 只读取本机 usage 元数据（日期、模型、客户端名称、token 数量）。
- **默认不上传任何数据**，不读取 / 不上传代码、prompt、对话正文。
- 「消耗金额」只是基于本地定价表的粗略估算，不等于真实账单。

完整说明见 [`docs/PRIVACY.md`](docs/PRIVACY.md)（macOS 版隐私模型，Windows 版同样遵循）。

## 本地构建（Windows）

**环境要求：** Windows 10/11 (x64)、Rust (stable, `x86_64-pc-windows-msvc`) + MSVC 构建工具、Tauri CLI。

```bat
cd windows\src-tauri
cargo install tauri-cli --version "^2.0"
cargo tauri dev
```

打包发布版安装包：

```bat
windows\scripts\build-release.bat
```

产物位于 `windows\src-tauri\target\release\bundle\nsis\TokenStep_x.x.x_x64-setup.exe`。

更详细的构建说明、签名、数据目录等见 **[`windows/README.md`](windows/README.md)**。

## 本地构建（macOS，原版）

```bash
./script/build_and_run.sh --verify
```

详见 [原版说明](https://github.com/Backtthefuture/TokenStep) 与 [`docs/INSTALL.md`](docs/INSTALL.md)。

## 贡献

欢迎提 Issue 和 PR！请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。发现安全漏洞请按 [SECURITY.md](SECURITY.md) 的流程私下上报，**不要**直接开公开 Issue。

## 路线图

Windows 版的待办与已知限制（更多 Agent 支持、桌面通知、数据导出、自动启动等）见 [`windows/docs/ROADMAP.md`](windows/docs/ROADMAP.md)。

## 开源协议

MIT License。本仓库同时包含原作者的 macOS 实现（`TokenStepSwift/`）与本 Windows 移植（`windows/`）。详见 [LICENSE](LICENSE)。

## 致谢

- 原版 **TokenStep**（macOS）由 [**Chaoqiang Huang（黄叔）**](https://github.com/Backtthefuture) 开发，本 Windows 移植复用了其设计理念、定价表与 `usage.json` 数据格式。
- Windows 移植（Tauri 2 + Rust）由 [**十七°**](https://github.com/canyexuanfan) 维护。
