# 用户指南 — 测试 rusql

说明 **main 分支当前可用功能** 及验证方法。

英文 canonical：[user-guide.md](../en/user-guide.md)

## 前置条件

- Rust 1.75+（[rustup](https://rustup.rs)）
- 可选：MySQL 客户端（`mysql` CLI）用于手动测试

## 构建与运行

```bash
cargo build --release
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

- `--data-dir` — WAL 文件目录（`rusql.wal`），默认 `rusql-data`
- **重启后数据仍在**：停止服务再启动，表和行会从 WAL 重放

### 可选密码验证

默认握手插件为 `caching_sha2_password`（MySQL 8）。`--auth-password` 启用密码校验。

```bash
cargo run -p rusql-server -- --port 3307 --auth-password 你的密码
```

用户默认为 `root`，可用 `--auth-user` 修改。详见 [adr-m6-auth-and-dml.md](../en/specs/adr-m6-auth-and-dml.md)。

中文错误消息：

```bash
RUSQL_LOCALE=zh-CN cargo run -p rusql-server -- --port 3307
```

## 自动化测试

```bash
cargo test -p rusql-server compat
cargo test -p rusql-server com_query
cargo test
```

兼容性 JSON 用例位于 `crates/rusql-server/compat/`。

## MySQL 客户端

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

```sql
CREATE TABLE users (id INT, name VARCHAR(64));
INSERT INTO users VALUES (1, 'alice');
BEGIN;
INSERT INTO users VALUES (2, 'bob');
COMMIT;
DELETE FROM users WHERE id = 1;
DROP TABLE users;

-- GRANT / REVOKE（M54）
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;

-- 复合索引（M50）
CREATE INDEX idx_ab ON t (a, b);
SELECT * FROM t WHERE a = 1 AND b = 2;

-- 多用户认证（M55-auth）
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
CREATE USER 'legacy'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
DROP USER 'legacy'@'%';
```

## 已实现功能（M1–M9）

| 功能 | 状态 | 说明 |
|------|------|------|
| 握手 + 可选密码验证 | 完成 | `--auth-password` |
| COM_QUERY / COM_QUIT | 完成 | |
| CREATE / INSERT / SELECT | 完成 | |
| SELECT 列列表 | 完成 | M14 |
| ORDER BY | 完成 | M17 |
| 列别名 | 完成 | M18 `SELECT col AS alias` |
| LIMIT | 完成 | M16 |
| OFFSET | 完成 | M19 `LIMIT n OFFSET m` |
| DROP / DELETE / UPDATE | 完成 | |
| 事务 | 完成 | `BEGIN` / `COMMIT` / `ROLLBACK` |
| SHOW TABLES / DATABASES | 完成 | M10 元数据发现 |
| DESCRIBE / information_schema | 完成 | M12 表结构发现 |
| SHOW CREATE TABLE | 完成 | M13 DDL 导出 |
| 预编译语句 | 完成 | M11 `COM_STMT_*` |
| COM_CHANGE_USER / COM_RESET_CONNECTION | 完成 | M51 重新认证；重置清除预编译状态 |
| COM_FIELD_LIST / 长参数 | 完成 | M52 字段列表；`COM_STMT_SEND_LONG_DATA` + `COM_STMT_RESET` |
| SHOW PROCESSLIST / COM_PROCESS_INFO | 完成 | M53 连接注册表 |
| 持久化、索引、兼容性测试套件 | 完成 | `cargo test -p rusql-server compat` |

## 持久化测试

```bash
cargo test -p rusql-server persistence_across_connections
```

## 故障排查

| 问题 | 处理 |
|------|------|
| 连接被拒绝 | 确认服务已启动且端口一致 |
| 认证插件错误 | 尝试 `--default-auth=mysql_native_password` |

## 存储程序与复制（P3 MVP）

- **存储过程 / 触发器 / 函数**：`CREATE PROCEDURE`、`CALL`、`CREATE FUNCTION … RETURNS …`（`SELECT` 标量调用）、`CREATE TRIGGER`（BEFORE INSERT 的 `SET NEW.col`；AFTER UPDATE/DELETE 的 `OLD.col`/`NEW.col` DML）、`DROP`；元数据保存在 `{data_dir}/programs.json`。
- **信息模式**：`information_schema.ROUTINES`、`information_schema.TRIGGERS`。
- **COMMIT 写 binlog**：事务提交时将事件追加到 `{data_dir}/binlog/`。`INSERT` 写入 `TABLE_MAP` 再写入 `WRITE_ROWS`（v1，UTF-8 单元格）；`UPDATE`/`DELETE` 写入 `TABLE_MAP` 再写入 `UPDATE_ROWS`/`DELETE_ROWS`（v1）。
- **复制**：`COM_BINLOG_DUMP` 在 flags `0` 时从请求位置起按事件分包（`0x00` + 事件）并保持连接，后续 COMMIT 会继续推送；`BINLOG_DUMP_NON_BLOCK`（`0x01`）导出当前文件后 OK。`COM_REGISTER_SLAVE` 返回 OK；`SHOW MASTER STATUS` / `SHOW SLAVE STATUS`。`apply_binlog_file` 从行事件还原 INSERT SQL。副本上表必须已存在。

## 开发传感器

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/doc-parity.mjs
node scripts/check-changelog.mjs
node scripts/metrics.mjs
```

## 性能基准（PERF-B1）

持久连接微基准（与 [performance-benchmark-2026-08-11.md](../en/reports/performance-benchmark-2026-08-11.md) 相同的 7 项 workload）：

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql --output target/bench-rusql.json
```

### 排序规则（M59 / M62）

`ORDER BY` 与 `WHERE =` 支持列级 `COLLATE`（`CREATE TABLE` 时指定）。默认 `utf8mb4_unicode_ci`；另支持 MySQL 8.0 默认 `utf8mb4_0900_ai_ci`。

### 会话信息函数（M65）

```sql
SELECT DATABASE(), SCHEMA(), USER(), CURRENT_USER(), VERSION();
SELECT LAST_INSERT_ID();
SELECT CONNECTION_ID(), ROW_COUNT();
SELECT @@version, @@autocommit, @@character_set_client, @@collation_connection, @@sql_mode;
SELECT FOUND_ROWS();
```

`VERSION()` 返回 MySQL 8.0 兼容字符串（如 `8.0.33-rusql`）。

### LAST_INSERT_ID（M75）

```sql
CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16));
INSERT INTO t (name) VALUES ('alice');
SELECT LAST_INSERT_ID();
```

会话级：返回该连接上最近一次成功 `INSERT` 生成的第一个 `AUTO_INCREMENT` 值（尚无插入时为 `0`）。INSERT 的 OK 包 `last_insert_id` 与之相同。显式写入的 id 不更新该值；`LAST_INSERT_ID(expr)` 赋值形式未实现。

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```

### CONNECTION_ID / ROW_COUNT（M76）

```sql
SELECT CONNECTION_ID();
INSERT INTO t (id, name) VALUES (1, 'alice');
SELECT ROW_COUNT();
SELECT id FROM t;
SELECT ROW_COUNT();
```

`CONNECTION_ID()` 是本会话握手时的线程 id（与 `SHOW PROCESSLIST` 的 `Id` 相同）。`ROW_COUNT()` 是该连接上最近一次 `INSERT`/`UPDATE`/`DELETE` 的受影响行数；在 `SELECT`（或任何结果集语句）之后为 `-1`。连接之间互不可见；`COM_RESET_CONNECTION` / `COM_CHANGE_USER` 会将 `ROW_COUNT()` 重置为 `-1`。未实现 `FOUND_ROWS()`。

```bash
cargo test -p rusql-executor connection_id
cargo test -p rusql-executor row_count
cargo test -p rusql-server connection_id
cargo test -p rusql-server row_count
```

### 会话变量（M77 / M79 / M80 / M81 / M82 / M83 / M84）

```sql
SELECT @@version, @@version_comment;
SELECT @@autocommit, @@session.autocommit;
SELECT @@character_set_client, @@character_set_connection, @@character_set_results, @@character_set_server;
SELECT @@collation_connection, @@sql_mode;
SELECT @@auto_increment_increment, @@session.auto_increment_increment;
SELECT @@time_zone, @@system_time_zone;
SELECT @@transaction_isolation, @@tx_isolation;
SELECT @@max_allowed_packet, @@license;
SHOW VARIABLES;
SHOW SESSION VARIABLES;
SHOW GLOBAL VARIABLES;
SHOW VARIABLES LIKE 'auto_increment%';
SET @@autocommit = 0;
SELECT @@autocommit, @@session.autocommit;
SET SESSION autocommit = 1;
SET @@session.autocommit = 1;
SET NAMES utf8mb4;
SET NAMES utf8mb4 COLLATE utf8mb4_unicode_ci;
SET CHARACTER SET utf8mb4;
SET CHARSET utf8mb4;
SET @foo = 1;
SELECT @foo;
SELECT @foo := 1;
SELECT @foo;
SET TRANSACTION ISOLATION LEVEL READ COMMITTED;
SELECT @@transaction_isolation, @@tx_isolation;
SET SESSION TRANSACTION ISOLATION LEVEL REPEATABLE READ;
SELECT @@transaction_isolation;
```

面向客户端/ORM 探测的文档化 stub 集合。`@@version` 与 `VERSION()` 相同（`8.0.33-rusql`）。默认 `@@autocommit` 为 `1`。字符集变量返回 `utf8mb4`。`@@collation_connection` 为 `utf8mb4_0900_ai_ci`。`@@sql_mode` 为类 MySQL 8.0 的模式字符串（不强制执行）。连接器握手 stub：`@@auto_increment_increment` 为 `1`；`@@time_zone` 为 `SYSTEM`；`@@system_time_zone` 为 `UTC`（非主机时区）；`@@transaction_isolation` / `@@tx_isolation` 为 `REPEATABLE-READ`；`@@max_allowed_packet` 为 `67108864`；`@@license` 为 `GPL`。对本集合 `@@session.var` 与 `@@var` 等价。未知名称返回 errno 1193。`SHOW VARIABLES` / `SHOW SESSION VARIABLES` 列出 stub 目录（`Variable_name`、`Value`），含该连接上的 `SET` 覆盖；`SHOW GLOBAL VARIABLES` 保持文档化默认值。`LIKE` 过滤该集合；不匹配的模式返回空结果。这不是完整的 MySQL 8.0 目录。

`SET @@var`、`SET @@session.var`、`SET SESSION var` 与 `SET var` 在该连接内存中持久化（不写 WAL）。设置 `transaction_isolation` 同时更新 `tx_isolation`（反之亦然）。`COM_RESET_CONNECTION` 与 `COM_CHANGE_USER` 恢复文档化默认值。`SET GLOBAL` 被拒绝（errno 1229）。只读 stub `version`、`version_comment`、`license`、`system_time_zone` 拒绝 SET（errno 1238）。autocommit 的 DML 引擎行为不变（仍为自动提交）。

`SET NAMES charset [COLLATE collation]` 覆盖 `@@character_set_client` / `connection` / `results`（不改变数据包编码）。`utf8mb4` 且无 `COLLATE` 时将 `@@collation_connection` 设为 `utf8mb4_0900_ai_ci`。`SET NAMES DEFAULT` 恢复这些 stub。`SET CHARACTER SET charset` 与 `SET CHARSET charset` 在本覆盖语义上是 `SET NAMES` 的别名。`SET @foo = expr` 后在该连接上 `SELECT @foo` 返回该值；`SELECT @foo := expr` 赋值并返回该值。未赋值用户变量为空单元格（NULL）。

`SET TRANSACTION ISOLATION LEVEL …` 与 `SET SESSION TRANSACTION ISOLATION LEVEL …` 以连字符名称覆盖 `@@transaction_isolation` / `@@tx_isolation`（`READ-COMMITTED`、`REPEATABLE-READ`、`SERIALIZABLE`、`READ-UNCOMMITTED`）。rusql 将两种形式视为同一内存会话覆盖（不做 MySQL 的“仅下一事务”作用域）。DML 引擎隔离仍为快照。`SET GLOBAL TRANSACTION` 被拒绝（errno 1229）。`SET TRANSACTION READ ONLY` / `READ WRITE` 为空操作。

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-executor set_session_var
cargo test -p rusql-executor set_names
cargo test -p rusql-executor set_charset
cargo test -p rusql-executor user_var
cargo test -p rusql-executor show_variables
cargo test -p rusql-executor set_transaction
cargo test -p rusql-server session_var
cargo test -p rusql-server set_session_var
cargo test -p rusql-server set_names
cargo test -p rusql-server set_charset
cargo test -p rusql-server user_var
cargo test -p rusql-server show_variables
cargo test -p rusql-server set_transaction
```

### FOUND_ROWS / SQL_CALC_FOUND_ROWS（M78）

```sql
SELECT id FROM t LIMIT 1;
SELECT FOUND_ROWS();
SELECT SQL_CALC_FOUND_ROWS id FROM t LIMIT 1;
SELECT FOUND_ROWS();
```

普通 `SELECT` 之后，`FOUND_ROWS()` 为该结果集行数（含 `LIMIT` 之后）。`SELECT SQL_CALC_FOUND_ROWS … LIMIT n` 再执行 `FOUND_ROWS()` 返回**未应用** `LIMIT` 的匹配行数（MySQL 8.0.17 起已弃用的分页辅助；推荐 `COUNT(*)`）。`INSERT`/`UPDATE`/`DELETE` 之后与 `ROW_COUNT()` 相同。会话级；`COM_RESET_CONNECTION` / `COM_CHANGE_USER` 重置为 `0`。

```bash
cargo test -p rusql-executor found_rows
cargo test -p rusql-server found_rows
```

### CASE / IF（M66）

```sql
SELECT CASE WHEN id = 1 THEN 'one' ELSE 'other' END FROM t;
SELECT CASE name WHEN 'a' THEN 1 ELSE 0 END FROM t;
SELECT IF(id > 0, 'yes', 'no') FROM t;
```

### SELECT DISTINCT（M67）

```sql
SELECT DISTINCT tag FROM t ORDER BY tag;
SELECT DISTINCT tag FROM t ORDER BY tag LIMIT 1;
```

### INSERT … SELECT / ON DUPLICATE KEY UPDATE（M68）

```sql
INSERT INTO dst (id, name) SELECT id, name FROM src WHERE id > 1;
INSERT INTO dst VALUES (1, 'z') ON DUPLICATE KEY UPDATE name = VALUES(name);
```

### WITH CTE（M69）

```sql
WITH c AS (SELECT id, name FROM t WHERE id > 1) SELECT id, name FROM c;
```

```bash
cargo test -p rusql-executor with_cte
cargo test -p rusql-server with_cte
```

### 窗口排名函数（M70）

```sql
SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS n FROM t;
SELECT grp, RANK() OVER (PARTITION BY grp ORDER BY score) AS r FROM t;
SELECT grp, DENSE_RANK() OVER (PARTITION BY grp ORDER BY score) AS d FROM t;
```

```bash
cargo test -p rusql-executor window
cargo test -p rusql-server window
```

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation
```

```sql
SHOW COLLATION;
CREATE TABLE t (name VARCHAR(64) COLLATE utf8mb4_0900_ai_ci);
```

### Sysbench 对比（M61 / PERF-B6）

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-sysbench
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308 --threshold 0.7
```

## 性能优化（PERF-B2 / PERF-B3）

```bash
cargo test -p rusql-storage scan_index_ordered_with_limit pk_update_without_index_rebuild
cargo test -p rusql-executor select_order_by_indexed_limit update_pk_by_index
```

### 多线程基准（PERF-B4）

```bash
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308
```

### WAL 同步策略（PERF-B5）

```bash
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
```
