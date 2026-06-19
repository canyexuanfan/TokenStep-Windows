# 安全策略 | Security Policy

中文 | [English](#english)

## 报告漏洞

如果你发现 TokenStep 存在安全漏洞，**请不要在公开 Issue 中提交**，请改为私下上报，以免漏洞在被修复前被利用。

请通过以下任一方式联系维护者：

- 私下联系仓库维护者 **十七°**（通过 GitHub 的 Security Advisory）
- 发送邮件到维护者邮箱（请在仓库主页查看维护者联系方式）

上报时请尽量包含：

- 漏洞的清晰描述与影响范围
- 复现步骤（最小可复现示例最佳）
- 受影响的版本
- 你期望的修复方式（可选）

我们会在收到报告后尽快确认，并在修复后致谢上报者（除非你希望匿名）。

## 报告范围

**欢迎上报**：可导致数据泄露、本地文件越权访问、签名绕过、供应链风险等的安全问题。

**不在范围内**：自签名证书导致的 SmartScreen 警告（这是预期行为，见 [`windows/docs/SIGNING.md`](windows/docs/SIGNING.md)）；功能建议与普通 Bug（请走 [Issues](../../issues)）。

## 支持版本

安全修复只针对**最新的发布版本**。请始终使用 [Releases](../../releases/latest) 中的最新版。

---

## English

### Reporting a vulnerability

If you discover a security vulnerability in TokenStep, **please do not open a public issue**. Report it privately so it isn't exploited before a fix is available.

Contact the maintainer **十七°** via:

- GitHub's Security Advisory feature on this repo
- Maintainer email (see the repo profile for contact details)

Please include:

- A clear description of the issue and its impact
- Reproduction steps (a minimal repro is ideal)
- Affected version
- Suggested fix (optional)

We will acknowledge receipt promptly and credit reporters after a fix (unless you prefer to remain anonymous).

### Scope

**In scope**: issues that could cause data disclosure, unauthorized local file access, signature bypass, or supply-chain risk.

**Out of scope**: SmartScreen warnings caused by the self-signed certificate (this is expected — see [`windows/docs/SIGNING.md`](windows/docs/SIGNING.md)); feature requests and ordinary bugs (use [Issues](../../issues)).

### Supported versions

Security fixes target the **latest release** only. Always run the newest version from [Releases](../../releases/latest).
