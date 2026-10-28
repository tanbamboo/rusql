# ADR：M195 错误日志 `--log-error`

**状态**：已接受（M195 / [#348](https://github.com/tanbamboo/rusql/issues/348)）  
**日期**：2026-10-28

## 背景

运维需要把服务器错误写进文件。MySQL 8.0 用 `--log-error` / `--log_error`，以及只读全局变量 `log_error`。不传该开关时，消息留在 stderr。这一片写入该文件。不修改 `CONSTITUTION.md`。不是通用查询日志（M192），不是慢查询日志（M191），也不是 M185 的审计 JSON 文件。

## 决定

1. **默认关闭。** 没有 `--log-error` 时，rusql 不创建、不追加文件。stderr 上的 tracing 仍是原来的订阅器（环境变量里的 `EnvFilter`）。默认级别不会被抬到 debug。`@@log_error` 和 `@@global.log_error` 是记号 `stderr`。

2. **开关。** `--log-error=PATH` 与 `--log_error=PATH` 是同一个选项。进程在启动时打开该路径（追加，不存在则创建）。空路径或打不开的路径以 `server.error_log_open_failed` 停止进程。其后的启动失败（存储、权限、绑定、TLS 材料、其他日志文件）先追加一行 `Error` 再退出。绑定成功后追加一行 `System`，内容是已经本地化的 `server.starting` 文本，包含端口。

3. **文件打开时追加什么。**
   - 登录失败（未知账号、密码错误、明文 `REQUIRE SSL`）写一行 `Error`，errno **1045**，文本是本地化的拒绝访问消息。密码字节不写入。
   - 已建立连接上的每个 SQL ERR 包写一行 `Error`，带该 errno 和包里的消息。成功的语句不是一行。`SELECT 1` 成功，所以不会出现。
   - 其他连接失败，只要文本不是拒绝访问，就写一行 `Error`，errno 为 `0`，消息已脱敏。
   - 磁盘满时服务器记一条警告。客户端仍得到原来的 errno。

4. **文件行（子集）。** 字段用制表符分隔。`System` 和 `Error` 在所有语言环境里都是文件格式记号。连接 id `0` 表示启动。没有 `log_error_verbosity` 矩阵，也不轮转。

   ```
   2026-10-28T00:00:00.000Z	0	System	Starting rusql server on port 3307
   2026-10-28T00:00:00.000Z	8	Error	1045	Access denied for user
   2026-10-28T00:00:00.000Z	8	Error	1146	Table 'no_such' doesn't exist
   ```

   时间是带毫秒的 UTC。消息压成一行。示例里的英文句子是 en-US 文案；zh-CN 进程写入对应的本地化句子。

5. **变量。** `log_error` 仅全局且只读。`SELECT @@log_error` / `@@global.log_error` 返回启动路径；省略开关时为 `stderr`。`SET GLOBAL` 是 errno **1238**。`SET` 和 `SET SESSION` 是 errno **1229**（语句解析器不会把不带范围的 `SET` 标成 `GLOBAL`，因此两者都按“对全局变量做会话赋值”拒绝）。`SELECT @@session.log_error` 是 errno **1238**。`SHOW VARIABLES` 列出该名字。进程运行期间不能改路径。

6. **密码。** 与 M185 审计日志相同的字面量（`IDENTIFIED BY`、`IDENTIFIED BY PASSWORD`、`IDENTIFIED WITH … BY|AS`、`SET PASSWORD … =`、`PASSWORD(`）在错误消息里变成 `'<redacted>'`。该记号在所有语言环境里都是文件格式的一部分。不复制认证包。不写入 `--auth-password`。

## 后果

- 通用日志、慢查询日志和审计文件不变。失败语句在那些日志打开时也可以同时出现在错误日志里。
- `mysql-diff` 的 `log_error` 比较只读和仅全局的失败。它不比较 `@@log_error` 的文本（MySQL 的值是主机路径或 `stderr`）。文件检查是 `cargo test -p rusql-server log_error`。
- `performance_schema.global_variables` 包含 `log_error`，因为它复用 `SHOW VARIABLES`。

## 范围外

- `log_error_verbosity`、`log_error_services`，以及 JSON 错误日志
- 日志轮转，以及 UTC 以外的时间戳时区
- 记录结果行或成功语句
- 声称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-executor log_error
cargo test -p rusql-server log_error
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --log-error ./rusql-data/rusql.err
```

登录失败和 `SELECT * FROM no_such_table` 会追加 `Error` 行。不带 `--log-error` 启动不会创建该文件，且 `@@log_error` 为 `stderr`。
