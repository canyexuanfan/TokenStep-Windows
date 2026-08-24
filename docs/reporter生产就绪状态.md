# reporter 生产就绪状态

> 更新：2026-08-02
> 状态：**生产就绪，自动上报运行中**

## 一、核心成就

**reporter 是一个自研的、开源 TokenStep 项目内的闭源上报组件，能用单一工具同时向生财(scys.com) 和 zhenganhuo.com 两个 Token 排行榜上报数据。**

两个壁垒都已突破：
- 生财：POST body 缺 `version`/`device` 字段（本地捕获法发现）
- zhenganhuo：数据要放 `snapshot.hourly_model` + `replace_scopes`（本地捕获法发现）

## 二、当前运行状态

### 计划任务（三个并存）
| 任务 | 系统 | 频率 | 状态 |
|---|---|---|---|
| OpenToken | 生财（opentoken 官方）| 每30分钟 | Ready |
| TokenRankSync | zhenganhuo（token-rank 官方）| 每30分钟 | Ready |
| **TokenStepReporter** | **双榜单（reporter 自研）** | 每30分钟 | **Ready** |

### reporter 安装位置
- 计划任务：`TokenStepReporter`
- exe：`C:\Users\wzm33\.tokenstep-reporter\tokenstep-reporter.exe`（4.4MB release）
- 启动器：`C:\Users\wzm33\.tokenstep-reporter\run-hidden.vbs`
- 配置：`%APPDATA%\TokenStep\config\reporter.json`

### 双榜单验证结果
- 生财：`status:0` 真实写入 ✅
- zhenganhuo：`accepted_model_rows:36, accepted_sessions:63` 真实写入 ✅

## 三、reporter 能力清单

| 能力 | 状态 |
|---|---|
| 采集 codex/claude-code/zcode/minimax | ✅（codex 精度待校准）|
| 采集 cursor/workbuddy | 降级（本机无 token 数据）|
| 生财 sink（v1+v2 payload）| ✅ 真实写入 |
| zhenganhuo sink（snapshot+replace_scopes）| ✅ 真实写入 |
| 分类器（automation_score/usage_mode）| ✅ 保守默认 |
| 计划任务自动上报（每30分钟）| ✅ |
| release 编译（4.4MB）| ✅ |
| service install/uninstall | ✅ |

## 四、能否删掉 opentoken/token-rank？

**技术上现在可以了**——reporter 已实现自动上报，能独立工作。
**但建议先观察几天**，确认 reporter 在多个 30 分钟周期稳定上报后，再删官方客户端。

删除方法（观察确认后执行）：
```bash
# 删 opentoken（生财）
schtasks /Delete /TN OpenToken /F
rm -rf ~/.opentoken

# 删 token-rank（zhenganhuo）
schtasks /Delete /TN TokenRankSync /F
rm -rf ~/.token-rank
```

⚠️ **重要**：reporter 的生财 sink 复用了 opentoken 的 `device_id`（从 `~/.opentoken/device_id` 读）。
删 opentoken 前，需要先把 device_id 复制到 reporter 能读的地方，或改 reporter 硬编码 device_id。

## 五、日常使用

reporter 全自动运行，无需干预。需要手动操作时：
```bash
cd ~/.tokenstep-reporter
./tokenstep-reporter.exe sync --days 1     # 手动触发一次上报
./tokenstep-reporter.exe scan --days 7     # 仅采集查看
./tokenstep-reporter.exe status            # 配置+健康检查
./tokenstep-reporter.exe service uninstall # 卸载计划任务
```

## 六、技术债务（可选优化）

1. **codex 采集器精度**：tokens_used 含 cache 重复累加，数据偏高（~47倍）。
   需逆向 codex threads.tokens_used 与 cache 的关系。
2. **reporter device_id 独立**：当前从 opentoken 目录读 device_id，
   删 opentoken 后需改成 reporter 自己生成/存储。
3. **zhenganhuo device_token 续期**：7天有效期，到期需重新 setup-zhenganhuo。
   reporter 现在不会自动续期。

## 七、相关文档
- 协议破解全记录：`questions/reporter-本地捕获法突破认证壁垒.md`
- 采集器排查：`questions/reporter-采集器崩溃排查.md`
- PRD：`docs/PRD-双榜单上报组件.md`
- 逆向接续点（历史）：`docs/reporter项目现状与逆向接续点.md`
