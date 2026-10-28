# ADR：M191 慢查询日志

**状态**：已接受（M191 / [#343](https://github.com/tanbamboo/rusql/issues/343)）  
**日期**：2026-10-28

## 背景

运维需要一份超过阈值的语句文件。MySQL 8.0 用 `slow_query_log`、`slow_query_log_file` 和 `long_query_time`，以及 `--slow-query-log`、`--slow-query-log-file`。日志默认关闭。阈值为 `0` 时记录每一条符合条件的语句。

这一片会写入该文件。不修改 `CONSTITUTION.md`。不是通用查询日志（M192），不是 `performance_schema` 摘要（M189），也不是实时的 `Slow_queries` 状态计数（该名字仍是 M190 的常量 `0`）。

## 决定

1. **默认关闭。** 不传开关且 `slow_query_log` 保持 `0` 时，rusql 不创建、不追加文件。`--slow-query-log` 在启动时打开。`--slow-query-log-file=PATH` 设置路径（默认 `{data-dir}/rusql-slow.log`）。`--long-query-time=SECONDS` 设置全局阈值（默认 `10`，显示为 `10.000000`）。启动时为负数或非有限值，进程以 `sql.wrong_value_for_var` 的文案停止。日志在启动时已启用但路径打不开，进程以 `server.slow_query_log_open_failed` 停止。

2. **变量。**
   - `long_query_time` 同时是会话和全局。`SET SESSION`、`SET @@session`、`SET @@` 和裸 `SET` 写入会话值。会话尚未设置时，`@@long_query_time` 跟随全局值，包括本连接上的 `SET GLOBAL`。会话值设置之后，`SET GLOBAL` 不会覆盖它。`SELECT` 显示六位小数（`0.000000`）。超出 `[0, 31536000]` 的数值会被夹到该区间后保存（MySQL 同样返回 OK；rusql 不发出警告 1292，因为 `SHOW WARNINGS` 仍为空）。非数值是 errno **1231**。`--long-query-time` 在启动时仍拒绝负数或非有限值。
   - `slow_query_log` 和 `slow_query_log_file` 仅全局。`SET GLOBAL` / `SET @@global` 更新它们。`SET SESSION` 是 errno **1229**。`SELECT @@session.slow_query_log`（以及文件变量）是 errno **1238**。`@@slow_query_log` 为 `0` 或 `1`（不是 `OFF` / `ON`）。空路径是 errno **1231**。
   - `SHOW VARIABLES` 列出这三个名字。会话列表里的 `long_query_time` 是会话值。另外两个在会话和全局列表里都显示全局值。

3. **写入内容。** 一条 `COM_QUERY` 结束后，若日志已启用且耗时不少于有效的 `long_query_time`（按微秒比较，因此 `0` 会记下每个非空包），rusql 追加一条记录。比较使用该包执行之后的阈值，所以日志已经打开时，`SET SESSION long_query_time = 0` 自身也会被记下。一个多语句包只写一条，内容是整个包的文本。`COM_STMT_EXECUTE`、`COM_CHANGE_USER` 和其他命令不是记录。只有分隔符的包不是记录。

4. **文件行（子集）。** 没有启动横幅，没有 `Lock_time`、`Rows_sent`、`Rows_examined`：

   ```
   # Time: 2026-10-28T00:00:00.000Z
   # User@Host: root @ 127.0.0.1
   # Query_time: 0.000012
   SELECT 1
   ```

   `Time` 是带毫秒的 UTC。`Query_time` 是六位小数的秒。语句压成一行。

5. **密码。** 不写 `COM_CHANGE_USER`，因此它的密码包不会进入文件。`COM_QUERY` 文本里，与 M185 审计日志相同的字面量（`IDENTIFIED BY`、`IDENTIFIED BY PASSWORD`、`IDENTIFIED WITH … BY|AS`、`SET PASSWORD … =`、`PASSWORD(`）变成 `'<redacted>'`。这个记号在所有语言环境下都是文件格式的一部分。

6. **写失败不改变客户端结果。** 磁盘满时服务器记一条警告。语句仍返回原来的 errno。

## 后果

- `SHOW STATUS` 里的 `Slow_queries` 仍为 `0`。
- 打开日志不会立刻建文件，直到真的有语句被记下。
- `mysql-diff` 的 `slow_query_log` 比较 `@@long_query_time`、`@@slow_query_log`、会话 `SET` 和仅全局的 errno。它不读文件，也不在参照服务器上 `SET GLOBAL`。文件检查是 `cargo test -p rusql-server slow_query_log`。
- `performance_schema.session_variables` / `global_variables` 会带上这三个名字，因为它们复用 `SHOW VARIABLES`。

## 范围外

- 通用查询日志（M192）和 `--log-error`（M195）
- `log_queries_not_using_indexes`、`min_examined_row_limit`、`log_slow_extra`、`log_throttle_queries_not_using_indexes`
- 微秒级附加列（`Lock_time`、`Rows_sent`、`Rows_examined`）
- 记录 `COM_STMT_EXECUTE` 或 `COM_CHANGE_USER`
- 实时 `Slow_queries` 计数
- `performance_schema` 语句摘要
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor slow_query_log
cargo test -p rusql-server slow_query_log
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --slow-query-log --long-query-time 0
```

```sql
SET GLOBAL slow_query_log = ON;
SET SESSION long_query_time = 0;
SELECT 1;
```

`{data-dir}/rusql-slow.log` 含有 `SELECT 1`。不传 `--slow-query-log` 且不执行 `SET GLOBAL slow_query_log = ON` 时不会创建文件。
