# Release Notes

What landed on `main` and how to verify it. For day-to-day usage see [user-guide.md](user-guide.md).

**中文**: [release-notes.md](../zh-CN/release-notes.md)

---

## Latest: M98 SHOW FUNCTION STATUS stubs (2026-09-18)

**What**: `SHOW FUNCTION STATUS` (optional `LIKE`) returns MySQL-shaped columns (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from catalog `FunctionMeta`; `Type` is `FUNCTION`. Definer / timestamps / charset are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. This is not live DEFINER persistence. `SHOW PROCEDURE STATUS` from M97, `SHOW CREATE FUNCTION` from M96, and `SHOW CREATE PROCEDURE` from M95 are unchanged.

```bash
cargo test -p rusql-sql show_function_status
cargo test -p rusql-executor show_function_status
cargo test -p rusql-server show_function_status
```

---

## Latest: M97 SHOW PROCEDURE STATUS stubs (2026-09-18)

**What**: `SHOW PROCEDURE STATUS` (optional `LIKE`) returns MySQL-shaped columns (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from catalog `ProcedureMeta`; `Type` is `PROCEDURE`. Definer / timestamps / charset are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. This is not live DEFINER persistence and not `SHOW FUNCTION STATUS`. `SHOW CREATE FUNCTION` from M96, `SHOW CREATE PROCEDURE` from M95, and `SHOW CREATE TRIGGER` from M94 are unchanged.

```bash
cargo test -p rusql-sql show_procedure_status
cargo test -p rusql-executor show_procedure_status
cargo test -p rusql-server show_procedure_status
```

---

## Latest: M96 SHOW CREATE FUNCTION stubs (2026-09-18)

**What**: `SHOW CREATE FUNCTION` returns MySQL-shaped columns (`Function`, `sql_mode`, `Create Function`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Function` cell is reconstructed from catalog `FunctionMeta` (`CREATE FUNCTION \`f\`() RETURNS … BEGIN RETURN … END` with an empty parameter list). `sql_mode` / charset are documented stubs (empty `sql_mode`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown functions return errno 1305. This is not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE PROCEDURE` from M95, `SHOW CREATE TRIGGER` from M94, and `SHOW CREATE VIEW` from M92 are unchanged.

```bash
cargo test -p rusql-sql show_create_function
cargo test -p rusql-executor show_create_function
cargo test -p rusql-server show_create_function
```

---

## Latest: M95 SHOW CREATE PROCEDURE stubs (2026-09-17)

**What**: `SHOW CREATE PROCEDURE` returns MySQL-shaped columns (`Procedure`, `sql_mode`, `Create Procedure`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Procedure` cell is reconstructed from catalog `ProcedureMeta` (`CREATE PROCEDURE \`p\`() BEGIN … END` with an empty parameter list). `sql_mode` / charset are documented stubs (empty `sql_mode`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown procedures return errno 1305. This is not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE TRIGGER` from M94, `SHOW TRIGGERS` from M93, and `SHOW CREATE VIEW` from M92 are unchanged.

```bash
cargo test -p rusql-sql show_create_procedure
cargo test -p rusql-executor show_create_procedure
cargo test -p rusql-server show_create_procedure
```

---

## Latest: M94 SHOW CREATE TRIGGER stubs (2026-09-17)

**What**: `SHOW CREATE TRIGGER` returns MySQL-shaped columns (`Trigger`, `sql_mode`, `SQL Original Statement`, `character_set_client`, `collation_connection`, `Database Collation`, `Created`). The `SQL Original Statement` cell is reconstructed from catalog `TriggerMeta` (`CREATE TRIGGER \`t\` {BEFORE|AFTER} {INSERT|UPDATE|DELETE} ON \`table\` FOR EACH ROW …`). `sql_mode` / charset / `Created` are documented stubs (empty `sql_mode`/`Created`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown triggers return errno 1360. This is not DEFINER / sql_mode dump and not a full mysqldump. `SHOW TRIGGERS` from M93, `SHOW CREATE VIEW` from M92, and `SHOW CREATE TABLE` from M13 are unchanged.

```bash
cargo test -p rusql-sql show_create_trigger
cargo test -p rusql-executor show_create_trigger
cargo test -p rusql-server show_create_trigger
```

---

## Latest: M93 SHOW TRIGGERS stubs (2026-09-17)

**What**: `SHOW TRIGGERS` (optional `FROM`/`IN` db and `LIKE`) returns MySQL-shaped columns (`Trigger`, `Event`, `Table`, `Statement`, `Timing`, `Created`, `sql_mode`, `Definer`, `character_set_client`, `collation_connection`, `Database Collation`). Catalog cells come from M48 `TriggerMeta`; Definer / sql_mode / charset are documented stubs (`root@%`, empty `Created`/`sql_mode`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. Unknown `FROM` databases return errno 1049. This is not `SHOW CREATE TRIGGER` and not live DEFINER persistence. `SHOW CREATE VIEW` from M92, `SHOW CREATE TABLE` from M13, and `SHOW CREATE DATABASE` from M91 are unchanged.

```bash
cargo test -p rusql-sql show_triggers
cargo test -p rusql-executor show_triggers
cargo test -p rusql-server show_triggers
```

---

## Latest: M92 SHOW CREATE VIEW stubs (2026-09-17)

**What**: `SHOW CREATE VIEW` returns MySQL-shaped columns (`View`, `Create View`, `character_set_client`, `collation_connection`). The `Create View` cell is reconstructed from the catalog SELECT (`CREATE VIEW \`v\` AS …`). `character_set_client` / `collation_connection` are documented stubs (`utf8mb4` / `utf8mb4_unicode_ci`). Unknown views return errno 1146. This is not ALGORITHM / DEFINER / SQL SECURITY and not a full mysqld dump. `SHOW CREATE TABLE` from M13, `SHOW CREATE DATABASE` from M91, and `SHOW WARNINGS` from M90 are unchanged.

```bash
cargo test -p rusql-sql show_create_view
cargo test -p rusql-executor show_create_view
cargo test -p rusql-server show_create_view
```

---

## Latest: M91 SHOW CREATE DATABASE stubs (2026-09-17)

**What**: `SHOW CREATE DATABASE` and `SHOW CREATE SCHEMA` return MySQL-shaped columns (`Database`, `Create Database`) with documented stub DDL that includes `utf8mb4` and rusql's default collation `utf8mb4_unicode_ci`. Unknown databases return errno 1049. This is not a live per-schema charset catalog, not `CREATE DATABASE … CHARACTER SET` / `COLLATE`, and not a full mysqld dump. `SHOW CREATE TABLE` from M13 and `SHOW WARNINGS` from M90 are unchanged.

```bash
cargo test -p rusql-sql show_create_database
cargo test -p rusql-executor show_create_database
cargo test -p rusql-server show_create_database
```

---

## Latest: M90 SHOW WARNINGS stubs (2026-09-17)

**What**: `SHOW WARNINGS` and `SHOW ERRORS` return MySQL-shaped columns (`Level`, `Code`, `Message`) as a documented empty diagnostic list. After a successful statement (for example `SELECT 1`) the result is zero rows, not an unsupported-statement error. This is not live truncation / sql_mode / note generation. `SHOW COUNT(*) WARNINGS`, `LIMIT`, and `WHERE` are not implemented. `SHOW CHARACTER SET` from M89 and `SHOW ENGINES` from M88 are unchanged.

```bash
cargo test -p rusql-sql warnings
cargo test -p rusql-executor warnings
cargo test -p rusql-server warnings
```

---

## Latest: M89 SHOW CHARACTER SET stubs (2026-09-17)

**What**: `SHOW CHARACTER SET` and `SHOW CHARSET` return MySQL-shaped columns (`Charset`, `Description`, `Default collation`, `Maxlen`) for a documented stub set: `utf8mb4` (`Maxlen` = `4`, `Default collation` = `utf8mb4_unicode_ci`, matching M87 `SHOW TABLE STATUS` `Collation`). `SHOW CHARACTER SET LIKE 'utf8%'` filters that set; a non-matching pattern returns zero rows. This is not the full MySQL 8.0 charset catalog and not wire encoding conversion. `SHOW COLLATION` from M59, `SHOW ENGINES` from M88, and `SET CHARACTER SET` from M83 are unchanged.

```bash
cargo test -p rusql-sql character_set
cargo test -p rusql-executor character_set
cargo test -p rusql-server character_set
```

---

## Latest: M88 SHOW ENGINES stubs (2026-09-17)

**What**: `SHOW ENGINES` and `SHOW STORAGE ENGINES` return MySQL-shaped columns (`Engine`, `Support`, `Comment`, `Transactions`, `XA`, `Savepoints`) for a documented stub set: `InnoDB` (`Support` = `DEFAULT`, matching M87 `SHOW TABLE STATUS`), `MEMORY`, `MyISAM`, and `PERFORMANCE_SCHEMA` (`YES`). Comments and YES/NO flags are constants, not live plugins. rusql does not switch engines. This is not the full MySQL 8.0 engine catalog. `SHOW TABLE STATUS` from M87 and `SHOW STATUS` from M86 are unchanged. `SHOW ENGINE INNODB STATUS` is not implemented.

```bash
cargo test -p rusql-sql show_engines
cargo test -p rusql-executor show_engines
cargo test -p rusql-server show_engines
```

---

## Latest: M87 SHOW TABLE STATUS stubs (2026-09-17)

**What**: `SHOW TABLE STATUS` (and `SHOW TABLE STATUS LIKE '…'`, optional `FROM`/`IN` db) returns one row per table in the current database with MySQL-shaped columns (`Name`, `Engine`, `Version`, `Row_format`, `Rows`, `Avg_row_length`, `Data_length`, `Max_data_length`, `Index_length`, `Data_free`, `Auto_increment`, `Create_time`, `Update_time`, `Check_time`, `Collation`, `Checksum`, `Create_options`, `Comment`). `Name` matches `SHOW TABLES`. `Engine` is the documented stub `InnoDB`; `Version`/`Row_format` are `10`/`Dynamic`; `Rows` is the current heap row count; `Auto_increment` follows the table counter when the table has one; `Collation` is `utf8mb4_unicode_ci`. Other numeric/time/comment cells are `0` or empty. This is not live InnoDB file-per-table stats. A non-matching `LIKE` returns zero rows. `SHOW STATUS` from M86 is unchanged. `SHOW ENGINE INNODB STATUS` is not implemented.

```bash
cargo test -p rusql-sql table_status
cargo test -p rusql-executor table_status
cargo test -p rusql-server table_status
```

---

## Latest: M86 SHOW STATUS stubs (2026-09-17)

**What**: `SHOW STATUS`, `SHOW SESSION STATUS`, and `SHOW GLOBAL STATUS` return `Variable_name`/`Value` rows for a documented stub set: `Uptime`, `Threads_connected`, `Threads_running`, `Questions`, `Slow_queries`, `Open_tables`, `Connections`, `Aborted_connects`, `Bytes_received`, `Bytes_sent`. Values are stable constants (`0` or `1`) except `Threads_connected`, which is the current connection-registry count. `SHOW STATUS LIKE 'Threads%'` filters that set; a non-matching pattern returns zero rows. Session and global lists are the same for this slice. This is not the full MySQL 8.0 catalog / `performance_schema`. `SELECT … FOR UPDATE` from M85 and `SET TRANSACTION ISOLATION LEVEL` from M84 are unchanged.

```bash
cargo test -p rusql-executor show_status
cargo test -p rusql-server show_status
```

---

## Latest: M85 SELECT … FOR UPDATE (2026-09-16)

**What**: `SELECT … FOR UPDATE`, `FOR SHARE`, and `LOCK IN SHARE MODE` (plus `NOWAIT` / `SKIP LOCKED`) are accepted and return the same rows as the unlocked `SELECT`. rusql does not take row locks, wait, or skip locked rows. Two connections can both `SELECT … FOR UPDATE` the same row. `SET TRANSACTION ISOLATION LEVEL` overlays from M84 are unchanged; DML stays snapshot isolation.

```bash
cargo test -p rusql-executor for_update
cargo test -p rusql-server for_update
```

---

## Latest: M84 SET TRANSACTION ISOLATION LEVEL (2026-09-16)

**What**: `SET TRANSACTION ISOLATION LEVEL …` and `SET SESSION TRANSACTION ISOLATION LEVEL …` overlay `@@transaction_isolation` and `@@tx_isolation` on that connection (hyphenated MySQL names: `READ-COMMITTED`, `REPEATABLE-READ`, `SERIALIZABLE`, `READ-UNCOMMITTED`). `SHOW VARIABLES` / `SHOW SESSION VARIABLES` see the overlay; other connections and `SHOW GLOBAL VARIABLES` keep `REPEATABLE-READ`. rusql treats next-transaction `SET TRANSACTION` the same as `SET SESSION TRANSACTION` (in-memory overlay only; DML stays snapshot isolation). `SET GLOBAL TRANSACTION` is rejected (errno 1229). Overlays are not WAL. `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore defaults. `SET NAMES`, `SET CHARACTER SET`, and `@foo :=` from M82/M83 are unchanged.

```bash
cargo test -p rusql-executor set_transaction
cargo test -p rusql-server set_transaction
```

---

## Latest: M83 SET CHARACTER SET / SELECT @foo := expr (2026-09-16)

**What**: `SET CHARACTER SET charset` and `SET CHARSET charset` overlay the same `@@character_set_client` / `connection` / `results` stubs as `SET NAMES` (packet encoding is unchanged). `SELECT @foo := expr` assigns on that connection and returns the value; a later `SELECT @foo` sees it. Unset user variables remain an empty cell (NULL). Overlays are in-memory (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` still clear user variables and restore charset stubs. `SET NAMES` / `SET @foo = expr` from M82 are unchanged.

```bash
cargo test -p rusql-executor set_charset
cargo test -p rusql-executor user_var
cargo test -p rusql-server set_charset
cargo test -p rusql-server user_var
```

---

## Latest: M82 SET NAMES / user variables @foo (2026-09-16)

**What**: `SET NAMES charset [COLLATE collation]` overlays `@@character_set_client` / `connection` / `results` (and `@@collation_connection` when `COLLATE` is given, or `utf8mb4_0900_ai_ci` for `utf8mb4`). `SET NAMES DEFAULT` restores documented charset stubs. `SET @foo = expr` then `SELECT @foo` returns the value on that connection; unset `@bar` is an empty cell (NULL). Overlays are in-memory (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear user variables and restore `SET NAMES` defaults. Packet encoding is unchanged. `SELECT @foo := expr` assignment expressions remain unimplemented.

```bash
cargo test -p rusql-executor set_names
cargo test -p rusql-executor user_var
cargo test -p rusql-server set_names
cargo test -p rusql-server user_var
```

---


**What**: `SET @@var`, `SET @@session.var`, `SET SESSION var`, and `SET var` overlay the documented M77+M79 stub catalog on that connection. `SELECT @@var` / `SELECT @@session.var` and `SHOW VARIABLES` / `SHOW SESSION VARIABLES` see the new value; `SHOW GLOBAL VARIABLES` and other connections keep documented defaults. Overlays are in-memory (not WAL). `COM_RESET_CONNECTION` and `COM_CHANGE_USER` restore defaults. Unknown names still return errno 1193. `SET GLOBAL` is rejected (errno 1229). Read-only stubs `version`, `version_comment`, `license`, and `system_time_zone` reject SET (errno 1238). Autocommit DML behavior is unchanged (still autocommit-on). `SET NAMES` and user variables `@foo` remain unimplemented.

```bash
cargo test -p rusql-executor set_session_var
cargo test -p rusql-server set_session_var
```

---

## Latest: M80 SHOW VARIABLES stub catalog (2026-09-16)

**What**: `SHOW VARIABLES`, `SHOW SESSION VARIABLES`, and `SHOW GLOBAL VARIABLES` return `Variable_name`/`Value` rows for the documented M77+M79 stub set (including `tx_isolation`). `SHOW VARIABLES LIKE 'auto_increment%'` filters that set; a non-matching pattern returns zero rows. Session and global lists are the same for this slice (stubs are not persisted). This is not the full MySQL 8.0 catalog. `SET @@` and user variables `@foo` remain unimplemented.

```bash
cargo test -p rusql-executor show_variables
cargo test -p rusql-server show_variables
```

---

## Latest: M79 more @@ connector probes (2026-09-15)

**What**: JDBC/ORM handshake stubs in addition to M77: `@@auto_increment_increment` (`1`), `@@time_zone` (`SYSTEM`), `@@system_time_zone` (`UTC`, not the host TZ), `@@transaction_isolation` / `@@tx_isolation` (`REPEATABLE-READ`), `@@max_allowed_packet` (`67108864`), `@@license` (`GPL`). `@@session.var` equals `@@var`. Unknown names still return errno 1193. `SET @@`, `@foo`, and `SHOW VARIABLES` are not implemented.

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-server session_var
```

---

## Latest: M78 FOUND_ROWS() / SQL_CALC_FOUND_ROWS (2026-09-15)

**What**: `SELECT FOUND_ROWS()` after a plain `SELECT` returns that result’s row count (after `LIMIT`). `SELECT SQL_CALC_FOUND_ROWS … LIMIT n` then `FOUND_ROWS()` returns the un-LIMITed match count (deprecated in MySQL 8.0.17; `COUNT(*)` remains the supported alternative). After DML it matches `ROW_COUNT()`. Session-scoped; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset to `0`.

```bash
cargo test -p rusql-executor found_rows
cargo test -p rusql-server found_rows
```

---

## Latest: M77 @@ session variables (2026-09-15)

**What**: `SELECT @@version` matches `VERSION()` (`8.0.33-rusql`). Documented stubs: `@@version_comment`, `@@autocommit` (`1`), `@@character_set_client`/`connection`/`results`/`server` (`utf8mb4`), `@@collation_connection` (`utf8mb4_0900_ai_ci`), `@@sql_mode` (MySQL 8.0-like string, not enforced). `@@session.var` equals `@@var` for this set. Unknown names return errno 1193. `SET @@`, `@foo`, and `SHOW VARIABLES` are not implemented.

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-server session_var
```

---

## Latest: M76 CONNECTION_ID() / ROW_COUNT() (2026-09-14)

**What**: `SELECT CONNECTION_ID()` returns the connection’s handshake thread id (stable for the session; same value as `SHOW PROCESSLIST` `Id` / `COM_PROCESS_INFO`). `SELECT ROW_COUNT()` returns the affected-row count of the last `INSERT`/`UPDATE`/`DELETE` on that connection, and `-1` after a statement that returns a result set (MySQL). Connections do not share `ROW_COUNT()`; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset it to `-1`. `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` are not implemented.

```bash
cargo test -p rusql-executor connection_id
cargo test -p rusql-executor row_count
cargo test -p rusql-server connection_id
cargo test -p rusql-server row_count
```

---

## Latest: M75 LAST_INSERT_ID() (2026-09-14)

**What**: `SELECT LAST_INSERT_ID()` returns the first generated `AUTO_INCREMENT` value from the last successful `INSERT` on that connection (`0` if none). The INSERT OK packet `last_insert_id` field matches. Explicit non-generated inserts (and `LAST_INSERT_ID(expr)` setter) are not tracked. Connections do not share the value; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear it.

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```

---

## Latest: M74 UPDATE/DELETE row events (2026-09-14)

**What**: Committed `UPDATE`/`DELETE` write `TABLE_MAP_EVENT` then `UPDATE_ROWS_EVENT_V1` (type 24) / `DELETE_ROWS_EVENT_V1` (type 25). Cells are UTF-8 with a u32 length prefix (same as M72 INSERT). `extract`/`apply_binlog_file` reconstruct `UPDATE`/`DELETE` SQL. INSERT row events and live dump follow are unchanged. Replica tables must already exist.

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M73 live COM_BINLOG_DUMP follow (2026-09-14)

**What**: `COM_BINLOG_DUMP` with flags `0` streams existing events from the requested position and stays open (no OK). Later `COMMIT`s are written as additional `0x00` + event packets (INSERT `TABLE_MAP` + `WRITE_ROWS`, or UPDATE/DELETE `QUERY_EVENT`). `BINLOG_DUMP_NON_BLOCK` (flags `0x01`) keeps the M71 one-shot dump then OK. Client disconnect or `COM_QUIT` ends follow.

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M72 TABLE_MAP + WRITE_ROWS for INSERT (2026-09-14)

**What**: Committed `INSERT`s write `TABLE_MAP_EVENT` (type 19) then `WRITE_ROWS_EVENT_V1` (type 23) after `FORMAT_DESCRIPTION`. Cells are UTF-8 with a u32 length prefix; empty cells are SQL `NULL`. `extract`/`apply_binlog_file` reconstruct `INSERT INTO … VALUES (…)`. UPDATE/DELETE stay QUERY_EVENT. `COM_BINLOG_DUMP` still sends one packet per event (M71). Replica tables must already exist (DDL is not in the binlog).

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M71 per-event COM_BINLOG_DUMP (2026-09-14)

**What**: `COM_BINLOG_DUMP` sends each binlog event as its own packet (`0x00` + event bytes) starting at the requested file position. Position `4` yields `FORMAT_DESCRIPTION` first; committed `INSERT`s appear as `QUERY_EVENT`s. One-shot dump of the current file (no live follow).

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M70 window ranking functions (2026-09-14)

**What**: `ROW_NUMBER()`, `RANK()`, and `DENSE_RANK()` as top-level `SELECT` items with `OVER (PARTITION BY … ORDER BY …)`. `WHERE` runs before the window; outer `ORDER BY` / `LIMIT` after. Window frames (`ROWS`/`RANGE`) and other window functions are rejected.

```bash
cargo test -p rusql-executor window
cargo test -p rusql-server window
```

---

## Latest: M69 non-recursive WITH (CTE) (2026-09-14)

**What**: `WITH cte AS (SELECT …) SELECT … FROM cte` names a subquery. Multiple CTEs can chain (later CTEs may read earlier ones). `WITH RECURSIVE` is rejected.

```bash
cargo test -p rusql-executor with_cte
cargo test -p rusql-server with_cte
```

---

## Latest: M68 INSERT … SELECT / ON DUPLICATE KEY UPDATE (2026-09-14)

**What**: `INSERT INTO dst SELECT … FROM src` copies query results. Duplicate single-column `PRIMARY KEY` returns errno 1062 unless `ON DUPLICATE KEY UPDATE` upserts (`VALUES(col)` and existing-row expressions such as `cnt = cnt + 1`).

```bash
cargo test -p rusql-executor insert_select
cargo test -p rusql-executor on_duplicate
cargo test -p rusql-server insert_select
```

---

## Latest: M67 SELECT DISTINCT (2026-09-07)

**What**: `SELECT DISTINCT` removes duplicate projected rows (MySQL semantics). Applies after projection and before `ORDER BY` / `LIMIT`. `DISTINCT ON` is rejected.

```bash
cargo test -p rusql-executor select_distinct
cargo test -p rusql-server select_distinct
```

---

## Latest: M66 CASE / IF expressions (2026-09-07)

**What**: Searched and simple `CASE … END`, plus `IF(cond, then, else)`, in `SELECT` projections (with or without `FROM`).

```bash
cargo test -p rusql-executor case_searched
cargo test -p rusql-server case_and_if
```

---

## Latest: M65 session info functions (2026-09-07)

**What**: `DATABASE()` / `SCHEMA()`, `USER()` / `CURRENT_USER()` / `SESSION_USER()`, and `VERSION()` return session metadata (MySQL 8.0-compatible `8.0.33-rusql` version string).

```bash
cargo test -p rusql-executor session_info
cargo test -p rusql-server session_info_functions
```

---

## Latest: M62 utf8mb4_0900_ai_ci (2026-09-02)

**What**: `utf8mb4_0900_ai_ci` collation for `CREATE TABLE … COLLATE`, `ORDER BY`, and `WHERE =`; listed in `SHOW COLLATION` alongside `utf8mb4_unicode_ci`.

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation_0900
```

---

## Latest: M63 CREATE FUNCTION (2026-09-01)

**What**: `CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END`, `DROP FUNCTION`, scalar calls in `SELECT f()` and `SELECT f() + 1`; `information_schema.ROUTINES` shows `ROUTINE_TYPE = FUNCTION`.

```bash
cargo test -p rusql-sql parse_create_function
cargo test -p rusql-executor create_function
```

---

## Latest: M64 AFTER UPDATE/DELETE triggers (2026-09-01)

**What**: `CREATE TRIGGER … AFTER UPDATE` and `AFTER DELETE` fire side-effect DML with `OLD.col` / `NEW.col` substitution (audit-table pattern).

```bash
cargo test -p rusql-executor trigger
```

---

## Latest: P3 stored programs + replication MVP (2026-09-01)

**What**: `CREATE PROCEDURE` / `CALL` / `DROP PROCEDURE`, `CREATE TRIGGER` (BEFORE INSERT with `SET NEW.col`), binlog QUERY events on `COMMIT` with GTID comment stub, `COM_BINLOG_DUMP` / `COM_REGISTER_SLAVE`, `apply_binlog_file` replica applier, `SHOW MASTER/SLAVE STATUS` GTID stubs.

```bash
cargo test -p rusql-sql stored_programs
cargo test -p rusql-executor programs
cargo test -p rusql-storage binlog
cargo test -p rusql-storage replica
```

---

## Latest: M59 collation + M61 Sysbench (2026-09-01)

**What**: `utf8mb4_unicode_ci` compare/sort for `ORDER BY` and `WHERE =`; Sysbench `oltp_point_select` harness vs Docker MySQL.

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation_order_by
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
```

---

## Latest: PERF-B2/B3 query and DML optimizations (2026-09-01)

**What**: Index-ordered scan for `ORDER BY` + `LIMIT` (no `WHERE`); PK-targeted `UPDATE` with incremental index maintenance.

```bash
cargo test -p rusql-storage scan_index_ordered_with_limit pk_update_without_index_rebuild
cargo test -p rusql-executor select_order_by_indexed_limit update_pk_by_index
```

---

## Latest: PERF-B4–B6 performance harness (2026-09-01)

**What**: Multi-thread benchmark (`--threads`, `--thread-matrix`), `--wal-sync` policy, optional Sysbench CI gate.

```bash
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
```

---

## Latest: M51–M53 wire protocol commands (2026-09-01)

**What**: `COM_CHANGE_USER`, `COM_RESET_CONNECTION`, `COM_FIELD_LIST`, prepared-statement long data/reset, and `SHOW PROCESSLIST` / `COM_PROCESS_INFO`.

```bash
cargo test -p rusql-protocol
cargo test -p rusql-server show_processlist
cargo test -p rusql-server com_field_list
cargo test -p rusql-server com_change_user
```

---

## Latest: PERF-B4/B5/B6 concurrency, WAL sync, Sysbench (2026-09-01)

**What**: Multi-threaded benchmark harness, configurable WAL `fsync` policy, and optional Sysbench `oltp_point_select` gate.

```bash
# Multi-thread benchmark
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308

# WAL sync policy
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
cargo test -p rusql-storage wal_sync_none

# Sysbench gate (soft-fail if tools missing)
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
```

---

## Latest: PERF-B1 persistent-connection benchmark (2026-09-01)

**What**: `scripts/bench-rusql-vs-mysql.mjs` runs the same 7 workloads as the 2026-08-11 CLI baseline using one persistent wire client (no per-query process spawn).

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql --output target/bench-rusql.json
```

---

## M55-auth multi-user accounts (2026-09-01)

**What**: `CREATE USER` / `DROP USER` with passwords persisted in `mysql.user.json`; login as non-root users via `caching_sha2_password` or `mysql_native_password`.

```bash
cargo test -p rusql-core parse_create_user_ddl
cargo test -p rusql-server auth
```

**Try it** (dev server, then as root):

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
```

Connect with `--default-auth=mysql_native_password` and `-u app -p`.

---

## M50 composite indexes (2026-09-01)

**What**: Multi-column `CREATE INDEX idx ON t (a, b)` with prefix equality lookups, composite EXPLAIN plans, and MySQL-style `Seq_in_index` in SHOW INDEX / `information_schema.STATISTICS`.

```bash
cargo test -p rusql-storage composite_index_lookup
cargo test -p rusql-planner composite_eq
cargo test -p rusql-server run_basic_compat
```

---

## M54 GRANT/REVOKE (2026-08-31)

**What**: MySQL-style privilege grants persisted to `mysql.user.json` in the data directory; `GRANT`/`REVOKE`/`SHOW GRANTS`; unauthorized DML returns errno **1142**. User `root` bypasses all checks.

```bash
cargo test -p rusql-core privileges
cargo test -p rusql-server run_basic_compat
```

Example:

```sql
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;
REVOKE INSERT ON rusql.* FROM app;
```

---

## M39 FOREIGN KEY + M44 UNION (2026-08-31)

**What**: `UNION`/`UNION ALL` result combination; `FOREIGN KEY` on `CREATE TABLE` with RESTRICT enforcement (MySQL errno 1451/1452) and `information_schema.KEY_COLUMN_USAGE`.

```bash
cargo test -p rusql-executor foreign_key
cargo test -p rusql-server compat_suite
node scripts/mysql-diff.mjs   # requires Docker
```

---

## Latest: P1 SQL gaps M40–M60 (2026-08-31)

**What**: Extended column types, outer joins, GROUP BY/HAVING/aggregates, subqueries, SQL expressions, and 100-case mysql-test wire harness with CI pass floor.

```bash
cargo test -p rusql-executor aggregate
cargo test -p rusql-server mysql_test_subset
node scripts/mysql-diff.mjs   # requires Docker
```

---

## Latest: Issue #73 — metadata EOF + SESSION_TRACK (2026-07-06)

**What**: Completes MySQL 8.0 CLI compat after PR #78. Text/binary resultsets now send metadata EOF/OK between column definitions and rows (#79). OK packets include an empty session-state trailer when `CLIENT_SESSION_TRACK` is negotiated (#80). Command-phase OK responses honor client capabilities.

```bash
cargo test -p rusql-protocol response::tests
cargo test -p rusql-server mysql_cli
node scripts/mysql-diff.mjs   # requires Docker; CI uses apt mysql client
```

---

## Issue #73 — MySQL 8.0 CLI COM_QUERY compat (2026-07-06)

**What**: Official `mysql:8.0` clients negotiate `CLIENT_QUERY_ATTRIBUTES` and `CLIENT_DEPRECATE_EOF`. rusql now strips the query-attributes preamble on `COM_QUERY` and ends text/binary resultsets with an OK packet instead of a legacy EOF when required.

```bash
cargo test -p rusql-protocol command::tests
cargo test -p rusql-server mysql_cli_query_attributes
node scripts/mysql-diff.mjs   # requires Docker
```

---

## M31 — Durable COMMIT WAL (2026-06-30)

**What**: `COMMIT` appends pending transaction records to `rusql.wal` with `sync_data`; `ROLLBACK` discards overlay without WAL writes. Verified across storage replay and wire-protocol tests.

```bash
cargo test -p rusql-storage commit_transaction_survives
cargo test -p rusql-server transaction
```

---

## M30 — mysql-test subset (2026-06-30)

**What**: 12 Oracle mysql-test inspired wire cases in `tests/mysql-test/manifest.json`, run via internal test client. Skips documented in `tests/mysql-test/SKIPS.md`.

```bash
node scripts/mysql-test-subset.mjs
cargo test -p rusql-server mysql_test_subset
```

---

## M29 — mysql-diff runner (2026-06-30)

**What**: `node scripts/mysql-diff.mjs` compares portable SQL in `compat/mysql-diff.json` against Docker MySQL 8.0 and rusql-server (skips without Docker).

```bash
node scripts/mysql-diff.mjs
```

---

## M28 — SHOW INDEX (2026-06-30)

**What**: `SHOW INDEX FROM tbl` (also `SHOW INDEXES`, `SHOW KEYS`) lists PRIMARY and secondary indexes with MySQL column names.

```bash
cargo test -p rusql-sql show_index
cargo test -p rusql-executor show_index
cargo test -p rusql-server compat
```

---

## M27 — information_schema SCHEMATA & STATISTICS (2026-06-30)

**What**: `SELECT * FROM information_schema.SCHEMATA` and `STATISTICS` (PRIMARY + secondary indexes).

```bash
cargo test -p rusql-executor info_schema_schemata
cargo test -p rusql-server compat
```

---

## M26 — caching_sha2 RSA full auth (2026-06-30)

**What**: Non-TLS clients can complete `caching_sha2_password` via RSA public-key exchange when `--auth-password` is set.

```bash
cargo test -p rusql-server accepts_caching_sha2_rsa
cargo test -p rusql-protocol rsa_password_roundtrip
```

---

## M25 — Binary resultset (COM_STMT_EXECUTE) (2026-06-30)

**What**: Prepared-statement SELECT returns binary protocol rows with correct MySQL column types (`INT` as 4-byte LE, `VARCHAR` as lenenc string).

```bash
cargo test -p rusql-protocol binary
cargo test -p rusql-server stmt_prepare_execute_binary
```

---

## M24 — ALTER TABLE ADD COLUMN (2026-06-30)

**What**: `ALTER TABLE t ADD COLUMN c TYPE` (and MySQL shorthand `ADD c TYPE`); existing rows get NULL (empty string) in the new column; WAL replay.

```bash
cargo test -p rusql-executor alter_table_add_column
cargo test -p rusql-server compat
```

---

## M23 — PRIMARY KEY metadata (2026-06-30)

**What**: `PRIMARY KEY` and `NOT NULL` stored in catalog; shown in DESCRIBE / SHOW CREATE TABLE.

```bash
cargo test -p rusql-executor describe_primary_key
cargo test -p rusql-server compat
```

---

## M22 — INNER JOIN (2026-06-30)

**What**: `SELECT ... FROM a INNER JOIN b ON a.col = b.col` (two tables).

```bash
cargo test -p rusql-executor inner_join_two_tables
cargo test -p rusql-server compat
```

---

## M21 — IS NULL / IS NOT NULL (2026-06-30)

**What**: `WHERE col IS NULL` and `IS NOT NULL`; `INSERT … NULL` supported.

```bash
cargo test -p rusql-executor where_is_null
cargo test -p rusql-server compat
```

---

## M20 — WHERE comparisons and AND (2026-06-30)

**What**: `WHERE id > 1`, `id <> 2`, `id = 1 AND name = 'x'` on table SELECT.

```bash
cargo test -p rusql-executor where_comparisons_and
cargo test -p rusql-server compat
```

---

## M19 — SELECT LIMIT OFFSET (2026-06-30)

**What**: `LIMIT n OFFSET m` after ORDER BY / projection on table SELECT.

```sql
SELECT * FROM users ORDER BY id LIMIT 1 OFFSET 1;
```

```bash
cargo test -p rusql-executor select_limit
cargo test -p rusql-server compat
```

---

## M18 — SELECT column aliases (2026-06-30)

**What**: Result set column headers use `AS` aliases (e.g. `SELECT id AS user_id`).

```sql
SELECT id AS user_id FROM users;
```

```bash
cargo test -p rusql-executor select_column_aliases
cargo test -p rusql-server compat
```

---

## M17 — SELECT ORDER BY (2026-06-30)

**What**: `ORDER BY col [ASC|DESC]` on table `SELECT` (after projection/filter, before `LIMIT`).

```sql
SELECT * FROM users ORDER BY id;
SELECT name FROM users ORDER BY name DESC;
```

```bash
cargo test -p rusql-executor select_order_by
cargo test -p rusql-server compat
```

---

## M16 — SELECT LIMIT (2026-06-30)

**What**: `SELECT * FROM tbl LIMIT n` caps result rows (with projection/WHERE).

```bash
cargo test -p rusql-executor select_limit
```

---

## M15 — USE database (2026-06-30)

**What**: `USE rusql` sets session default database; unknown DB names error.

```sql
USE rusql;
```

Note: `USE DATABASE name` is not parsed by our MySQL dialect yet; clients using `USE name` work.

```bash
cargo test -p rusql-executor use_database
cargo test -p rusql-server use_database
```

---

## M14 — SELECT column projection (2026-06-30)

**What**: `SELECT id, name FROM users` returns only listed columns; `SELECT *` unchanged.

**Try it**:

```sql
SELECT name FROM users;
SELECT id, name FROM users WHERE id = 1;
```

**Automated**:

```bash
cargo test -p rusql-executor select_column_projection
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## Book — Harness Engineering narrative (#28)

**What**: mdBook in English and zh-CN — one chapter per milestone (M0–M13), Harness Engineering part, metrics appendix.

**Read**: [docs/book/README.md](../../docs/book/README.md)

**Build**:

```bash
cargo install mdbook   # once
node scripts/build-book.mjs
node scripts/check-book.mjs
```

---

## M13 — SHOW CREATE TABLE (2026-06-30)

**What**: `SHOW CREATE TABLE tbl` returns `Table` and `Create Table` columns with reconstructable DDL.

**Try it**:

```sql
CREATE TABLE users (id INT, name VARCHAR(32));
SHOW CREATE TABLE users;
```

**Automated**:

```bash
cargo test -p rusql-executor show_create
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## M12 — DESCRIBE and information_schema (2026-06-30)

**What**: `DESCRIBE tbl`, `SHOW COLUMNS FROM tbl`, and virtual `information_schema.tables` / `information_schema.columns` for tooling.

**Try it**:

```sql
DESCRIBE users;
SHOW COLUMNS FROM users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
```

**Automated**:

```bash
cargo test -p rusql-executor describe
cargo test -p rusql-executor information_schema
cargo test -p rusql-server describe
cargo test -p rusql-server run_basic_compat_fixtures
```

Spec: [m12-describe-info-schema.md](specs/m12-describe-info-schema.md)

---

## M11 — Prepared statements (2026-06-30)

**What**: `COM_STMT_PREPARE`, `COM_STMT_EXECUTE`, `COM_STMT_CLOSE`. Supports `?` placeholders (text/VARCHAR params).

**Try it** (via wire tests):

```bash
cargo test -p rusql-server stmt_prepare
```

**Automated**:

```bash
cargo test -p rusql-protocol stmt::
cargo test -p rusql-server stmt_
```

Spec: [m11-stmt-prepare.md](specs/m11-stmt-prepare.md)

---

## M10 — SHOW TABLES / SHOW DATABASES (2026-06-30)

**What**: List tables and the default `rusql` database (MySQL-style result columns).

**Try it**:

```sql
CREATE TABLE users (id INT);
SHOW TABLES;
SHOW DATABASES;
```

**Automated**:

```bash
cargo test -p rusql-executor show_tables
cargo test -p rusql-server compat
```

---

## M9 — Transactions (2026-06-30)

**What**: Explicit `BEGIN`, `COMMIT`, and `ROLLBACK`. Uncommitted writes are visible only to the same connection until commit.

**Try it**:

```bash
cargo run -p rusql-server -- --port 3307
```

```sql
CREATE TABLE t (id INT);
BEGIN;
INSERT INTO t VALUES (1);
SELECT * FROM t;
COMMIT;
```

**Automated**:

```bash
cargo test -p rusql-server transaction_commit_and_rollback
cargo test -p rusql-server compat
```

Spec: [m9-transactions.md](specs/m9-transactions.md)

---

## M8 — UPDATE

`UPDATE table SET col = value [WHERE col = value]` with WAL persistence.

```bash
cargo test -p rusql-server compat
```

---

## M7 — caching_sha2_password

Default auth plugin for MySQL 8 clients. Optional `--auth-password` enables verification.

```bash
cargo test -p rusql-protocol
```

Spec: [adr-m7-caching-sha2.md](specs/adr-m7-caching-sha2.md)

---

## M6 — Auth + DROP / DELETE

Password verification and destructive DML.

Spec: [adr-m6-auth-and-dml.md](specs/adr-m6-auth-and-dml.md)

---

## M5 — Compatibility fixtures

JSON-driven wire tests in `crates/rusql-server/compat/basic.json`.

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## M4 — Indexes

`CREATE INDEX`, point lookup via `WHERE column = literal`.

---

## M3 — Persistence

`--data-dir` and WAL replay across restarts.

```bash
cargo test -p rusql-server persistence_across_connections
```

---

## M2 — COM_QUERY

SQL over MySQL wire protocol.

```bash
cargo test -p rusql-server com_query
```

---

## M1 — Handshake

MySQL v10 handshake.

```bash
cargo test -p rusql-protocol handshake
```

---

## M0 — Harness

Project bootstrap and CI sensors.

```bash
node scripts/harness-validate.mjs
node scripts/metrics.mjs
```

---

## Updating these notes

Every merged PR that changes user-visible behavior must:

1. Add a bullet under `CHANGELOG.md` → `[Unreleased]`
2. Add or update a **Latest** section here (move previous Latest down)
3. Update [user-guide.md](user-guide.md) test steps if needed

Sensor: `node scripts/check-changelog.mjs`
