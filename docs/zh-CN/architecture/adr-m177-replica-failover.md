# ADR：M177 副本故障转移（手动提升）

**状态**：已接受（M177 / [#329](https://github.com/tanbamboo/rusql/issues/329)）  
**日期**：2026-10-26

## 背景

MySQL 8.0 异步复制的故障转移是运维步骤，不是 Group Replication、InnoDB Cluster，也不是外部编排器。rusql 已能写 binlog 并转储（M71–M175）。`CHANGE MASTER` / `START SLAVE`（M179）以及持久化的 `@@gtid_executed`（M176）尚未进入 `main`。

本 ADR 是 M177 实现的故障转移子集。未列出的行为不在范围内。

## 决策

1. **不自动故障转移。** 启动、崩溃或 dump 都不会提升。没有编排器，也没有 Group Replication（M199）。

2. **默认可写。** 没有 `{data_dir}/replica-role.json` 的数据目录接受写入（主库）。

3. **隔离写入。** `SET GLOBAL super_read_only = ON` 同时打开 `super_read_only` 与 `read_only` 并落盘。`SET GLOBAL read_only = ON` 只打开 `read_only`。

4. **提升。** `SET GLOBAL read_only = OFF` 会同时清掉两个开关（与 MySQL 一致）并把可写角色落盘。这就是提升命令。`SET GLOBAL super_read_only = OFF` 只关闭 `super_read_only`，`read_only` 保持打开。

5. **谁可以写。**
   - `super_read_only` 打开：包括 SUPER（`root`）在内的所有账号得到 errno **1290**，选项文本为 `--super-read-only`。
   - 仅 `read_only` 打开：非 SUPER 账号得到 errno **1290**，选项文本为 `--read-only`。SUPER 仍可写。
   - `SELECT` 及其他只读语句仍然允许。

6. **作用域错误。** `read_only` 与 `super_read_only` 仅 GLOBAL。`SET read_only = …` 或 `SET SESSION` 为 errno **1229**。除 `0`/`1`/`ON`/`OFF`（以及常见布尔别名，`DEFAULT` 表示 OFF）以外的值为 errno **1231**。

7. **不会悄悄脑裂。** 提升只改执行该语句的进程自己的数据目录。另一个目录若已隔离则保持隔离。重启不会自动打开或清掉开关。rusql **不会**连接旧主库，也 **不会**把它设成只读。操作者必须先隔离旧主库，再提升副本。两个目录都可写是操作错误，rusql 不会掩盖它。

8. **手动步骤**（本子集实际测试的是 `SET GLOBAL`；复制线程命令不在此子集）：

   ```sql
   -- 先在旧主库上停止应用写入：
   SET GLOBAL super_read_only = ON;

   -- 在副本进程上，确认已经停止回放之后（本里程碑没有 SQL 线程）：
   SET GLOBAL read_only = OFF;
   ```

   位点请在进程外比较（`SHOW MASTER STATUS` 的文件与位置，以及 `{data_dir}/gtid.json`）。本里程碑不持久化 `@@gtid_executed`（M176），也不实现 `STOP SLAVE` / `RESET SLAVE` / `CHANGE MASTER`（M179）。

## 后果

- 复制已提升的数据目录会连同可写标志一起复制。除非再隔离，否则该副本就是第二个主库。
- 角色禁止该用户写入时，存储程序语句、账号 DDL、`LOCK TABLES`、`XA PREPARE`/`XA COMMIT`，以及带待提交 DML 的 `COMMIT` 都会以 errno 1290 拒绝。只读的 `CALL` 也会被拒绝，比 MySQL 更严。
- 不是延迟复制，不是半同步（M178），不是 InnoDB Cluster。

## 不在范围内

- Group Replication 与自动选主
- `STOP SLAVE` / `RESET SLAVE` / `START REPLICA SQL_THREAD`（M179）
- GTID 自动定位与 `@@gtid_executed` 握手（M176）
