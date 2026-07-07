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

**MySQL 8.0 CLI**（Issue #73）：协商 `CLIENT_QUERY_ATTRIBUTES` 时，客户端在 SQL 前发送 WL#12542 属性块；rusql 解析前剥离。协商 `CLIENT_DEPRECATE_EOF` 时，结果集以 OK 包结束（非传统 EOF），且列定义与行数据之间有 metadata EOF/OK。协商 `CLIENT_SESSION_TRACK` 时 OK 包含 session-state 尾。

## 取舍

- **无查询优化器** — MVP 可接受；planner crate 预留扩展。
- 列类型用解析器 **Display 字符串** —— 简化后续 DESCRIBE（M12）。

## 延后

索引、持久化、认证、预编译语句。

## Harness 启示

> M2 之前在 **ADR** 锁定解析器与 crate 边界，避免中途更换 `sqlparser`。

## 延伸阅读

- [adr-sql-parser.md](../../../en/specs/adr-sql-parser.md)
