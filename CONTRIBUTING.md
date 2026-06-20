# 贡献指南 | Contributing

感谢你对 TokenStep 的兴趣！🎉 本仓库同时包含 **macOS 原生版**（`TokenStepSwift/`）和 **Windows 移植版**（`windows/`），下面主要针对 Windows 版说明（也是目前主要维护的方向）。

[English](#english) | 中文

## 报告问题

- 使用 [GitHub Issues](../../issues) 报告 Bug 或提出功能建议。
- 提 Bug 前，请先**搜索**是否已有相同 Issue。
- 提 Bug 时请使用 Issue 模板（自动加载），并尽量提供：系统版本、Codex / Claude Code 版本、复现步骤、相关日志路径。
- **安全漏洞**请勿开公开 Issue，参见 [SECURITY.md](SECURITY.md)。

## 开发环境（Windows 版）

要求：

- Windows 10/11 (x64)
- [Rust](https://rustup.rs/) stable，target `x86_64-pc-windows-msvc`
- MSVC 构建工具（Visual Studio 2022 Build Tools 的 "Desktop development with C++" 工作负载，提供 `link.exe` / `cl.exe`）
- WebView2 运行时（Win10/11 一般已预装）
- Tauri CLI（构建脚本会自动安装）

## 本地运行

```bat
cd windows\src-tauri
cargo install tauri-cli --version "^2.0"
cargo tauri dev
```

应用会启动在系统托盘。点击托盘图标打开仪表盘。

## 提交 Pull Request

1. Fork 本仓库，基于 `main` 创建分支：
   ```bat
   git checkout -b fix/short-description
   ```
2. 做出修改，保持每个 PR 聚焦于单一主题。
3. 确保本地能编译通过：
   ```bat
   cd windows\src-tauri
   cargo build --release
   ```
   （CI 也会在 PR 上自动跑一遍 Windows 构建。）
4. 如果改动涉及用户可见的行为变化，请更新相关文档（`windows/README.md`、`windows/docs/`）。
5. 提交 PR，描述清楚改动内容和动机，关联相关 Issue（如 `Closes #12`）。

### 提交信息规范

建议使用约定式提交（Conventional Commits）：

- `feat: 支持新的 Agent`
- `fix: 修复缓存命中 token 重复计入`
- `docs: 补充构建说明`
- `refactor: 重构 collector`

不强求，但清晰的提交信息有助于 review 和 changelog 整理。

## 代码风格

- Rust 代码遵循 `rustfmt` 默认风格（`cargo fmt`）。
- Clippy 无警告为佳（`cargo clippy`）。
- 与现有代码风格保持一致：命名、注释密度、错误处理方式。

## 数据与隐私

TokenStep 是 local-first 工具。任何改动都应保持"**不上传用户代码、prompt、对话正文**"这一原则。如果新增功能涉及网络请求，请在 PR 中明确说明并征得维护者同意。

## 行为准则

参与本项目的所有贡献者均需遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

---

## English

Thanks for your interest in TokenStep! This repo hosts both the **macOS native app** (`TokenStepSwift/`) and the **Windows port** (`windows/`); the notes below focus on the Windows port (the actively maintained one).

### Reporting issues

- Use [GitHub Issues](../../issues) for bugs and feature requests.
- **Search first** to avoid duplicates.
- Bug reports auto-load an issue template — please fill in OS version, Codex / Claude Code version, reproduction steps, and relevant log paths.
- **Do not** open public issues for security vulnerabilities — see [SECURITY.md](SECURITY.md).

### Dev setup (Windows)

Requires Windows 10/11 (x64), [Rust](https://rustup.rs/) stable (`x86_64-pc-windows-msvc`), MSVC build tools, WebView2, and the Tauri CLI.

### Run locally

```bat
cd windows\src-tauri
cargo install tauri-cli --version "^2.0"
cargo tauri dev
```

### Submitting a PR

1. Fork and branch off `main`.
2. Keep each PR focused on a single concern.
3. Make sure it compiles locally (`cargo build --release` in `windows\src-tauri`). CI will also build it on Windows automatically.
4. Update docs (`windows/README.md`, `windows/docs/`) for any user-facing change.
5. Open the PR with a clear description and link the related issue (e.g. `Closes #12`).

### Code style

Follow `rustfmt` defaults, keep clippy clean, and match the surrounding code's style.

### Data & privacy

TokenStep is local-first. Any change must preserve the principle of **never uploading user code, prompts, or conversation content**. If a new feature makes network requests, call it out explicitly in the PR.

### Code of Conduct

By participating you agree to abide by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
