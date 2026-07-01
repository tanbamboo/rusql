# M3 — WAL 持久化

**合并**：PR #11 · Issue #10

## 问题

内存堆在重启后丢失数据。用户期望像数据库一样通过 `--data-dir` 持久化。

## 设计选择

- 可配置目录下追加写 **WAL**（`rusql.wal`）
- 启动时重放到堆与目录
- WAL 记录中目录快照需 `ColumnDef` 序列化

## 取舍

- **WAL 骨架**，非完整 ARIES —— 足以重放 CREATE/INSERT
- 事务延至 M9（覆盖层而非 WAL 级事务）

## 事件（反馈）

并行测试共享临时目录 → `persistence_across_connections` 不稳定。改为每测试独立数据目录 —— **反馈发现的是测试 harness 问题**。

## Harness 启示

> 持久化里程碑需要**重启集成测试**；并行测试须隔离临时路径。
