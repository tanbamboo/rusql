# M0 — Harness 引导

**合并**：引导提交，Issue #1

## 问题

从第一天起用 **AI 原生工作流** 启动 Rust 版 MySQL 兼容数据库 —— 而非事后补 harness。

## 设计选择

| 主题 | 选择 | 未采纳 |
|------|------|--------|
| 仓库布局 | Cargo workspace + 分层 crate | 单体二进制 |
| 治理 | CONSTITUTION、AGENTS、issue-loop 规则 | 仅 README |
| Profile | `profiles/rust/sensors.yaml` | 复制粘贴 CI 命令 |
| i18n | 尽早引入 `rusql-i18n` | 仅英文硬编码 |

## 取舍

空仓库时 harness 文件显得重 —— 但定义了后续每个里程碑**如何交付**。

## 延后

Hello-world 以外的 SQL；内存之外的存储。

## Harness 启示

> 在 M0 建好**流程工件**（传感器、Issue 模板、HANDOFF），Agent 不必再问「测试在哪」。

## 延伸阅读

- [架构概览](../../../en/architecture/overview.md)
- [spec-to-ship 工作流](../../../en/workflows/spec-to-ship.md)
