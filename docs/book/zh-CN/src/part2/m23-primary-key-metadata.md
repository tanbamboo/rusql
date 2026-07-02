# M23 — PRIMARY KEY 元数据

**Issue #46**

## 问题

ORM 与迁移脚本从 `DESCRIBE` / `information_schema` 读取键与可空性。catalog 无 `PRIMARY KEY`、`NOT NULL` 则无法校验 schema。

## 决策

- `ColumnDef` 增加 `nullable`、`primary_key`。
- 解析列级与表级 `PRIMARY KEY`、`NOT NULL`。
- 反映到 DESCRIBE、`SHOW CREATE TABLE`、`information_schema.columns`。

## Harness 启示

> `primary_key_metadata` compat 同时锁定 DESCRIBE 与 SHOW CREATE 输出。
