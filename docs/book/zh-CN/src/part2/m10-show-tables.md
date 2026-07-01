# M10 — SHOW TABLES

**合并**：PR #25 · Issue #24

## 问题

工具与人通过 **`SHOW TABLES`**、**`SHOW DATABASES`** 发现 schema，而非只靠 `SELECT`。

## 设计选择

- MySQL 风格列名：`Tables_in_rusql`、`Database`
- 单一逻辑库 `rusql`（多库延后）
- 执行器基于引擎目录实现

## 取舍

无 `SHOW TABLES LIKE`；尚无 `information_schema`（M12）。

## Harness 启示

> 元数据命令是**高性价比兼容** —— 用户感知强、存储风险低。
