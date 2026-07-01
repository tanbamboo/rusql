# M4 — 二级索引

**合并**：PR #13 · Issue #12

## 问题

堆表上 `SELECT … WHERE col = 字面量` 为 O(n)。MySQL 用户期望索引点查。

## 设计选择

- crate 内单列 **B+Tree** 二级索引
- 执行器支持 `CREATE INDEX idx ON tbl (col)`
- 谓词匹配索引列时走 `scan_eq` 快路径

## 取舍

- 仅单列二级索引
- 无联合键、覆盖索引、优化器代价模型

## CI 注记

本 PR 首次出现跨平台 **rustfmt** 失败 —— 现为已知 harness 成本（本地先 fmt）。

## Harness 启示

> **B+Tree 单元测试** + 带 `WHERE` 的 **compat 夹具** —— 双层反馈。
