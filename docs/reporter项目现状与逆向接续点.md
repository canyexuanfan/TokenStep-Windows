# reporter 项目现状与逆向接续点

> 更新：2026-08-01
> 状态：**阶段0-2完成，阶段3-4遇认证壁垒，逆向中（已暂停，待续）**
> 关联：`docs/PRD-双榜单上报组件.md`、`questions/reporter-采集器崩溃排查.md`、`questions/reporter-生财sink认证壁垒.md`

## 一、项目目标

在 TokenStep 仓库新增**独立闭源 Rust 组件** `reporter/`，自采原始日志，
把同一份数据分别按各自协议上传到**生财(scys.com) + zhenganhuo.com** 两个榜单，
最终替代 opentoken + token-rank 两个独立客户端。

## 二、已完成的阶段（✅ 可运行）

### 阶段0：工程骨架
- `reporter/` 独立 Cargo 工程（.gitignore，不污染开源主程序）
- CLI 全链路跑通：`status` / `scan` / `sync` / `setup-scys` / `setup-zhenganhuo` / `service`
- 配置（`%APPDATA%\TokenStep\config\reporter.json`）、状态、双 sink 框架

### 阶段1：采集器（4 完整 + 2 降级）
| 平台 | 状态 | 数据源 | 精度 |
|---|---|---|---|
| zcode | ✅ 完整 | `~/.zcode/cli/db/db.sqlite` model_usage | 准确 |
| claude-code | ✅ 完整 | `~/.claude/projects/**/*.jsonl` message.usage | 准确 |
| codex | ⚠️ 占位 | `~/.codex/state_5.sqlite` threads 表 | 偏高(tokens_used含cache累加，待校准) |
| minimax | ✅ 完整 | `~/.minimax/v2/sessions/**/ledger.jsonl` | 准确 |
| cursor | 降级 | 本机 tokenCount 全 0 | 无数据 |
| workbuddy | 降级 | 只有 credit 金额 | 无 token 数据 |

**架构亮点**：流式聚合（Aggregator），内存与事件量解耦（codex 161万事件→几百桶）。

### 阶段2：分类器
- `classify.rs` 实现 automation_score/usage_mode 启发式
- 已集成进 sync 流程，给每个 SessionAgg 打分
- 保守默认（guided_agent），CLASSIFICATION_VERSION=6

### 阶段3-4：双 sink payload 组装（✅ 组装完成，❌ 上报被拒）
- **生财 sink**：v1{rows,sessions} + v2{v2_hourly,v2_sessions} 组装✅，POST 被 403 拒
- **zhenganhuo sink**：19字段 hourly_model + 25字段 sessions 组装✅，POST 被 400 拒

## 三、认证壁垒（❌ 核心障碍，需逆向突破）

### 生财壁垒：x-gz3 / x-t41 混淆认证头
- **现象**：reporter 上报生财，无论 UA/payload 如何，全部 `403 client_version_blocked_upgrade_required`
- **根因**：opentoken 发请求时带两个自定义头 `x-gz3` 和 `x-t41`，值由运行时混淆算法计算。
  生财服务器靠这两个头识别"合法 opentoken 客户端"。
- **证据**：exe offset 确认（`x-gz3` @ 0x140203011，`x-t41` @ 0x140035e48）
- **算法**：未公开，需逆向

### zhenganhuo 壁垒：设备-客户端绑定校验
- **现象**：即使有正确 device_token（Bearer），ingest 返回
  `400 Token Rank client is outdated for this recalculated device`
- **根因**：zhenganhuo 不只校验 device_token，还校验"设备与注册时客户端的绑定关系"
- **register 壁垒**：install-register.php 报"缺少本机唯一标识"，
  试了 25+ 种字段名（device_id/machine_id/fingerprint/...）全部失败
- **算法**：未公开，需逆向

### 关键结论
**两个榜单官方都从协议层封锁了第三方客户端上报。** 这是有意设计的反第三方保护。

## 四、逆向工具链（✅ 已就绪）

### 已安装
- **Ghidra 11.4.2**（NSA 开源反汇编器，scoop 安装）：`~/scoop/apps/ghidra/current/`
- **JDK 25**（Temurin LTS）：`~/scoop/apps/temurin-lts-jdk/current/`
- **Python capstone 5.0.6 + pefile 2023.2.7**

### Ghidra 逆向脚本（保存在 `reporter/tools/reverse/`）
- `find_header_funcs.py`：扫描 LEA 指令找字符串引用 + fallback 找包含函数
- `decompile_header_refs.py`：找定义的字符串 + 反编译引用函数
- **注意**：Jython 2.7 只接受 ASCII，脚本里不能有中文（已修复）

### 运行 Ghidra headless 的命令
```bash
export PATH="/c/Users/wzm33/scoop/apps/temurin-lts-jdk/current/bin:$PATH"
GHIDRA="/c/Users/wzm33/scoop/apps/ghidra/current"
PROJ_DIR="/tmp/ghidra_proj"
mkdir -p "$PROJ_DIR"
"$GHIDRA/support/analyzeHeadless.bat" "$PROJ_DIR" proj_name \
    -import "C:\Users\wzm33\.opentoken\bin\opentoken.exe" \
    -overwrite \
    -scriptPath "F:\zcode\projects\tokenstep\TokenStep\reporter\tools\reverse" \
    -postscript "find_header_funcs.py" \
    -deleteProject
```

## 五、逆向接续点（下次从这里继续）

### 已定位的函数地址（opentoken.exe）
- `x-t41` → 在 `FUN_140035d50` 内（但这是底层字符串处理，非 header 构造）
- `x-gz3` → 在 `FUN_140201c70` 内（同上，是底层序列化）

### 下一步逆向思路（按优先级）
1. **找 reqwest 的 header 插入点**：不是找 x-gz3 字符串，而是找
   `HeaderName`/`HeaderMap`/`insert` 调用链。reqwest 设置 header 时会调
   `HeaderMap::insert`，定位它，看 x-gz3/x-t41 的值怎么算出来的。
2. **沿调用链回溯**：从 FUN_140035d50/FUN_140201c70 的调用者往上找，
   直到找到组装 HTTP 请求的函数（特征：同时引用 scys.com URL + header 名）。
3. **动态分析备选**：opentoken 没做证书钉扎，可用自签 CA 中间人抓真实请求
   （需授权导入 CA 到系统根证书库——用户当前未授权，可后续重议）。

### zhenganhuo 逆向思路
- "缺少本机唯一标识"字段名：在 token-rank.exe 的 install-register 附近，
  字段簇是 `event_usage install_token desktop headless platform`。
  本机唯一标识字段名可能在 `desktop`/`platform` 这组常量附近，需细查。
- 设备绑定校验：可能和 register 时上报的客户端指纹有关，需对比
  token-rank register 请求体的完整字段。

## 六、当前可运行的能力

reporter 现在能做（即使上报被封锁）：
- `scan`：准确采集 4 平台的 token 用量（与 opentoken 交叉校验吻合）
- `status`：探测各平台健康度 + 双榜单配置状态
- `sync --dry-run`：组装双榜单 payload 并打印（不上报）

不能做（被认证壁垒封锁）：
- 真实上报到任一榜单

## 七、待办（逆向恢复后）

1. 逆向 x-gz3/x-t41 算法 → 解锁生财 sink
2. 逆向 zhenganhuo register 字段名 + 设备绑定 → 解锁 zhenganhuo sink
3. 校准 codex 采集器精度（tokens_used 含 cache 累加问题）
4. 实现 minimax 的 model 字段（当前占位）
5. 阶段5：集成验证（计划任务 + 真实上报 + 交叉校验）
6. 阶段6：稳定后下线旧客户端

## 八、文件清单

- 工程根：`F:\zcode\projects\tokenstep\TokenStep\reporter\`
- 源码：`reporter\src\*.rs`（models/config/collect/classify/sinks/auth/state/main）
- 逆向工具：`reporter\tools\reverse\*.py`
- PRD：`docs\PRD-双榜单上报组件.md`
- 排查记录：`questions\reporter-采集器崩溃排查.md`、`questions\reporter-生财sink认证壁垒.md`
- 本文档：`docs\reporter项目现状与逆向接续点.md`
