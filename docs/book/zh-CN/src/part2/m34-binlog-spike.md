# M34 — Binlog 复制探针

**Issue #57**

## 问题

外部复制工具使用 MySQL **binlog**，而非 JSON WAL。需要研究性探针，评估在不替换 `rusql.wal` 的前提下导出 binlog 事件是否可行。

## 决策

- 保持 JSON WAL 为持久化真相源。
- 增加 `binlog.rs` 探针：magic 头 + `FORMAT_DESCRIPTION_EVENT` + `QUERY_EVENT`。
- 更新 [adr-replication.md](../../../en/specs/adr-replication.md)，说明 M34 事件子集与明确非目标（GTID、行事件、校验和）。

## 权衡

探针可写文件，但未接入服务端主循环。完整复制仍延后；ADR 记录路径。

## Harness 经验

> **探针里程碑** 应落在 storage + 单元测试 + ADR 更新 — 不要半集成进生产路径。
