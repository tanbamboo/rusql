# M21 — IS NULL / IS NOT NULL

**Issue #44**

## 问题

可空列是 SQL 基础。没有 `IS NULL` 就无法过滤缺失值；`= NULL` 在三值逻辑中不为真。

## 决策

- `INSERT … NULL` 以空字符串作为 NULL 哨兵（MVP）。
- `WHERE col IS NULL` / `IS NOT NULL` 接入 M20 谓词树。

## Harness 启示

> compat 中 `INSERT NULL` + `IS NULL` 成对出现，确保存储与过滤一致。
