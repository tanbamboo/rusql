# ADR：M190 实时 `SHOW STATUS`

**状态**：已接受（M190 / [#342](https://github.com/tanbamboo/rusql/issues/342)）  
**日期**：2026-10-27

## 背景

M86 的 `SHOW STATUS` 是一小份常量目录。运维和连接器会读 `Questions`、`Uptime` 和 `Threads_connected`。这三个值可以跟着本进程已经在做的事走：每次 `COM_QUERY`、进程启动时间，以及连接注册表。MySQL 8.0 其余状态变量（包括 `Com_*`）有几百个名字，不属于这一片。

本 ADR 不修改 `CONSTITUTION.md`。它不是完整的状态变量对齐，不是 `performance_schema.session_status` / `global_status`（它们仍是 M189 的 errno **1146**），也不是实时 InnoDB 状态（M193）。

## 决定

1. **三个实时名字。** 会话带有连接注册表时（服务器总会挂上）：
   - `Questions` 是进程启动以来的 `COM_QUERY` 包数量。语句执行前就加一，因此 `SHOW STATUS LIKE 'Questions'` 把自身算进去。再查一次至少多 1。
   - `Uptime` 是该注册表创建（进程启动）以来的整秒数。
   - `Threads_connected` 是已注册连接数。
2. **一份进程级列表。** `SHOW STATUS`、`SHOW SESSION STATUS` 和 `SHOW GLOBAL STATUS` 返回相同的名字和相同的值。这一片不单独维护会话级 `Questions`。
3. **目录里的其他名字仍是 M86 常量。** `Threads_running` 为 `1`。`Connections` 为 `1`。`Aborted_connects`、`Bytes_received`、`Bytes_sent`、`Open_tables`、`Slow_queries` 为 `0`。不增加 `Com_*`。`LIKE` 不变：匹配不到的模式（包括 `Com%`）返回零行。
4. **列名。** 仍是 `Variable_name` 和 `Value`。匹配不到的 `LIKE` 不是错误。
5. **没有注册表。** 没有进程列表的执行器会话保持 M86 默认值（`Questions` `0`、`Uptime` `0`、`Threads_connected` `1`）。
6. **`FLUSH STATUS` 仍是 errno 1235**（M187），不会把这些计数清零。

## 后果

- 两次 `COM_QUERY` 执行 `SHOW STATUS LIKE 'Questions'`，第二次的 `Questions` 更高。
- 墙钟走过至少一秒后，`Uptime` 增加。
- 另一条连接注册后，`Threads_connected` 改变。
- mysql-diff 比较 `SHOW STATUS` / `LIKE 'Questions'` / `LIKE 'Uptime'` 是否成功，不比较数值（MySQL 的目录更大，计数也不相同）。

## 范围外

- `Com_*` 以及其余 MySQL 8.0 状态变量
- 按会话与按全局分开的 `Questions`
- `FLUSH STATUS` 清零
- `performance_schema.session_status` / `global_status`
- 实时 InnoDB 状态（M193）和 `information_schema.INNODB_*`（M194）
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor show_status_live
cargo test -p rusql-server show_status_live
```

```sql
SHOW STATUS LIKE 'Questions';
SHOW SESSION STATUS LIKE 'Questions';
SHOW GLOBAL STATUS LIKE 'Uptime';
SHOW STATUS LIKE 'Threads_connected';
SHOW STATUS LIKE 'Slow_queries';
```

第二次的 `Questions` 大于第一次。`Slow_queries` 仍为 `0`。`SHOW STATUS LIKE 'Com%'` 返回零行。
