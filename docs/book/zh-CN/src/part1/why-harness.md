# 为何采用 Harness Engineering？

[Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) 把**编码者周围的环境**——人或 AI——当作产品来设计。目标不是更聪明的提示词，而是可靠的**前馈与反馈**，使小粒度变更能快速合并且不破坏 `main`。

## 前馈与反馈

| 方向 | 问题 | rusql 示例 |
|------|------|------------|
| **前馈** | 下一步做什么、边界在哪？ | `agent-ready` Issue、ADR、文件边界、HANDOFF |
| **反馈** | 改动是否真的可用？ | `cargo test`、clippy、compat JSON 夹具、CI |

前馈模糊（「做成 MySQL」）或反馈太慢（只靠手工 QA）时，严肃项目就会失控。rusql 的优化是：**一个里程碑 → 一个 Issue → 一个 PR**，每步都有传感器把关。

## 为何不能「只用 Copilot」？

没有 harness 的随意 AI 编码容易：

- 单次 diff 跨多层乱改
- 不写可验证的用户文档
- 每个会话重新争论决策（认证插件、SQL 解析器）

Harness Engineering 让**决策可沉淀**（ADR）、**范围可约束**（Issue 正文）、**质量可度量**（CI 一次通过率、compat 夹具）。

## 为何数据库适合展示 Harness

数据库容错低：线缆字节、SQL 语义、持久化必须一致；用户可用真实 `mysql` 客户端连接。这使 rusql 成为**可信**的 harness 案例——不是 Todo 应用——同时仍能按垂直切片交付。

## Harness 启示

> 选择**外部客户端**能提供免费反馈的领域（MySQL 线缆协议），并尽早投入**可执行规格**（compat JSON）。
