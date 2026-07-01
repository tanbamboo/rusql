# M5 — 兼容性夹具套件

**合并**：PR #15 · Issue #14

## 问题

单元测试只证明各 crate 孤立正确；需要**可执行证明**：MySQL 线缆路径能跑通真实 SQL 场景。

## 设计选择

- `crates/rusql-server/compat/basic.json` JSON 套件
- 跑在**真实 TCP** 上断言列、行、影响行数
- 用户指南中作为推荐回归路径

## 取舍

若不每个 SQL 里程碑追加步骤，夹具会落后 —— 现为合并清单一部分。

## 影响

项目最佳**反馈**投资（[回顾报告](../../../en/reports/harness-retrospective-2026-06-30.md) §6）。编码用户可测合同，Agent 可扩 JSON 而非复制 Rust 集成样板。

## Harness 启示

> 每个 SQL 能力优先 **数据驱动线缆测试**，少复制 Rust 集成样板。

## 验证

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```
