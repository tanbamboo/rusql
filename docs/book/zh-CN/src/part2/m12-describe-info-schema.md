# M12 — DESCRIBE 与 information_schema

**合并**：PR #30 · Issue #29 · [spec](../../../en/specs/m12-describe-info-schema.md)

## 问题

ORM 与 GUI 通过 **`DESCRIBE`**、**`SHOW COLUMNS`**、**`information_schema`** 自省。

## 设计选择

- 虚拟 `information_schema.tables` / `.columns`（无磁盘系统库）
- 固定 schema 名 `rusql`
- 列类型来自目录 Display 字符串（输出小写）
- 仅 columns 支持 `WHERE table_name = '…'`

## 取舍

最小列集 —— 非完整 MySQL information_schema；不支持查询计划 `EXPLAIN`（用 DESCRIBE）。

## Harness 启示

> 工具兼容里程碑应**复用目录真源** —— 不要在第二套存储里复制元数据。

## 延伸阅读

- [m12-describe-info-schema.md](../../../en/specs/m12-describe-info-schema.md)
