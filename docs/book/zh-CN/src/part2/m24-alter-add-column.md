# M24 — ALTER TABLE ADD COLUMN

**Issue #47**

## 问题

生产环境中表结构会演进：迁移脚本经常在不重建表的情况下加列。MySQL 客户端与 ORM 会执行 `ALTER TABLE … ADD COLUMN`（或简写 `ADD col type`）。缺少该能力则无法跑增量迁移。

## 设计

- 用 sqlparser MySQL 方言解析 `ALTER TABLE t ADD [COLUMN] c TYPE`。
- 存储层：在 `TableMeta` 追加列，已有行用空字符串填充（与 M21 的 NULL 表示一致）。
- WAL 记录 `AddColumn`，重启后可重放。
- DDL 后同步 session catalog，保证 DESCRIBE / `information_schema` 一致。

## 实现要点

1. **Executor** — `execute_alter_table` 处理 `AlterTableOperation::AddColumn`；复用 CREATE TABLE 的 `column_def_from_ast`。
2. **HeapEngine** — `add_column` 拒绝重复列名；扩展每行向量。
3. **PersistentEngine** — 先写 WAL 再改堆；新连接从 `table_metas()` 重建 catalog。

## Harness 经验

> `alter_add_column` 兼容用例同时校验新列 SELECT（NULL 为空串）与 DESCRIBE 列列表。

## 参考

- MySQL 8.0：[ALTER TABLE](https://dev.mysql.com/doc/refman/8.0/en/alter-table.html)
