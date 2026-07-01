# M15 — USE database

**合并**：PR #37 · Issue #36

## 问题

MySQL 客户端假定每个会话有**当前数据库**（schema）。元数据查询（`information_schema`、`SHOW TABLES`）与未限定表名都依赖它。没有 `USE`，ORM 与 GUI 工具无法复现生产连接串行为。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| `Session.database` 字符串 | 状态小；info_schema 易用 | 须贯穿 executor |
| 永远隐式单 schema | 无需 `USE` | 破坏客户端兼容 |
| 完整多库 catalog | 贴近生产 | MVP 范围过大 |

## 决策

- 在 `rusql-core` 保存 `session.database`（默认 `rusql`）。
- 接受 `USE rusql`；未知库名报错。
- `information_schema` 与 `SHOW TABLES` 列名使用会话 schema。

## 内部流程

```
USE rusql → session.database = "rusql"
information_schema.tables → TABLE_SCHEMA = session.database
```

**说明：** 当前 sqlparser MySQL 方言未解析 `USE DATABASE name`；使用 `USE name` 的客户端可正常工作。

## 取舍

现阶段仅逻辑库 `rusql`；多租户 catalog 见路线图元数据阶段（M27–M28）。

## 延伸阅读

- MySQL 8.0：[USE](https://dev.mysql.com/doc/refman/8.0/en/use.html)
- Gray & Reuter，《Transaction Processing》

## Harness 启示

> 会话状态变更需要 **executor 单测 + compat 中 `USE` 步骤**；握手不会自动设置数据库。
