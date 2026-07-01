# M5 — 兼容性夹具套件

**合并**：PR #15 · Issue #14

## 问题

单元测试只证明各 crate 孤立正确，但 **MySQL 线缆边界** 仍会回归：握手成功、`COM_QUERY` 帧错误、列数不匹配。需要真实 TCP 客户端看到正确表格式结果的可执行证明。

手工 `mysql` CLI 无法在 CI 与自主 Agent 中扩展。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| JSON 线缆夹具 | 声明式；Agent 可扩 JSON | 须与功能同步 |
| 仅 Rust 集成测试 | 类型安全 | 每场景重复样板 |
| 外部 mysqltest | MySQL 原生 | 依赖重 |

## 决策

- `crates/rusql-server/compat/basic.json` JSON 套件
- 在**真实 TCP** 上断言列、行、影响行数
- 用户指南作为推荐回归路径
- 每个 SQL 里程碑尽量追加步骤而非新建套件

## 内部流程

```
basic.json → compat runner → TCP → 握手 → 逐步 COM_QUERY → 对照 JSON expect
```

套件按场景名隔离；同连接内步骤顺序执行（会话状态延续）。

## 取舍

若不每个里程碑追加步骤，夹具会落后 —— 现为 issue-loop 合并清单一部分。

## 影响

项目最佳**反馈**投资（[回顾报告](../../../en/reports/harness-retrospective-2026-06-30.md) §6）。

## 延伸阅读

- MySQL Internals：[客户端/服务端协议](https://dev.mysql.com/doc/internals/en/client-server-protocol.html)
- Harness 回顾 — 前馈与反馈环

## Harness 启示

> 每个 SQL 能力优先 **数据驱动线缆测试**，少复制 Rust 集成样板。

## 验证

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```
