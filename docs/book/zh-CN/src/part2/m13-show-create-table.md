# M13 — SHOW CREATE TABLE

**合并**：PR #32 · Issue #31 · [spec](../../../en/specs/m13-show-create-table.md)

## 问题

 schema 导出与部分迁移依赖 **`SHOW CREATE TABLE`** 的 DDL 字符串。

## 设计选择

- 由目录元数据重建 `CREATE TABLE`
- 反引号标识符；DDL 中类型大写
- 仅表（无视图）

## 取舍

无引擎子句、字符集、`IF NOT EXISTS` —— 可读性优先于 mysqldump 级保真。

## Harness 启示

> **SHOW CREATE** 与断言精确 DDL 的 compat 夹具成对 —— 可抓目录/类型回归。

## 延伸阅读

- [m13-show-create-table.md](../../../en/specs/m13-show-create-table.md)
