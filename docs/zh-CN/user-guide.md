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
| SHOW TABLE STATUS | 完成 | M87 文档化 stub；`LIKE` / 可选 `FROM` db |
| SHOW ENGINES | 完成 | M88 文档化 stub（`InnoDB` DEFAULT） |
| SHOW CHARACTER SET | 完成 | M89 文档化 stub（`utf8mb4`） |
| SHOW WARNINGS / ERRORS | 完成 | M90 文档化空列表 |
| SHOW CREATE DATABASE | 完成 | M91 文档化 stub DDL（`utf8mb4` / `utf8mb4_unicode_ci`） |
| SHOW CREATE VIEW | 完成 | M92 由目录 SELECT 重建 |
| SHOW TRIGGERS | 完成 | M93 目录行；Definer/sql_mode/字符集为 stub |
| SHOW CREATE TRIGGER | 完成 | M94 由目录重建 DDL；sql_mode/字符集为 stub |
| SHOW CREATE PROCEDURE | 完成 | M95 由目录重建 DDL；空参数列表；字符集为 stub |
| SHOW CREATE FUNCTION | 完成 | M96 由目录重建 DDL；空参数列表；字符集为 stub |
| SHOW PROCEDURE STATUS | 完成 | M97 目录行；Definer/时间戳/字符集为 stub |
| SHOW FUNCTION STATUS | 完成 | M98 目录行；Definer/时间戳/字符集为 stub |
| SHOW CREATE USER | 完成 | M99 由目录重建 DDL；插件名，无哈希 |
| SHOW CREATE EVENT | 完成 | M102 由目录重建 DDL；未知 errno 1539 |
| SHOW EVENTS | 完成 | M102 目录行；不匹配的 `LIKE` 为零行 |
| CREATE EVENT / DROP EVENT | 完成 | M102 目录持久化；M104–M107 调度 / DEFINER / ON COMPLETION |
| ALTER EVENT | 完成 | M103 目录调度/状态/重命名/`DO`；M106 `STARTS`/`ENDS`；M107 DEFINER / ON COMPLETION |
| 事件调度器（到期 AT） | 完成 | M104 执行到期的 ENABLED `ONE TIME` `AT`；`@@event_scheduler` 为 `ON` |
| 事件调度器（`EVERY`） | 完成 | M105 下一条 COM_QUERY 首次执行，之后按 `last_executed + interval` |
| 事件调度器（`STARTS`/`ENDS`） | 完成 | M106 约束 `EVERY`；`SHOW EVENTS` Starts/Ends 来自目录 |
| 事件 DEFINER / ON COMPLETION | 完成 | M107 目录；`PRESERVE` 使 AT 执行后保留为 DISABLED |
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

- **存储过程 / 触发器 / 函数 / 事件**：`CREATE PROCEDURE`、`CALL`、`CREATE FUNCTION … RETURNS …`（`SELECT` 标量调用）、`CREATE TRIGGER`（BEFORE INSERT 的 `SET NEW.col`；AFTER UPDATE/DELETE 的 `OLD.col`/`NEW.col` DML）、`CREATE EVENT … ON SCHEDULE … DO …` / `ALTER EVENT`（目录；到期的一次性 `AT` 与 `EVERY` 事件在 COM_QUERY 上执行 `DO`，受 `STARTS`/`ENDS` 约束；含 `DEFINER` / `ON COMPLETION`）、`DROP PROCEDURE` / `DROP FUNCTION` / `DROP TRIGGER` / `DROP EVENT`；元数据保存在 `{data_dir}/programs.json`。
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

`CONNECTION_ID()` 是本会话握手时的线程 id（与 `SHOW PROCESSLIST` 的 `Id` 相同）。`ROW_COUNT()` 是该连接上最近一次 `INSERT`/`UPDATE`/`DELETE` 的受影响行数；在 `SELECT`（或任何结果集语句）之后为 `-1`。连接之间互不可见；`COM_RESET_CONNECTION` / `COM_CHANGE_USER` 会将 `ROW_COUNT()` 重置为 `-1`。`FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` 见 M78。

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

面向客户端/ORM 探测的文档化 stub 集合。`@@version` 与 `VERSION()` 相同（`8.0.33-rusql`）。默认 `@@autocommit` 为 `1`。字符集变量返回 `utf8mb4`。`@@collation_connection` 为 `utf8mb4_0900_ai_ci`。`@@sql_mode` 为类 MySQL 8.0 的模式字符串（不强制执行）。连接器握手 stub：`@@auto_increment_increment` 为 `1`；`@@time_zone` 为 `SYSTEM`；`@@system_time_zone` 为 `UTC`（非主机时区）；`@@transaction_isolation` / `@@tx_isolation` 为 `REPEATABLE-READ`；`@@max_allowed_packet` 为 `67108864`；`@@license` 为 `GPL`；`@@event_scheduler` 为 `ON`（只读）。对本集合 `@@session.var` 与 `@@var` 等价。未知名称返回 errno 1193。`SHOW VARIABLES` / `SHOW SESSION VARIABLES` 列出 stub 目录（`Variable_name`、`Value`），含该连接上的 `SET` 覆盖；`SHOW GLOBAL VARIABLES` 保持文档化默认值。`LIKE` 过滤该集合；不匹配的模式返回空结果。这不是完整的 MySQL 8.0 目录。

`SET @@var`、`SET @@session.var`、`SET SESSION var` 与 `SET var` 在该连接内存中持久化（不写 WAL）。设置 `transaction_isolation` 同时更新 `tx_isolation`（反之亦然）。`COM_RESET_CONNECTION` 与 `COM_CHANGE_USER` 恢复文档化默认值。`SET GLOBAL` 被拒绝（errno 1229）。只读 stub `version`、`version_comment`、`license`、`system_time_zone`、`event_scheduler` 拒绝 SET（errno 1238）。autocommit 的 DML 引擎行为不变（仍为自动提交）。

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

### SELECT … FOR UPDATE（M85）

```sql
BEGIN;
SELECT id FROM t FOR UPDATE;
SELECT id FROM t FOR SHARE;
SELECT id FROM t LOCK IN SHARE MODE;
SELECT id FROM t FOR UPDATE NOWAIT;
SELECT id FROM t FOR UPDATE SKIP LOCKED;
COMMIT;
```

作为文档化空操作接受：返回行与无锁 `SELECT` 相同。rusql 不获取 InnoDB 风格行锁、不等待、不跳过已锁行。两个连接可同时对同一行执行 `SELECT … FOR UPDATE`。M84 的 `SET TRANSACTION ISOLATION LEVEL` 覆盖不变；DML 仍为快照隔离。未实现 `GET_LOCK()` 与列级 `FOR UPDATE OF col`（`OF table` 被忽略）。

```bash
cargo test -p rusql-executor for_update
cargo test -p rusql-server for_update
```

### SHOW TABLE STATUS（M87）

```sql
SHOW TABLE STATUS;
SHOW TABLE STATUS LIKE 't%';
SHOW TABLE STATUS LIKE 'no_such%';
SHOW TABLE STATUS FROM rusql;
```

当前库每张表一行（`Name` 集合与 `SHOW TABLES` 相同），列形与 MySQL 接近：`Name`、`Engine`、`Version`、`Row_format`、`Rows`、`Avg_row_length`、`Data_length`、`Max_data_length`、`Index_length`、`Data_free`、`Auto_increment`、`Create_time`、`Update_time`、`Check_time`、`Collation`、`Checksum`、`Create_options`、`Comment`。`Engine` 为文档化 stub `InnoDB`；`Version` 为 `10`；`Row_format` 为 `Dynamic`；`Rows` 为堆行数；有自增列时 `Auto_increment` 跟随表计数器；`Collation` 为 `utf8mb4_unicode_ci`。其余数值/时间/注释单元格为 `0` 或空（不是 InnoDB 表空间统计）。`LIKE` 按 `Name` 过滤；不匹配的模式返回空结果。可选 `FROM`/`IN` 列出另一个已存在的数据库。M86 的 `SHOW STATUS` 行为不变。未实现 `SHOW ENGINE INNODB STATUS` 与 `WHERE` 过滤。

```bash
cargo test -p rusql-sql table_status
cargo test -p rusql-executor table_status
cargo test -p rusql-server table_status
```

### SHOW ENGINES（M88）

```sql
SHOW ENGINES;
SHOW STORAGE ENGINES;
```

面向客户端/GUI 探测的文档化 stub 目录（`Engine`、`Support`、`Comment`、`Transactions`、`XA`、`Savepoints`）：`InnoDB`（`DEFAULT`，与 M87 `SHOW TABLE STATUS` 一致）、`MEMORY`、`MyISAM`、`PERFORMANCE_SCHEMA`（`YES`）。注释与 YES/NO 标志为常量；rusql 不切换存储引擎。这不是完整的 MySQL 8.0 插件列表。M87 的 `SHOW TABLE STATUS` 与 M86 的 `SHOW STATUS` 行为不变。未实现 `SHOW ENGINE INNODB STATUS` 与 `ENGINE=` 切换。

```bash
cargo test -p rusql-sql show_engines
cargo test -p rusql-executor show_engines
cargo test -p rusql-server show_engines
```

### SHOW CHARACTER SET（M89）

```sql
SHOW CHARACTER SET;
SHOW CHARSET;
SHOW CHARACTER SET LIKE 'utf8%';
SHOW CHARACTER SET LIKE 'no_such_charset%';
```

面向客户端/GUI 探测的文档化 stub 目录（`Charset`、`Description`、`Default collation`、`Maxlen`）：`utf8mb4`（`Maxlen` 为 `4`，`Default collation` 为 `utf8mb4_unicode_ci`，与 M87 `SHOW TABLE STATUS` 的 `Collation` 一致）。`LIKE` 按 `Charset` 过滤；不匹配的模式返回空结果。这不是完整的 MySQL 8.0 字符集目录，也不是线协议编码转换。M59 的 `SHOW COLLATION`、M88 的 `SHOW ENGINES` 与 M83 的 `SET CHARACTER SET` 行为不变。

```bash
cargo test -p rusql-sql character_set
cargo test -p rusql-executor character_set
cargo test -p rusql-server character_set
```

### SHOW WARNINGS（M90）

```sql
SELECT 1;
SHOW WARNINGS;
SHOW ERRORS;
```

面向客户端/驱动探测的文档化空诊断列表（`Level`、`Code`、`Message`）。成功且无诊断的语句之后结果为零行，而不是不支持的语句错误。对本切片 `SHOW ERRORS` 等价。这不是实时截断 / sql_mode / note 生成。未实现 `SHOW COUNT(*) WARNINGS`、`LIMIT` 与 `WHERE`。M89 的 `SHOW CHARACTER SET` 与 M88 的 `SHOW ENGINES` 行为不变。

```bash
cargo test -p rusql-sql warnings
cargo test -p rusql-executor warnings
cargo test -p rusql-server warnings
```

### SHOW CREATE DATABASE（M91）

```sql
SHOW CREATE DATABASE rusql;
SHOW CREATE SCHEMA rusql;
```

面向客户端/GUI 探测的文档化 stub DDL（`Database`、`Create Database`）。`Create Database` 单元格含 `utf8mb4` 与 rusql 文档化默认排序规则 `utf8mb4_unicode_ci`（常量，不是按库的实时字符集目录）。对本切片 `SHOW CREATE SCHEMA` 等价。未知数据库返回 errno 1049。这不是 `CREATE DATABASE … CHARACTER SET` / `COLLATE`，也不是完整 mysqld dump。M13 的 `SHOW CREATE TABLE` 与 M90 的 `SHOW WARNINGS` 行为不变。

```bash
cargo test -p rusql-sql show_create_database
cargo test -p rusql-executor show_create_database
cargo test -p rusql-server show_create_database
```

### SHOW CREATE VIEW（M92）

```sql
CREATE VIEW v_ids AS SELECT id FROM users;
SHOW CREATE VIEW v_ids;
```

面向客户端/GUI 探测、由目录重建的 DDL（`View`、`Create View`、`character_set_client`、`collation_connection`）。`Create View` 单元格为 `CREATE VIEW … AS` 加上 M33 保存的 SELECT；字符集/排序规则单元格为文档化 stub（`utf8mb4` / `utf8mb4_unicode_ci`）。未知视图返回 errno 1146。这不是 ALGORITHM / DEFINER / SQL SECURITY。M13 的 `SHOW CREATE TABLE`、M91 的 `SHOW CREATE DATABASE` 与 M90 的 `SHOW WARNINGS` 行为不变。

```bash
cargo test -p rusql-sql show_create_view
cargo test -p rusql-executor show_create_view
cargo test -p rusql-server show_create_view
```

### SHOW TRIGGERS（M93）

```sql
CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id;
SHOW TRIGGERS;
SHOW TRIGGERS LIKE 'tr_s%';
SHOW TRIGGERS FROM rusql;
```

面向客户端/GUI 探测的目录触发器列表（`Trigger`、`Event`、`Table`、`Statement`、`Timing`，以及文档化 stub `Created`、`sql_mode`、`Definer`、`character_set_client`、`collation_connection`、`Database Collation`）。`Trigger` / `Event` / `Table` / `Timing` / `Statement` 来自 M48 `TriggerMeta`；Definer 为 stub `root@%`，字符集/排序规则为 `utf8mb4` / `utf8mb4_unicode_ci`，`Created` / `sql_mode` 为空。`LIKE` 按触发器名过滤；不匹配的模式返回零行。未知 `FROM`/`IN` 数据库返回 errno 1049。这不是 `SHOW CREATE TRIGGER`，也不是实时 DEFINER / sql_mode 持久化。M92 的 `SHOW CREATE VIEW`、M13 的 `SHOW CREATE TABLE` 与 M91 的 `SHOW CREATE DATABASE` 行为不变。

```bash
cargo test -p rusql-sql show_triggers
cargo test -p rusql-executor show_triggers
cargo test -p rusql-server show_triggers
```

### SHOW CREATE TRIGGER（M94）

```sql
CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id;
SHOW CREATE TRIGGER tr_src;
```

面向客户端/GUI 探测、由目录重建的 DDL（`Trigger`、`sql_mode`、`SQL Original Statement`、`character_set_client`、`collation_connection`、`Database Collation`、`Created`）。`SQL Original Statement` 单元格为 `CREATE TRIGGER … {BEFORE|AFTER} {INSERT|UPDATE|DELETE} ON … FOR EACH ROW …`，来自 M48 `TriggerMeta`；字符集/排序规则为文档化 stub（`utf8mb4` / `utf8mb4_unicode_ci`），`sql_mode` / `Created` 为空。未知触发器返回 errno 1360。这不是 DEFINER / sql_mode dump。M93 的 `SHOW TRIGGERS`、M92 的 `SHOW CREATE VIEW` 与 M13 的 `SHOW CREATE TABLE` 行为不变。

```bash
cargo test -p rusql-sql show_create_trigger
cargo test -p rusql-executor show_create_trigger
cargo test -p rusql-server show_create_trigger
```

### SHOW CREATE PROCEDURE（M95）

```sql
CREATE PROCEDURE p() BEGIN INSERT INTO src VALUES (42); END;
SHOW CREATE PROCEDURE p;
```

面向客户端/GUI 探测、由目录重建的 DDL（`Procedure`、`sql_mode`、`Create Procedure`、`character_set_client`、`collation_connection`、`Database Collation`）。`Create Procedure` 单元格为 `CREATE PROCEDURE …() BEGIN … END`，来自 P3 `ProcedureMeta`（空参数列表）；字符集/排序规则为文档化 stub（`utf8mb4` / `utf8mb4_unicode_ci`），`sql_mode` 为空。未知存储过程返回 errno 1305。这不是 DEFINER / sql_mode dump，也不是发明的 IN/OUT 参数。M94 的 `SHOW CREATE TRIGGER`、M93 的 `SHOW TRIGGERS` 与 M92 的 `SHOW CREATE VIEW` 行为不变。

```bash
cargo test -p rusql-sql show_create_procedure
cargo test -p rusql-executor show_create_procedure
cargo test -p rusql-server show_create_procedure
```

### SHOW CREATE FUNCTION（M96）

```sql
CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END;
SHOW CREATE FUNCTION f;
```

面向客户端/GUI 探测、由目录重建的 DDL（`Function`、`sql_mode`、`Create Function`、`character_set_client`、`collation_connection`、`Database Collation`）。`Create Function` 单元格为 `CREATE FUNCTION …() RETURNS … BEGIN RETURN … END`，来自 M63 `FunctionMeta`（空参数列表）；字符集/排序规则为文档化 stub（`utf8mb4` / `utf8mb4_unicode_ci`），`sql_mode` 为空。未知函数返回 errno 1305。这不是 DEFINER / sql_mode dump，也不是发明的 IN/OUT 参数。M95 的 `SHOW CREATE PROCEDURE`、M94 的 `SHOW CREATE TRIGGER` 与 M92 的 `SHOW CREATE VIEW` 行为不变。

```bash
cargo test -p rusql-sql show_create_function
cargo test -p rusql-executor show_create_function
cargo test -p rusql-server show_create_function
```

### SHOW PROCEDURE STATUS（M97）

```sql
CREATE PROCEDURE p() BEGIN INSERT INTO src VALUES (42); END;
SHOW PROCEDURE STATUS;
SHOW PROCEDURE STATUS LIKE 'p%';
```

面向客户端/GUI 探测的目录存储过程列表（`Db`、`Name`、`Type`、`Definer`、`Modified`、`Created`、`Security_type`、`Comment`、`character_set_client`、`collation_connection`、`Database Collation`）。`Db` / `Name` 来自 P3 `ProcedureMeta`；`Type` 为 `PROCEDURE`。Definer / 时间戳 / 字符集为文档化 stub（`root@%`、空的 `Modified`/`Created`/`Comment`、`DEFINER`、`utf8mb4` / `utf8mb4_unicode_ci`）。不匹配的 `LIKE` 返回零行。这不是实时 DEFINER 持久化，也不是 `SHOW FUNCTION STATUS`。M96 的 `SHOW CREATE FUNCTION`、M95 的 `SHOW CREATE PROCEDURE` 与 M94 的 `SHOW CREATE TRIGGER` 行为不变。

```bash
cargo test -p rusql-sql show_procedure_status
cargo test -p rusql-executor show_procedure_status
cargo test -p rusql-server show_procedure_status
```

### SHOW FUNCTION STATUS（M98）

```sql
CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END;
SHOW FUNCTION STATUS;
SHOW FUNCTION STATUS LIKE 'f%';
```

面向客户端/GUI 探测的目录函数列表（`Db`、`Name`、`Type`、`Definer`、`Modified`、`Created`、`Security_type`、`Comment`、`character_set_client`、`collation_connection`、`Database Collation`）。`Db` / `Name` 来自 M63 `FunctionMeta`；`Type` 为 `FUNCTION`。Definer / 时间戳 / 字符集为文档化 stub（`root@%`、空的 `Modified`/`Created`/`Comment`、`DEFINER`、`utf8mb4` / `utf8mb4_unicode_ci`）。不匹配的 `LIKE` 返回零行。这不是实时 DEFINER 持久化。M97 的 `SHOW PROCEDURE STATUS`、M96 的 `SHOW CREATE FUNCTION` 与 M95 的 `SHOW CREATE PROCEDURE` 行为不变。

```bash
cargo test -p rusql-sql show_function_status
cargo test -p rusql-executor show_function_status
cargo test -p rusql-server show_function_status
```

### SHOW CREATE USER（M99）

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
SHOW CREATE USER 'app'@'%';
SHOW CREATE USER CURRENT_USER();
```

面向客户端/GUI 探测、由目录重建的 DDL（`CREATE USER for {user}@{host}`）。单元格为 `CREATE USER \`u\`@\`h\` IDENTIFIED WITH '{plugin}'`，来自 M55 账户目录；不含密码哈希、`BY` 或 `AS`。支持 `'u'@'h'` / `user@host` 与会话 `CURRENT_USER`。未知账户返回 errno 3162。这不是 TLS / 资源限制 / DEFAULT ROLE dump。M98 的 `SHOW FUNCTION STATUS`、M97 的 `SHOW PROCEDURE STATUS` 与 M96 的 `SHOW CREATE FUNCTION` 行为不变。

```bash
cargo test -p rusql-sql show_create_user
cargo test -p rusql-executor show_create_user
cargo test -p rusql-server show_create_user
```

### SHOW CREATE EVENT（M100 / M102）

```sql
CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1;
SHOW CREATE EVENT e;
```

面向客户端/GUI 探测、由目录重建的 DDL（`Event`、`sql_mode`、`time_zone`、`Create Event`、`character_set_client`、`collation_connection`、`Database Collation`）。`Create Event` 单元格为 `CREATE DEFINER=\`u\`@\`h\` EVENT \`name\` ON SCHEDULE AT '…' ON COMPLETION NOT PRESERVE DO …` 或 `EVERY n UNIT [STARTS '…'] [ENDS '…']`，来自 `EventMeta`。`sql_mode` / 字符集为文档化 stub（空的 `sql_mode`、`SYSTEM` 时区、`utf8mb4` / `utf8mb4_unicode_ci`）。未知名称返回 errno 1539。这不是 COMMENT dump，也不是定时线程。M99 的 `SHOW CREATE USER` 与 M98 的 `SHOW FUNCTION STATUS` 行为不变。

```bash
cargo test -p rusql-sql show_create_event
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server show_create_event
```

### SHOW EVENTS（M101 / M102）

```sql
SHOW EVENTS;
SHOW EVENTS LIKE 'e%';
SHOW EVENTS FROM rusql;
```

面向客户端/GUI 探测的目录事件列表（`Db`、`Name`、`Definer`、`Time zone`、`Type`、`Execute at`、`Interval value`、`Interval field`、`Starts`、`Ends`、`Status`、`Originator`、`character_set_client`、`collation_connection`、`Database Collation`）。`Db` / `Name` / `Type` / 调度 / `Starts` / `Ends` / `Definer` 单元格来自 `EventMeta`；时区 / 字符集 / Originator 为文档化 stub（`SYSTEM`、`1`、`utf8mb4` / `utf8mb4_unicode_ci`）。不匹配的 `LIKE` 返回零行。未知 `FROM` 数据库返回 errno 1049。这不是实时 last-executed 时间戳。M99 的 `SHOW CREATE USER` 与 M98 的 `SHOW FUNCTION STATUS` 行为不变。

```bash
cargo test -p rusql-sql show_events
cargo test -p rusql-executor show_events
cargo test -p rusql-server show_events
```

### CREATE EVENT 目录（M102）

```sql
CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1;
CREATE EVENT r ON SCHEDULE EVERY 1 HOUR DO SELECT 1;
DROP EVENT e;
```

将 `EventMeta` 持久化到 `{data_dir}/programs.json`（以及会话目录），供客户端/GUI 探测。重复名称返回 errno 1537。接受 `IF NOT EXISTS` / `DROP EVENT IF EXISTS`。到期的 ENABLED 一次性 `AT` 事件在下一条 COM_QUERY 执行 `DO`（M104）。周期 `EVERY` 在下一条 COM_QUERY 执行，之后按间隔再次执行（M105），受 `STARTS`/`ENDS` 约束（M106）。M103 的 `ALTER EVENT` 可改调度 / 状态 / 名称 / 体 / 窗口。M99 的 `SHOW CREATE USER` 与 M98 的 `SHOW FUNCTION STATUS` 行为不变。

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-executor create_event
cargo test -p rusql-server create_event
```

### ALTER EVENT 目录（M103）

```sql
ALTER EVENT e ON SCHEDULE EVERY 1 DAY;
ALTER EVENT e DISABLE;
ALTER EVENT e RENAME TO e2 DO SELECT 2;
```

更新已存储的 `EventMeta`，供客户端/GUI 探测。未知名称返回 errno 1539。`SHOW EVENTS` 与 `SHOW CREATE EVENT` 反映变更。到期的一次性 `AT` 事件在下一条 COM_QUERY 执行 `DO`（M104）。周期 `EVERY` 在下一条 COM_QUERY 执行，之后按间隔再次执行（M105），受 `STARTS`/`ENDS` 约束（M106）。DEFINER / ON COMPLETION 会持久化（M107）。这不是 COMMENT 持久化。M99 的 `SHOW CREATE USER` 与 M98 的 `SHOW FUNCTION STATUS` 行为不变。

```bash
cargo test -p rusql-sql alter_event
cargo test -p rusql-executor alter_event
cargo test -p rusql-server alter_event
```

### 事件调度器到期 AT（M104）

```sql
CREATE TABLE t (id INT PRIMARY KEY);
CREATE EVENT e ON SCHEDULE AT '2000-01-01 00:00:00' DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SELECT @@event_scheduler;
SHOW VARIABLES LIKE 'event_scheduler';
```

到期（UTC `YYYY-MM-DD HH:MM:SS`）的 ENABLED 一次性 `AT` 事件在该服务器下一条 COM_QUERY 执行 `DO`，然后删除目录行（MySQL 默认 `ON COMPLETION NOT PRESERVE`）。`@@event_scheduler` 为只读 `ON` stub（SET 为 errno 1238；`SET GLOBAL` 仍为 1229）。DISABLED 与未来 `AT` 不执行。周期 `EVERY` 见 M105。`DO` 出错不导致客户端语句失败。这不是后台定时线程、`SHOW EVENTS` last-executed 列或 DEFINER。M103 的 `ALTER EVENT` 与 M99 的 `SHOW CREATE USER` 行为不变。

```bash
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor session_var
cargo test -p rusql-server event_scheduler
```

### 事件调度器 EVERY（M105）

```sql
CREATE TABLE t (id INT);
CREATE EVENT e ON SCHEDULE EVERY 1 HOUR DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SHOW EVENTS LIKE 'e';
```

ENABLED 的 `RECURRING` `EVERY n UNIT` 事件在下一条 COM_QUERY 执行 `DO`，之后按 `last_executed + interval` 再次执行（内部水位；不是 `SHOW EVENTS` 列）。目录行保留。`MONTH`/`YEAR` 按 30/365 天近似。DISABLED 的 `EVERY` 不执行。`STARTS`/`ENDS` 约束见 M106。M104 一次性 `AT`（执行后删除）与 M99 的 `SHOW CREATE USER` 行为不变。

```bash
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```

### 事件调度器 STARTS / ENDS（M106）

```sql
CREATE TABLE t (id INT);
CREATE EVENT e ON SCHEDULE EVERY 1 HOUR STARTS '2000-01-01 00:00:00' ENDS '2038-01-01 00:00:00' DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SHOW EVENTS LIKE 'e';
ALTER EVENT e ON SCHEDULE EVERY 1 HOUR STARTS '2001-01-01 00:00:00';
ALTER EVENT e STARTS '2001-01-01 00:00:00';
```

ENABLED 的 `EVERY` 事件在 `now < starts` 或 `now > ends` 时不执行（窗口含端点）。一旦 `starts` 到期，仍在下一条 COM_QUERY 首次执行（单元测试注入 `now`；无 sleep）。`SHOW EVENTS` 的 `Starts` / `Ends` 来自目录（未设置时为空）。`SHOW CREATE EVENT` 重建 `STARTS` / `ENDS`。列数仍为 15。MySQL 要求 `STARTS`/`ENDS` 写在 `ON SCHEDULE EVERY n UNIT` 之后；rusql 也接受独立的 `ALTER EVENT name STARTS` / `ENDS` 作为目录窗口简写。这不是定时线程。M105 间隔水位、M104 一次性 `AT`（执行后删除）与 M99 的 `SHOW CREATE USER` 行为不变。

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```

### 事件 DEFINER / ON COMPLETION（M107）

```sql
CREATE DEFINER=`app`@`%` EVENT e ON SCHEDULE AT '2000-01-01 00:00:00' ON COMPLETION PRESERVE DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SHOW EVENTS LIKE 'e';
SHOW CREATE EVENT e;
ALTER EVENT e ON COMPLETION NOT PRESERVE;
```

`DEFINER`（`user@host`；省略则为会话用户/主机）与 `ON COMPLETION`（`PRESERVE` / `NOT PRESERVE`；省略则为 `NOT PRESERVE`）持久化到 `{data_dir}/programs.json`。`SHOW EVENTS` 的 `Definer` 来自目录。`SHOW CREATE EVENT` 重建这两个子句。到期 ENABLED `AT` 在 `NOT PRESERVE` 时执行后仍删除（M104）。`PRESERVE` 保留目录行并设为 `DISABLED`。这不是 COMMENT / `DISABLE ON SLAVE` / `SHOW EVENTS` last-executed。M106 `STARTS`/`ENDS`、M105 水位与 M99 的 `SHOW CREATE USER` 行为不变。

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server event_scheduler
```

### SHOW STATUS（M86）

```sql
SHOW STATUS;
SHOW SESSION STATUS;
SHOW GLOBAL STATUS;
SHOW STATUS LIKE 'Threads%';
SHOW STATUS LIKE 'not_a_real_status%';
```

面向客户端/监控探测的文档化 stub 目录（`Variable_name`、`Value`）：`Uptime`（`0`）、`Threads_connected`（进程列表可用时为当前连接数，否则为 `1`）、`Threads_running`（`1`）、`Questions`（`0`）、`Slow_queries`（`0`）、`Open_tables`（`0`）、`Connections`（`1`）、`Aborted_connects`（`0`）、`Bytes_received`（`0`）、`Bytes_sent`（`0`）。对本切片 `SHOW SESSION STATUS` 与 `SHOW GLOBAL STATUS` 返回相同行。`LIKE` 过滤该集合；不匹配的模式返回空结果。这不是完整的 MySQL 8.0 目录，也不是实时 InnoDB/`performance_schema` 计数器。未实现 `FLUSH STATUS` 与 `SHOW ENGINE INNODB STATUS`。`SELECT … FOR UPDATE` 与 `SET TRANSACTION ISOLATION LEVEL` 行为不变。

```bash
cargo test -p rusql-executor show_status
cargo test -p rusql-server show_status
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
