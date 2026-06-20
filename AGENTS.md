# 项目记忆 / Project Memory — TokenStep Windows Port

> 本文件是 ZCode / AI 助手在**本仓库工作时的强制约束**。
> 任何改动都必须遵守这里的规则。如有冲突，以本文件为准。
> 最后更新：2026-06-21

## 项目背景

- **TokenStep Windows 版** 是原作者 **黄叔（Chaoqiang Huang / Backtthefuture）**
  的 macOS TokenStep 的 Windows 移植版，作者 **十七°（canyexuanfan）**。
- 技术栈：**Tauri 2 + Rust**（非标准前端构建，用静态 HTML/CSS/JS）。
- 上游仓库：`https://github.com/Backtthefuture/TokenStep`（remote 名 `upstream`）。
- 本仓库：`https://github.com/canyexuanfan/TokenStep-Windows`（remote 名 `origin`）。

---

## ⚠️ 强制规则：macOS 源码处理（不可违反）

这是**最重要的规则**，来源于 v0.1.0 上传 GitHub 时的明确决策（提交 `1d91dac`）。

### ✅ 必须做的
1. **保留** `TokenStepSwift/` 和 `TokenUsageMenuApp/` 两个 macOS 源码目录在仓库里，
   作为**把 macOS 新功能移植到 Windows 版的参考素材**。
2. **定期同步** 上游 `TokenStepSwift/` 的新提交到本仓库，命令：
   ```
   git fetch upstream
   git checkout upstream/main -- TokenStepSwift/
   git commit -m "sync: 上游 TokenStepSwift 源码同步"
   ```
   （**注意**：本仓库没有 `upstream/main` 的合并权限，只能用 `checkout` 单目录同步，
   不能 `git merge upstream/main`，因为上游没有 `windows/` 目录。）
3. macOS 源码里值得移植的功能，要在 `windows/` 目录里**重新实现为 Rust + HTML/JS**，
   不要直接编译 Swift。

### ❌ 绝对禁止的
1. **绝不**添加任何能从本仓库**构建出 macOS 安装包（.dmg / .app）的 CI 工作流**。
   - 已删除：`ci.yml`、`release.yml`（这些是 fork 自带的 macOS 构建/签名 CI）。
   - 原因：① 本仓库没有 Apple 签名密钥，构建会失败；
          ② 如果在本仓库生成 DMG，会**误导用户**下载到错误平台的包；
          ③ 这会**侵犯/干扰原作者**的分发权益。
   - 唯一允许的 CI：`ci-windows.yml`（仅 Windows 编译检查，无密钥）。
2. **绝不**修改 `TokenStepSwift/` 里原作者的代码内容（只能整体从上游 checkout 同步）。
3. **绝不**在本仓库发布 macOS 版本的 Release。

### 为什么这样设计（一句话）
> macOS 源码是**只读的功能移植素材**，不是可分发的产品。
> 用户只能从原作者的仓库获取 macOS 版；本仓库只产出 Windows 版。

---

## ⚠️ 强制规则：功能同步的工作流

当上游 macOS 版本新增功能时，按这个流程同步到 Windows 版：

1. **审计**：对照上游新提交，判断每个功能在 Windows 版的状态
   （已移植 / 未移植 / macOS 专属）。
2. **移植**：把跨平台功能用 Rust + HTML/JS 在 `windows/` 里重新实现，
   对齐上游的行为和设置项（字段名、默认值要一致）。
3. **macOS 专属功能跳过**：Token Island（刘海）、DMG、Helper 进程等
   无 Windows 等价物的，**不移植**，但源码同步保留。
4. **实测**：每移植一个功能，都要 `cargo check` + 构建验证。

---

## ⚠️ 强制规则：不要轻信 AI 截图分析

历史教训：AI 视觉工具曾多次误读截图（把 0.1.1 读成 0.1.2、把崩坏的 UI 说成正常）。
- 当**用户说有问题**时，以用户为准，不要用"我截图看着正常"来反驳。
- 截图分析只能作为**辅助参考**，不能作为"没问题"的证据。

---

## 发版流程（已自动化）

1. 改 `windows/src-tauri/tauri.conf.json` 的 `version`（**唯一版本来源**）。
2. 运行 `windows/scripts/build-release.bat`（自动 build + 签名 + 部署带版本号的产物）。
3. 产物命名约定（见 `windows/docs/PACKAGING.md`）：
   - `TokenStep.exe`（不带版本号，开发用）
   - `TokenStep_v<ver>.exe`（双击版，分发用）
   - `TokenStep_<ver>_x64-setup.exe`（NSIS 安装版）
4. 用户手动上传 exe 到 GitHub Release（`gh` CLI 通常未登录）。

---

## 当前版本：v0.1.1

已移植的跨平台功能：
- Codex 配额卡片 + **显示开关**（`show_codex_quota`）
- 多语言（简中/英/繁中）+ 托盘菜单跟随语言
- 自动更新（下载 + NSIS 静默安装 + bat 守护重启）
- 截图（今日页 renderDataCard + 其他页 DOM 截图）
- 主题系统（5 套配色）

macOS 专属、未移植（源码已同步保留）：Token Island、DMG、Helper 进程。
