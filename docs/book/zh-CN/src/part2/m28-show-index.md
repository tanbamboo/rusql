# M28 — SHOW INDEX

**Issue #51**

## 问题

MySQL 客户端与迁移工具常用 `SHOW INDEX FROM tbl` 查看索引。M27 已提供 `information_schema.STATISTICS`，但许多驱动仍发送经典 SHOW 命令。

## 决策

- 在 `rusql-sql` 中将 `SHOW INDEX` / `SHOW INDEXES` / `SHOW KEYS` 重写为内部虚拟表查询（sqlparser 0.53 无 SHOW INDEX AST）。
- 执行器通过 `__rusql_show_index` 返回 MySQL 列名：`Table`、`Non_unique`、`Key_name`、`Seq_in_index`、`Column_name`、`Index_type`。
- 行数据来自 catalog 的 `PRIMARY KEY` 元数据及 `StorageEngine::index_metas()` 中的二级索引。

## Harness 经验

> `show_index` 兼容套件验证 `SHOW INDEX` 与 `SHOW INDEXES` 对主键与二级索引的输出。
