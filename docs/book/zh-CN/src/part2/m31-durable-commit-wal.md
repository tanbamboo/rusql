# M31 — COMMIT 持久化 WAL

**Issue #54**

## 问题

M9 引入按连接的覆盖层与延迟写入，但运维需要 **已提交** 的数据在 `mysqld` 重启后仍在 — 与 M3 自提交 DML 的期望一致。

## 决策

- `COMMIT` 调用 `PersistentEngine::commit_transaction`，将每条待写 `WalRecord` 追加到 WAL（`sync_data`）并应用到共享堆。
- `ROLLBACK` 丢弃覆盖层，不修改 WAL 文件。
- 存储层测试验证 `PersistentEngine::open` 重放后提交仍在；wire 测试验证 `COMMIT`/`ROLLBACK` 的 WAL 语义及提交后重开。

## Harness 经验

> 若行为已在较早里程碑实现，下一里程碑以 **验收测试 + 文档** 交付，使回归在 CI 中可见。
