# M27 — information_schema 扩展

**Issue #50**

## 问题

ORM 与 `SHOW INDEX` 需要 `information_schema.SCHEMATA` 和 `STATISTICS`，不能只有 M12 的 `tables` / `columns`。

## 设计

- `SCHEMATA`：单行 `rusql`，utf8mb4 默认字符集/排序规则。
- `STATISTICS`：catalog 主键元数据生成 `PRIMARY` 行 + 存储层 `index_metas()` 的二级索引。
- 存储 trait 新增 `index_metas()`，事务下可见性一致。

## Harness 经验

> `information_schema_schemata_statistics` 兼容用例锁定 SCHEMATA 行与 PK/二级索引 STATISTICS 行。
