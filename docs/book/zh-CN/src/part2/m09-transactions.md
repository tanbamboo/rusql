# M9 — 事务

**合并**：PR #22 · Issue #19 · [spec](../../../en/specs/m9-transactions.md)

## 问题

并发连接需要未提交工作的**隔离**；用户期望 `BEGIN` / `COMMIT` / `ROLLBACK`。

## 设计选择

- 存储引擎上每连接**事务覆盖层**
- 未提交写入对其他连接不可见
- `COMMIT` 刷 WAL；`ROLLBACK` 丢弃覆盖层

## 取舍

非完整 MVCC —— 单写者覆盖 MVP；足以验证线缆语义并控制 harness 范围。

## Harness 启示

> 语义再大仍可**一个里程碑**交付 —— 前提是预先定义文件边界与 compat 测试。

## 延伸阅读

- [m9-transactions.md](../../../en/specs/m9-transactions.md)
