# M35 — utf8mb4 字符集元数据

**Issue #58**

## 问题

MySQL 8.0 客户端与 ORM 从握手与 `information_schema` 读取字符集/排序规则。元数据错误或缺失会在 SQL 已 UTF-8 安全时仍引发驱动问题。

## 决策

- 握手 charset 字节 **45**（`utf8mb4`）。
- `information_schema.columns` 增加 `COLUMN_COLLATION`（`utf8mb4_unicode_ci`）。
- 列定义包在 wire 上使用 utf8mb4 charset id。

## Harness 经验

> **元数据里程碑** 是低成本的兼容收益 — 在完整 collation 引擎之前先修握手与虚拟表。
