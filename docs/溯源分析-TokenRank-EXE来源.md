# Token Rank EXE 溯源分析报告

> 调查日期：2026-07-31
> 调查对象：zhenganhuo.com 下载的 `token-rank.exe`
> 调查目的：判定网站 Windows 版的来源——是上游 Mac 原版移植？是本项目（TokenStep for Windows）挪用？还是第三方独立实现？

## 一、结论（TL;DR）

**网站上的 `token-rank.exe` 不是你的 TokenStep，也不是上游黄叔的 TokenStep。它是一个独立的、第三方用 Rust 重写的 CLI 程序，作者就是 zhenganhuo.com 站方本人。**

三方技术栈完全不同，代码层面没有任何同源关系：

| 维度 | 上游 Mac 版（黄叔） | 你的 Windows 移植版 | 网站下载的 token-rank.exe |
|---|---|---|---|
| 仓库 | `Backtthefuture/TokenStep` | `canyexuanfan/TokenStep-Windows` | 闭源，仅随网站分发 |
| 语言/框架 | **Swift** + Python 脚本 | **Tauri 2**（Rust 后端 + HTML/JS 前端） | **纯 Rust CLI**（无 GUI） |
| 编译产物体积 | — | 7.8 MB（exe）/ 3 MB（setup） | **2.4 MB** |
| 形态 | macOS 菜单栏 App | Windows 桌面 App（托盘+仪表盘） | 命令行 + 后台计划任务 |
| 数据流向 | 本地为主 | **纯本地，不上传** | **每 30 分钟 POST 到 zhenganhuo.com** |
| Cursor accessToken | 不碰 | 不碰 | **读取并直调 `api2.cursor.sh`** |
| 签名 | — | 自签名 | **未签名** |
| 同步机制 | launchd | 无（本地） | **schtasks 计划任务** |

体积差异本身就是铁证：如果网站把你的 Tauri App 换皮打包，体积应在 7-8 MB 量级，不可能是 2.4 MB——Tauri 内嵌的 WebView2 适配层和前端资源不可能被压缩掉 70%。

## 二、关键证据（来自 EXE 字符串逆向）

### 2.1 编译环境——确认是 Rust + GitHub Actions

字符串里残留大量 Rust 编译路径，全部指向 `runneradmin`（GitHub Actions Windows runner 的默认用户）：

```
C:\Users\runneradmin\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\rusqlite-0.31.0\src\raw_statement.rs
C:\Users\runneradmin\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\ring-0.17.14\src\rsa\padding\pkcs1.rs
/rustc/8bab26f4f68e0e26f0bb7960be334d5b520ea452/library\core\src\...
```

- `8bab26f4f68e0e26f0bb7960be334d5b520ea452` 是 **rustc 1.83 系列** 的 commit hash
- `runneradmin` = GitHub Actions 的 Windows runner（站方用 CI 编译，非本地手搓）

### 2.2 用到的 Rust crate（20 个，典型精简 CLI 栈）

```
rusqlite 0.31.0      SQLite（读 Cursor state.vscdb）
clap_builder 4.6.0   命令行参数解析
reqwest/rustls 0.23  HTTPS（拉 Cursor API、上报 zhenganhuo）
ring 0.17.14         加密/签名
serde_json 1.0       JSON
chrono 0.4           时间
anstyle/anstream     终端彩色输出
anyhow               错误处理
... 共 20 个
```

**注意：没有任何 GUI / WebView crate**（无 tauri、无 wry、无 webview2-com、无 iced、无 egui）。这进一步排除"你的 Tauri App 被挪用"的可能——Tauri 程序的字符串里必然出现 `tauri`、`wry`、`WebView2` 等字样，这里一个都没有。

### 2.3 网络端点（确认数据流向）

```
https://api2.cursor.sh/aiserver.v1.DashboardService/   ← 直调 Cursor 官方 API
https://zhenganhuo.com                                  ← 上报目的地
/api/token-rank/ingest.php                              ← 数据上报接口
/api/token-rank/sync-plan.php                           ← 同步计划
/api/token-rank/sync-status.php                         ← 同步状态
/api/token-rank/install-register.php                    ← 安装注册
/api/token-rank/me.php?range=                           ← 个人榜单页
```

### 2.4 行为特征字符串（与描述不符的高风险点）

```
accessToken                              ← 取 Cursor 登录凭证
state.vscdb                              ← 读 Cursor 本地数据库
.claude / .codex / .hermes / .workbuddy ← 扫多个 AI 工具本地数据
device_id / fingerprint / automation_score ← 设备指纹 + 自动化检测分
schtasks / TokenRankSync / SC MINUTE     ← 每 30 分钟计划任务
"Token Rank setup complete. Connected as ..."
"Background sync installed"
```

### 2.5 install.ps1 反审计手法

网站分发的 `install.ps1` **故意混淆为逐字符 ASCII 码流**（如 `112\n97\n114…` 表示 `par`），解码后仅 1483 字节，逻辑：

```powershell
# 伪代码（解码后）
mkdir ~/.token-rank/bin/
Invoke-WebRequest .../token-rank.exe → token-rank.exe
& token-rank.exe setup --site $Site --install-token $Token --days 30
```

这种混淆本身就是反审计信号——正常软件的安装脚本不需要对自身做字符级编码。

## 三、来源判定

综合以上：

1. **不是你的 TokenStep for Windows** —— 技术栈（Tauri vs 纯 Rust CLI）、体积（7.8MB vs 2.4MB）、行为（本地 vs 上传 + 取凭证）全部不符。你也没有任何 POST 到 zhenganhuo 或读 Cursor accessToken 的逻辑。
2. **不是上游黄叔的 TokenStep** —— 上游是 Swift + Python，从未涉足 Rust，且仓库里没有任何 `token-rank`、`zhenganhuo`、`api2.cursor.sh` 相关内容。上游是纯本地统计 + 公开榜单 GET。
3. **是 zhenganhuo.com 站方独立开发的闭源 Rust CLI** —— 作者用 GitHub Actions（runneradmin）编译，借用"Token Rank"这个产品名做了个数据采集 + 排行榜服务，安装脚本刻意混淆反审计，运行时会取 Cursor 登录凭证并每 30 分钟回传。

## 四、与你的产品定位对比（可作为差异化材料）

|  | TokenStep for Windows（你） | Token Rank（网站） |
|---|---|---|
| 开源 | ✅ MIT | ❌ 闭源 |
| 签名 | ✅ 自签名 | ❌ 未签名 |
| Cursor 凭证 | ✅ 不碰 | ❌ 读取 accessToken |
| 数据上传 | ✅ 纯本地 | ❌ 每 30 分钟 POST |
| 设备指纹 | ✅ 不采集 | ❌ fingerprint + automation_score |
| 后台持久化 | ✅ 无强制计划任务 | ❌ 强装 schtasks |
| 安装脚本 | ✅ 明文可审计 | ❌ 逐字符混淆 |
| 静默更新 | ✅ 无 | ❌ 有，且无签名校验 |

## 五、附：物证清单

- 待分析 EXE：`F:\zcode\projects\token-rank.exe`（2,547,712 bytes）
  - SHA256：`827054DD758877DD4FA94F04B8B2A8F91A4F62AE0192050F16661E3E098F5A85`
- 你的 Windows 版：`F:\zcode\projects\tokenstep\TokenStep\TokenStep.exe`（7,869,944 bytes）
  - SHA256：见项目 Release
- 上游仓库：https://github.com/Backtthefuture/TokenStep （Swift + Python）
- 你的仓库：https://github.com/canyexuanfan/TokenStep-Windows （Tauri 2）
- 审计辅助脚本：`F:\zcode\projects\_tr_forensics.ps1`、`_tr_crates.ps1`（及上轮的 `_tr_*.ps1`）
- 全量字符串转储：`%TEMP%\tr_strings_all.txt`

## 六、待办 / 后续

- [ ] 可选：把 SHA256 提交 VirusTotal 做第三方交叉确认
- [ ] 可选：清理 `F:\zcode\projects\` 下的 `token-rank.exe` 和 `_tr_*.ps1` 辅助脚本
- [ ] 可选：在你项目 README/PRIVACY.md 里补充与本报告呼应的隐私对比段落
