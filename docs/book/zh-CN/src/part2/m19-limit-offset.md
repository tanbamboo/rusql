# M19 — SELECT LIMIT OFFSET

**Issue #42**

## 问题

分页 API 同时使用 `LIMIT` 与 `OFFSET`（第二页 = 跳过 10 条再取 10 条）。仅 M16 的 `LIMIT` 无法表达客户端发给 MySQL 的页码语义。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| 排序后 skip 再 take | MySQL 语义正确 | 需物化完整排序结果 |
| 键集分页 | 大规模高效 | SQL 表面不同 |
| 存储层 OFFSET | 可提前终止 | MVP 堆表复杂 |

## 决策

- 从 sqlparser 读取 `Query.offset`。
- 管线：过滤 → 投影 → **ORDER BY** → **OFFSET skip** → **LIMIT take**。
- 仅支持整数字面量 offset。

## 内部机制

```rust
apply_pagination(rows, offset, limit)
```

`information_schema` 扫描经 `finish_rows_query` 共用。

## 取舍

大 offset 为 O(n)，MVP 可接受。

## Harness 启示

> compat 中使用 `ORDER BY id LIMIT 1 OFFSET 1` 覆盖排序与分页组合。
