# ADR：M194 `information_schema.INNODB_*` 存根

**状态**：已接受（M194 / [#347](https://github.com/tanbamboo/rusql/issues/347)）  
**日期**：2026-10-28

## 背景

监控会查询 `information_schema.INNODB_TRX` 和 `information_schema.INNODB_LOCKS`。这些名字目前不是表，语句是 errno **1146**。rusql 不是 Oracle InnoDB。它已经为 `SHOW ENGINE INNODB STATUS`（M193）记录显式事务及其行锁 / 间隙锁。这一片把同一份注册表暴露成两张虚拟表。不创建表空间（M198），也不修改 `CONSTITUTION.md`。

MySQL 8.0 仍有 `INNODB_TRX`。它移除了 `INNODB_LOCKS`（替代品是 `performance_schema.data_locks`，在这里仍是 errno **1146**）。议题要求这两个监控名，所以 rusql 保留 `INNODB_LOCKS`。

## 决策

1. **虚拟表，不是存储对象。** 对下面两个名字做带模式限定的 `SELECT` 会读取打开事务注册表。不写入目录，也不写入 WAL。`INSERT`、`UPDATE`、`DELETE` 仍然看不到基表（errno **1146**）。若已经创建了相同存储键的真实基表，则使用该基表而不是存根。字母大小写无关。

2. **已实现的表。**

   | 表 | 行 | 列（`SELECT *` 顺序） |
   |----|----|------------------------|
   | `INNODB_TRX` | 每个显式 rusql 事务一行（`BEGIN`，或因此打开事务的 `SAVEPOINT`）。`COMMIT`、`ROLLBACK`、重置连接和断开连接会去掉它。自动提交语句不会留在列表里。 | `TRX_ID`、`TRX_STATE`、`TRX_STARTED`、`TRX_REQUESTED_LOCK_ID`、`TRX_WAIT_STARTED`、`TRX_MYSQL_THREAD_ID`、`TRX_QUERY`、`TRX_ROWS_LOCKED`、`TRX_ROWS_MODIFIED`、`TRX_ISOLATION_LEVEL` |
   | `INNODB_LOCKS` | 这些事务持有的每个行锁或间隙锁一行。没有锁时为零行。 | `LOCK_ID`、`LOCK_TRX_ID`、`LOCK_MODE`、`LOCK_TYPE`、`LOCK_TABLE`、`LOCK_INDEX`、`LOCK_SPACE`、`LOCK_PAGE`、`LOCK_REC`、`LOCK_DATA` |

   `SELECT *` 成功，返回零行或多行。不是 errno **1146**。

3. **每个单元格是什么。** `TRX_ID` 与 `LOCK_TRX_ID` 是 rusql 监听器本地的十进制数，与 `SHOW ENGINE INNODB STATUS` 里的 `---TRANSACTION {id}` 相同。不是 InnoDB 事务号。`TRX_STATE` 为 `RUNNING`（被阻塞的事务不会报成 `LOCK WAIT`）。`TRX_MYSQL_THREAD_ID` 是连接 id。`TRX_ROWS_LOCKED` 统计行锁笔记。`TRX_ROWS_MODIFIED` 恒为 `0`。`TRX_STARTED`、`TRX_REQUESTED_LOCK_ID`、`TRX_WAIT_STARTED`、`TRX_QUERY`、`TRX_ISOLATION_LEVEL` 为 SQL NULL。不为等待编造锁 id。

   `LOCK_ID` 为 `rusql:{trx_id}:{ordinal}`，`ordinal` 从 0 起，按笔记顺序。该字符串不是 InnoDB 的 `trx:space:page:heap`。`LOCK_MODE` 对行锁为 `X`，对间隙锁为 `X,GAP`。`LOCK_TYPE` 为 `RECORD`。`LOCK_TABLE` 是锁笔记上的表名（没有模式名）。`LOCK_DATA` 对行锁是主键文本，对间隙锁是 `{low}/{high}`（开区间边界为 `-`）。`LOCK_INDEX`、`LOCK_SPACE`、`LOCK_PAGE`、`LOCK_REC` 为 SQL NULL。不编造页号、空间号和堆号。

4. **其余 `INNODB_*` 名字都是 errno 1146。** 包括 `INNODB_LOCK_WAITS`、`INNODB_METRICS`、`INNODB_BUFFER_PAGE`、`INNODB_TABLES`，以及所有 `INNODB_SYS_*`。这一片不增加 `SYS_*` 表。

5. **未知列。** 不在该表列清单中的名字，包括 `TRX_WEIGHT`，是 errno **1054**。

6. **权限。** `root` 可以 `SELECT`。其他账户需要该表上的 `SELECT`（或更宽的授权）。否则语句是 errno **1142**。这不是 MySQL 的 `PROCESS` 过滤。

7. **mysql-diff。** `INNODB_TRX` 用两边都为空的过滤比较（`TRX_ID = '___none___'`），再加上 `INNODB_LOCK_WAITS` 与 `INNODB_SYS_TABLES`（两边都是 errno **1146**）。`INNODB_LOCKS` 不进入 mysql-diff，因为 MySQL 8.0 对该名字返回 **1146**，而 rusql 不返回。

## 后果

- 客户端可以 `SELECT` `INNODB_TRX` 和 `INNODB_LOCKS` 而不得到 errno 1146。空闲服务器返回零行。
- 锁 id 不能按 InnoDB 锁 id 解析。需要 `space` / `page` / `heap` 的工具在这些列上看到 NULL。
- `INNODB_LOCKS` 查询成功是与 MySQL 8.0 的已文档化差异。

## 不在范围内

- MySQL 8.0 其余 `INNODB_*` 目录（`INNODB_METRICS`、缓冲池表、`INNODB_TABLES` 等）
- `INNODB_SYS_*` 表
- `performance_schema.data_locks`（仍是 errno **1146**）
- InnoDB 表空间与崩溃恢复（M198）
- 编造的互斥量、缓冲池或锁内存计数
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor innodb_trx
cargo test -p rusql-server innodb_trx
```

```sql
BEGIN;
SELECT id FROM t WHERE id = 1 FOR UPDATE;
SELECT TRX_ID, TRX_STATE, TRX_MYSQL_THREAD_ID FROM information_schema.INNODB_TRX;
SELECT LOCK_ID, LOCK_MODE, LOCK_DATA FROM information_schema.INNODB_LOCKS;
COMMIT;
```

事务打开时，`INNODB_TRX` 有 `RUNNING` 和该连接 id，`INNODB_LOCKS`.`LOCK_ID` 以 `rusql:` 开头。`COMMIT` 之后这些行消失。`SELECT * FROM information_schema.INNODB_SYS_TABLES` 为 errno **1146**。`SELECT TRX_WEIGHT FROM information_schema.INNODB_TRX` 为 errno **1054**。
