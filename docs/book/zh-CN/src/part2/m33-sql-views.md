# M33 — SQL 视图

**Issue #56**

## 问题

客户端期望 `CREATE VIEW v AS SELECT …` 与 `SELECT * FROM v` 提供只读间接层。无视图时，兼容性报告与 information_schema 探测会早期失败。

## 决策

- 在会话 catalog 中存储视图定义（名称 + SQL 文本）— MVP 不落盘。
- `SELECT FROM view` 重新解析并执行存储的查询。
- 提供 `information_schema.VIEWS` 桩，并在 `information_schema.tables` 中标记 VIEW。

## 权衡

视图目前与会话绑定，重启后不保留。这与 catalog MVP 一致，且不改变 WAL。

## Harness 经验

> **Catalog 优先** 的特性（视图、info_schema）可在持久化元数据之前交付，只要 wire 测试与 compat JSON 定义了“完成”。
