# ADR：M193 实时 `SHOW ENGINE INNODB STATUS`

**状态**：已接受（M193 / [#345](https://github.com/tanbamboo/rusql/issues/345)）  
**日期**：2026-10-28

## 背景

M131 让 `SHOW ENGINE INNODB STATUS` 返回一行，客户端不再收到“不支持的语句”。当时 `Status` 是一句固定文本。运维和部分监控希望这段文本能写出当前持有事务的连接，以及该事务持有的锁。

rusql 不是 Oracle InnoDB。它有显式事务（`BEGIN`，以及在没有事务时由 `SAVEPOINT` 打开的事务），也有 `SELECT … FOR UPDATE` 和冲突 DML 使用的行锁 / 间隙锁。这一片报告的是这份 rusql 状态。不编造互斥量计数，也不编造缓冲池数字。不修改 `CONSTITUTION.md`。`information_schema.INNODB_*` 仍是 M194。

## 决定

1. **列保持 M131 的形状。** `Type`、`Name`、`Status`。`Type` 为 `InnoDB`。`Name` 为空。未知引擎名（包括 `SHOW ENGINE MUSQL STATUS`）仍是 errno **1286**。`SHOW ENGINES` 不变。仍未实现 `SHOW ENGINE … MUTEX`。

2. **`InnoDB` 是兼容名。** 引擎不会切换，文本也不是 Oracle InnoDB monitor 转储。只检查语句成功、并且头部像 monitor 的客户端仍能看到：

   ```
   =====================================
   INNODB MONITOR OUTPUT
   =====================================
   ```

3. **`TRANSACTIONS` 列出显式 rusql 事务。** `BEGIN` / `START TRANSACTION`，以及在没有事务时打开事务的 `SAVEPOINT`，会登记该连接。`COMMIT`、`ROLLBACK`、`COM_RESET_CONNECTION` 和断开连接会删掉它。每一组是：

   ```
   ---TRANSACTION {trx_id}, ACTIVE
   MySQL thread id {connection_id}
   ```

   `trx_id` 是从 1 开始的进程级计数。同一连接再次 begin 保持原来的 id。自动提交的语句结束后不留在列表里。不列出 XA。

4. **`LOCKS` 列出这些事务持有的行锁和间隙锁。** 行锁是 `RECORD LOCK table {table} trx id {trx_id} lock_mode X pk {pk}`。间隙锁是 `GAP LOCK table {table} trx id {trx_id} lock_mode X low {low} high {high}`（开区间边界写成 `-`）。锁在获取时记下，在该连接的行锁被释放或事务结束时删掉。没有信号量小节，也没有互斥量计数。

5. **格式标记在所有语言环境中保持英文。** `INNODB MONITOR OUTPUT`、`TRANSACTIONS`、`LOCKS`、`---TRANSACTION`、`MySQL thread id`、`RECORD LOCK`、`GAP LOCK` 和 `lock_mode X` 是客户端要找的 monitor 文本。它们不是需要翻译的错误。errno **1286** 仍走 `rusql-i18n`。

6. **没有缓冲池，也不是每一节 InnoDB 文本。** 文本里没有 `BUFFER POOL` 或 `SEMAPHORES`。文件 I/O、日志、插入缓冲和行操作历史都省略。mysql-diff 的 `show_engine_innodb_status` 仍使用 `compare_output: false`，因为这段文本不是 Oracle 的转储。

## 后果

- 执行过 `BEGIN` 和 `SELECT … FOR UPDATE` 的连接，会在 `Status` 里看到自己的连接 id、事务 id 和那一行的锁。`COMMIT` 之后这些行消失。
- `COM_CHANGE_USER` 仍然不会回滚 rusql 事务。它会释放行锁，因此该连接的锁行会消失；若事务仍开着，事务行还在。
- 没有事务的服务器仍返回头部和两个小节标题，没有 `---TRANSACTION` 行。

## 不在范围内

- 缓冲池、页和互斥量统计
- 与每一节 InnoDB monitor 文本逐段兼容
- `information_schema.INNODB_TRX` / `INNODB_LOCKS`（M194）
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor innodb_status_live
cargo test -p rusql-server show_engine_innodb_status
```

```sql
BEGIN;
SELECT id FROM t WHERE id = 1 FOR UPDATE;
SHOW ENGINE INNODB STATUS;
COMMIT;
```

事务打开时，`Status` 包含该连接的 `MySQL thread id` 以及该行的 `RECORD LOCK`。`COMMIT` 之后这些行消失。`SHOW ENGINE MUSQL STATUS` 为 errno 1286。
