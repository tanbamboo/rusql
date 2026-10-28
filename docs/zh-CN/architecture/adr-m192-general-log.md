# ADR：M192 通用查询日志

**状态**：已接受（M192 / [#344](https://github.com/tanbamboo/rusql/issues/344)）  
**日期**：2026-10-28

## 背景

运维需要一份可选的连接与语句文件。MySQL 8.0 用 `general_log` 和 `general_log_file`，以及 `--general-log`、`--general-log-file`。日志默认关闭。这一片会写入该文件。不修改 `CONSTITUTION.md`。不是慢查询日志（M191），不是 M185 的审计 JSON 文件，也不是 `log_output = TABLE`。

## 决定

1. **默认关闭。** 不传开关且 `general_log` 保持 `0` 时，rusql 不创建、不追加文件。`--general-log` 在启动时打开。`--general-log-file=PATH` 设置路径（默认 `{data-dir}/rusql-general.log`），单独设置路径不会打开日志。日志在启动时已启用但路径打不开，进程以 `server.general_log_open_failed` 停止。空路径同样停止。

2. **变量。** `general_log` 和 `general_log_file` 仅全局。`SET GLOBAL` / `SET @@global` 更新它们。`SET SESSION` 是 errno **1229**。`SELECT @@session.general_log`（以及文件变量）是 errno **1238**。`@@general_log` 为 `0` 或 `1`（不是 `OFF` / `ON`）。`SET GLOBAL` 接受 `ON` / `OFF` / `1` / `0`。空路径或含控制字符的路径是 errno **1231**。`SHOW VARIABLES` 列出这两个名字。会话列表和全局列表都显示全局值。

3. **打开时写入什么。**
   - 登录成功后写一行 `Connect`（`user@host`，客户端选了库时再加 ` on {database}`）。
   - 一条 `COM_QUERY` 完成后写一行 `Query`，内容是该包的文本。判断用的是该包执行之后的开关，因此 `SET GLOBAL general_log = ON` 自己也会被记下。多语句包只写一行。SQL 错误仍会记下。只有分隔符的空包不写。
   - `COM_QUIT` 写一行 `Quit`。连接突然断开没有这一行。
   - `COM_STMT_EXECUTE`、`COM_CHANGE_USER` 和其他命令不是记录。

4. **文件行（子集）。** 没有启动横幅，也不写结果行。字段用制表符分隔：

   ```
   2026-10-28T00:00:00.000Z	1	Connect	root@127.0.0.1 on rusql
   2026-10-28T00:00:00.000Z	1	Query	SELECT 1
   2026-10-28T00:00:00.000Z	1	Quit
   ```

   时间是带毫秒的 UTC。语句压成一行。`Connect`、`Query`、`Quit` 在所有语言环境里都是文件格式记号。

5. **密码。** 与 M185 审计日志和 M191 慢查询日志相同的字面量（`IDENTIFIED BY`、`IDENTIFIED BY PASSWORD`、`IDENTIFIED WITH … BY|AS`、`SET PASSWORD … =`、`PASSWORD(`）变成 `'<redacted>'`。该记号在所有语言环境里都是文件格式的一部分。不写 `COM_CHANGE_USER`，因此它的密码包不会进入文件。只出现在结果行里的值不会被写入。

6. **写失败不改变客户端结果。** 磁盘满时服务器记一条警告。语句仍返回原来的 errno。这个文件不是 M185 的 JSON 审计日志。默认 CI 不要求打开该日志；mysql-diff 不会在参照服务器上执行 `SET GLOBAL general_log = ON`。

## 后果

- 慢查询日志不变。一条语句可以只在其中一个文件、两个都在，或都不在。
- `mysql-diff` 的 `general_log` 比较 `@@general_log` 和仅全局的 errno。它不读文件，也不比较 `@@general_log_file`（MySQL 的默认路径是主机名）。文件检查是 `cargo test -p rusql-server general_log`。
- `performance_schema.session_variables` / `global_variables` 包含这两个名字，因为它们复用 `SHOW VARIABLES`。

## 范围外

- `log_output = TABLE` / `mysql.general_log`
- 记录结果集、`COM_STMT_EXECUTE` 或 `COM_CHANGE_USER`
- 日志轮转、错误日志（M195）和启动横幅
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor general_log
cargo test -p rusql-server general_log
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --general-log
```

```sql
SET GLOBAL general_log = ON;
SELECT 1;
```

日志打开期间建立的连接会在 `{data-dir}/rusql-general.log` 里出现 `Connect`，并且有 `Query` 和 `SELECT 1`。不传 `--general-log` 且不执行 `SET GLOBAL general_log = ON` 时不会创建文件。
