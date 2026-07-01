# M2 — COM_QUERY

**合并**：PR #9 · Issue #3、#4（ADR）

## 问题

握手后客户端以 **`COM_QUERY`** 发送 SQL。需要解析、目录、执行器与存储构成最小 DML 闭环。

## 设计选择

| 层 | 选择 |
|----|------|
| SQL | `sqlparser` MySQL 方言（[adr-sql-parser](../../../en/specs/adr-sql-parser.md)） |
| 目录 | `rusql-core` 内存 `TableMeta` |
| 存储 | `StorageEngine` trait + 堆引擎 |
| 执行 | 透传规划器、火山式执行器 |

支持：`CREATE TABLE`、`INSERT … VALUES`、`SELECT *`、`SELECT` 字面量。

## 取舍

- **无查询优化器** — MVP 可接受；planner crate 预留扩展。
- 列类型用解析器 **Display 字符串** —— 简化后续 DESCRIBE（M12）。

## 延后

索引、持久化、认证、预编译语句。

## Harness 启示

> M2 之前在 **ADR** 锁定解析器与 crate 边界，避免中途更换 `sqlparser`。

## 延伸阅读

- [adr-sql-parser.md](../../../en/specs/adr-sql-parser.md)
