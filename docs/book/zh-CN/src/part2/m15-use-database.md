# M15 — USE database

**合并**：（待合并）· Issue #36

## 问题

MySQL 客户端连接后常执行 `USE db` 选择默认 schema。

## 设计选择

- `Session.database` 字段（默认 `rusql`）
- 接受 `USE rusql`、`USE DATABASE rusql`、`USE SCHEMA rusql`
- 拒绝未知库名（单库 MVP）

## 取舍

尚无多库存储 —— `USE` 仅为会话状态，直至未来多库需求。

## Harness 启示

> 会话字段尽早放在 **rusql-core**；executor 写入，info_schema 读取。

## 延伸阅读

- [m15-use-database.md](../../../en/specs/m15-use-database.md)
