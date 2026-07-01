# M16 — SELECT LIMIT

**Issue #38**

## 问题

ORM 与 API 用 `LIMIT` 分页；没有它则每次查询返回全表。

## 设计选择

- 从 sqlparser 读取 `Query.limit`
- 在扫描、投影、过滤**之后** `take(n)`（简单 MVP）

## 取舍

无 `OFFSET`、无优化器下推 —— 小堆表可接受。

## Harness 启示

> 分页测试应放在 **compat JSON** 的 `basic_dml` 插入步骤旁。
