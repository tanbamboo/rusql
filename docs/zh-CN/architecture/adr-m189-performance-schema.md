# ADR：M189 `performance_schema` 存根

**状态**：已接受（M189 / [#341](https://github.com/tanbamboo/rusql/issues/341)）  
**日期**：2026-10-27

## 背景

客户端会探测 `performance_schema`（`session_variables`、`accounts`、语句摘要、`threads`）。这些名字目前不是表，语句是 errno **1146**。MySQL 8.0 的 performance schema 是一套很大的运行时探测。把每个 consumer 都打开，或把摘要行写入 WAL，不属于这一片，也会在本服务器并不收集的计数器上消耗 CPU。

本 ADR 是可查询的存根。它不修改 `CONSTITUTION.md`。它不是完整的 instrument 对齐，不是实时 `SHOW STATUS`（M190），也不是 `information_schema.INNODB_*`（M194）。

## 决定

1. **虚拟表，不是落盘对象。** 对下列名字的限定名 `SELECT` 读取内存存根。目录和 WAL 都不写这些表。`INSERT`、`UPDATE`、`DELETE` 仍然看不到基表（errno **1146**）。如果已经创建了相同存储键的基表，则使用该基表，而不是存根。`USE performance_schema` 不是已存储的数据库。`SHOW DATABASES` 不列出它。

2. **已实现的表。** 字母大小写不敏感（`PERFORMANCE_SCHEMA.SESSION_VARIABLES` 是同一张表）。

   | 表 | 行 | 列（`SELECT *` 顺序） |
   |----|----|------------------------|
   | `session_variables` | 现有的 `SHOW SESSION VARIABLES` 存根。会话 `SET` 覆盖生效。 | `VARIABLE_NAME` `varchar(64)`，`VARIABLE_VALUE` `varchar(1024)` |
   | `global_variables` | 同样的名字，不含会话覆盖（全局默认值，含 `@@global.gtid_executed`）。 | 同样两列 |
   | `accounts` | 零行。计数器列不递增。 | `USER` `char(32)`，`HOST` `char(255)`，`CURRENT_CONNECTIONS` `bigint unsigned`，`TOTAL_CONNECTIONS` `bigint unsigned` |
   | `events_statements_summary_by_digest` | 零行。这是 `events_statements_digest` 探测。不收集摘要，也不持久化。 | `SCHEMA_NAME`、`DIGEST`、`DIGEST_TEXT`、`COUNT_STAR`、`SUM_ERRORS`、`SUM_WARNINGS`、`SUM_ROWS_AFFECTED`、`SUM_ROWS_SENT`、`SUM_ROWS_EXAMINED`、`FIRST_SEEN`、`LAST_SEEN` |
   | `threads` | 零行。不是活动线程列表。 | `THREAD_ID`、`NAME`、`TYPE`、`PROCESSLIST_ID`、`PROCESSLIST_USER`、`PROCESSLIST_HOST`、`PROCESSLIST_DB`、`PROCESSLIST_COMMAND`、`PROCESSLIST_INFO`、`INSTRUMENTED` |

   对这些表的 `SELECT *` 成功，返回零行或多行。不是 errno **1146**。

3. **该库中的其他名字是 errno 1146。** 包括 `events_waits_current`、`setup_instruments`、`setup_consumers`、`session_status` 和 `global_status`。consumer 保持关闭。没有用来打开 instrument 的 setup 表。`session_status` / `global_status` 属于 M190（`SHOW STATUS`），不是这个存根。

4. **未知列。** 不在该表列清单中的名字是 errno **1054**。变量表只是已文档化的 `SHOW VARIABLES` 存根，不是完整的 MySQL 8.0 目录。

5. **权限。** `root` 可以 `SELECT`。其他账号需要该表上的 `SELECT`（或更宽的授权）。否则语句是 errno **1142**。

## 后果

- 客户端可以 `SELECT` 上面五张表，而不会得到 errno 1146。`threads` 是空存根，不是静默的 1146。
- `accounts` 和 `events_statements_summary_by_digest` 上的计数器列存在，但因为 instrument 关闭，所以保持零行。
- `mysql-diff` 比较两边都为空的列名和过滤条件，以及 `VARIABLE_NAME = 'autocommit'`。它不把完整变量清单和 MySQL 逐行对比。

## 不在范围内

- 完整的 Performance Schema instrument、consumer 和 history 表
- 持久化摘要或线程行
- 默认打开全部 instrument
- 实时 `SHOW STATUS`（M190）以及 `performance_schema.session_status` / `global_status`
- InnoDB mutex 数据（M193）和 `information_schema.INNODB_*`（M194）
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor performance_schema
cargo test -p rusql-server performance_schema
```

```sql
SELECT VARIABLE_NAME, VARIABLE_VALUE
  FROM performance_schema.session_variables
  WHERE VARIABLE_NAME = 'autocommit';
SELECT * FROM performance_schema.accounts;
SELECT * FROM performance_schema.events_statements_summary_by_digest;
SELECT THREAD_ID FROM performance_schema.threads;
```

在 `SET @@autocommit = 0` 之前，`autocommit` 为 `1`。该 `SET` 只改变 `session_variables`。`accounts`、摘要表和 `threads` 返回列和零行。`SELECT * FROM performance_schema.events_waits_current` 是 errno **1146**。
