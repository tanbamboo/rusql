# User Guide — Testing rusql

This guide describes **what works today** on `main` and how to verify it.

## Prerequisites

- Rust 1.75+ ([rustup](https://rustup.rs))
- Optional: MySQL client (`mysql` CLI) for manual testing

## Build

```bash
cargo build --release
```

## Run the server

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

- `--data-dir` — directory for the WAL file (`rusql.wal`). Default: `rusql-data`
- Data **survives restarts**: stop the server, start again, tables and rows are replayed from WAL

Default locale is `en-US`. For Chinese messages:

```bash
RUSQL_LOCALE=zh-CN cargo run -p rusql-server -- --port 3307
```

### Optional password verification

By default, any client password is accepted (dev mode). To enable verification (`caching_sha2_password` + `mysql_native_password`):

```bash
cargo run -p rusql-server -- --port 3307 --auth-password your_secret
```

Handshake advertises `caching_sha2_password` (MySQL 8 default). Legacy clients may still use `mysql_native_password`. See [adr-m7-caching-sha2.md](specs/adr-m7-caching-sha2.md).

## Automated tests (recommended)

Runs handshake + SQL over the wire without external tools:

```bash
cargo test -p rusql-server com_query
cargo test -p rusql-server compat
cargo test -p rusql-protocol
cargo test
```

### Compatibility fixture suite (M5)

JSON fixtures under `crates/rusql-server/compat/` drive end-to-end wire tests (CREATE/INSERT/SELECT/INDEX/WHERE). Add new cases by editing `basic.json` and running:

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```

## Manual test with MySQL client

After starting the server on port 3307:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

If your client defaults to `caching_sha2_password`, force native password (see [adr-auth-mvp.md](specs/adr-auth-mvp.md)):

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

MySQL 8 clients use `caching_sha2_password` by default. If needed:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --default-auth=mysql_native_password --protocol=TCP
```

Example session:

```sql
CREATE TABLE users (id INT, name VARCHAR(64));
CREATE INDEX idx_users_id ON users (id);
INSERT INTO users VALUES (1, 'alice');
SELECT * FROM users WHERE id = 1;
quit
```

Restart the server and run `SELECT * FROM users WHERE id = 1;` again — rows are still present.

### Transactions (M9)

```sql
BEGIN;
INSERT INTO users VALUES (2, 'bob');
SELECT * FROM users;
COMMIT;
```

Uncommitted changes are not visible to other connections. `ROLLBACK` discards the current transaction.

### Query SQL (M22–M46)

```sql
-- JOINs including OUTER (M41)
SELECT a.name, b.label FROM a LEFT JOIN b ON a.id = b.a_id;

-- GROUP BY / HAVING (M43)
SELECT dept, COUNT(*) AS cnt FROM emp GROUP BY dept HAVING cnt > 1;

-- Subqueries (M42)
SELECT id FROM t WHERE id IN (SELECT ref_id FROM refs);
SELECT id FROM t WHERE EXISTS (SELECT 1 FROM refs r WHERE r.t_id = t.id);
SELECT id, val FROM (SELECT id, val FROM t) AS d;

-- Expressions (M46 / M65 / M66 / M67 / M77)
SELECT id + 1, CONCAT(name, '!'), COALESCE(note, 'n/a'), LOWER(name) FROM t;
SELECT DATABASE(), USER(), VERSION();
SELECT LAST_INSERT_ID();
SELECT CONNECTION_ID(), ROW_COUNT();
SELECT @@version, @@autocommit, @@character_set_client, @@collation_connection, @@sql_mode;
SELECT FOUND_ROWS();
SELECT CASE WHEN id = 1 THEN 'one' ELSE 'other' END, IF(id > 0, 'y', 'n') FROM t;
SELECT DISTINCT tag FROM t ORDER BY tag;
SELECT DISTINCT tag FROM t ORDER BY tag LIMIT 1;

-- INSERT … SELECT / ON DUPLICATE KEY UPDATE (M68)
INSERT INTO dst (id, name) SELECT id, name FROM src WHERE id > 1;
INSERT INTO dst VALUES (1, 'z') ON DUPLICATE KEY UPDATE name = VALUES(name);

-- WITH CTE (M69)
WITH c AS (SELECT id, name FROM t WHERE id > 1) SELECT id, name FROM c;

-- Window ranking (M70)
SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS n FROM t;
SELECT grp, RANK() OVER (PARTITION BY grp ORDER BY score) AS r FROM t;
SELECT grp, DENSE_RANK() OVER (PARTITION BY grp ORDER BY score) AS d FROM t;

-- UNION (M44)
SELECT id FROM a UNION SELECT id FROM b;
SELECT id FROM a UNION ALL SELECT id FROM b;

-- FOREIGN KEY (M39)
CREATE TABLE parent (id INT PRIMARY KEY);
CREATE TABLE child (
  id INT PRIMARY KEY,
  parent_id INT,
  CONSTRAINT fk_child_parent FOREIGN KEY (parent_id) REFERENCES parent (id)
);
SELECT * FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_NAME = 'child';

-- GRANT / REVOKE (M54)
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;
REVOKE INSERT ON rusql.* FROM app;

-- Composite indexes (M50)
CREATE INDEX idx_ab ON t (a, b);
SELECT * FROM t WHERE a = 1 AND b = 2;
SHOW INDEX FROM t;

-- Multi-user auth (M55-auth)
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
CREATE USER 'legacy'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
DROP USER 'legacy'@'%';
```

Extended types (M40): `DECIMAL(p,s)`, `DATETIME`, `TEXT`, `BLOB`, `JSON` in `CREATE TABLE` and `DESCRIBE`.

Run the mysql-test wire subset (M60):

```bash
node scripts/mysql-test-subset.mjs
cargo test -p rusql-server mysql_test_subset
```

### Schema discovery (M10–M12)

```sql
SHOW TABLES;
SHOW TABLE STATUS;
SHOW TABLE STATUS LIKE 'users%';
SHOW ENGINES;
SHOW CHARACTER SET;
SHOW WARNINGS;
SHOW ERRORS;
SHOW DATABASES;
SHOW CREATE DATABASE rusql;
SHOW TRIGGERS;
SHOW TRIGGERS LIKE 'tr%';
SHOW CREATE TRIGGER tr_src;
SHOW CREATE PROCEDURE p;
SHOW CREATE FUNCTION f;
SHOW PROCEDURE STATUS;
SHOW PROCEDURE STATUS LIKE 'p%';
SHOW FUNCTION STATUS;
SHOW FUNCTION STATUS LIKE 'f%';
SHOW CREATE USER 'app'@'%';
USE rusql;
DESCRIBE users;
SHOW COLUMNS FROM users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
SHOW CREATE TABLE users;
```

### Prepared statements (M11)

Use MySQL client or driver with prepared statements; rusql supports `COM_STMT_*` with `?` binding and binary resultset rows on execute.

```bash
cargo test -p rusql-server stmt_prepare_execute
```

## Release history

See [release-notes.md](release-notes.md) and [CHANGELOG.md](../../CHANGELOG.md) at the repo root (updated on every merged PR).

## Persistence test (automated)

```bash
cargo test -p rusql-server persistence_across_connections
```

## Implemented features (M1–M6)

| Feature | Status | Notes |
|---------|--------|-------|
| MySQL wire protocol v10 handshake | Done | Default `caching_sha2_password`; native fallback |
| COM_QUERY | Done | Single-statement queries |
| COM_QUIT | Done | |
| CREATE TABLE | Done | Column types stored as metadata |
| INSERT … VALUES | Done | |
| SELECT * FROM table | Done | |
| SELECT column list | Done | M14 `SELECT id, name FROM …` |
| ORDER BY | Done | M17 `ORDER BY col [ASC|DESC]` |
| Column aliases | Done | M18 `SELECT col AS alias` |
| LIMIT | Done | M16 `LIMIT n` |
| OFFSET | Done | M19 `LIMIT n OFFSET m` |
| SELECT literal | Done | e.g. `SELECT 1` |
| Persistence (WAL) | Done | `--data-dir`, file `rusql.wal` |
| Prepared statements | Done | `COM_STMT_PREPARE` / `EXECUTE` / `CLOSE`; binary resultset on execute (M25) |
| COM_CHANGE_USER / COM_RESET_CONNECTION | Done | M51 re-auth; reset clears prepared state |
| COM_FIELD_LIST / stmt long data | Done | M52 legacy field list; `COM_STMT_SEND_LONG_DATA` + `COM_STMT_RESET` |
| SHOW PROCESSLIST / COM_PROCESS_INFO | Done | M53 active connection registry |
| Transactions | Done | `BEGIN` / `COMMIT` / `ROLLBACK`; see [m9-transactions.md](specs/m9-transactions.md) |
| SHOW TABLES / DATABASES | Done | M10 schema discovery |
| SHOW TABLE STATUS | Done | M87 documented stubs; `LIKE` / optional `FROM` db |
| SHOW ENGINES | Done | M88 documented stubs (`InnoDB` DEFAULT) |
| SHOW CHARACTER SET | Done | M89 documented stubs (`utf8mb4`) |
| SHOW WARNINGS / ERRORS | Done | M90 documented empty list |
| SHOW CREATE DATABASE | Done | M91 documented stub DDL (`utf8mb4` / `utf8mb4_unicode_ci`) |
| SHOW CREATE VIEW | Done | M92 catalog SELECT reconstruction |
| SHOW TRIGGERS | Done | M93 catalog rows; stub Definer/sql_mode/charset |
| SHOW CREATE TRIGGER | Done | M94 catalog DDL reconstruction; stub sql_mode/charset |
| SHOW CREATE PROCEDURE | Done | M95 catalog DDL reconstruction; empty params; stub charset |
| SHOW CREATE FUNCTION | Done | M96 catalog DDL reconstruction; empty params; stub charset |
| SHOW PROCEDURE STATUS | Done | M97 catalog rows; stub Definer/timestamps/charset |
| SHOW FUNCTION STATUS | Done | M98 catalog rows; stub Definer/timestamps/charset |
| SHOW CREATE USER | Done | M99 catalog DDL reconstruction; plugin name, no hash |
| DESCRIBE / information_schema | Done | M12; [m12-describe-info-schema.md](specs/m12-describe-info-schema.md) |
| SHOW CREATE TABLE | Done | M13 schema export DDL |
| ALTER TABLE ADD COLUMN | Done | M24 schema evolution |
| Indexes | Done | `CREATE INDEX`, point lookup via `WHERE col = literal` |
| Compat fixture suite | Done | `cargo test -p rusql-server compat` |
| DROP TABLE | Done | |
| DELETE | Done | `WHERE col = literal` or all rows |
| UPDATE | Done | `SET col = literal` with optional `WHERE` |

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Connection refused | Check server is running and port matches |
| Auth plugin error | Try without `--default-auth`; or use `mysql_native_password` |
| SQL syntax error | See [adr-sql-parser.md](specs/adr-sql-parser.md); we use `sqlparser` MySQL dialect |

## Stored programs and replication (P3 MVP)

- **Procedures / triggers / functions**: `CREATE PROCEDURE … BEGIN … END`, `CALL proc()`, `CREATE FUNCTION … RETURNS … BEGIN RETURN … END` (scalar in `SELECT`), `CREATE TRIGGER` (BEFORE INSERT with `SET NEW.col`; AFTER UPDATE/DELETE with `OLD.col`/`NEW.col` in DML body), `DROP PROCEDURE` / `DROP FUNCTION` / `DROP TRIGGER`. Metadata persists in `{data_dir}/programs.json`.
- **Catalog views**: `SELECT * FROM information_schema.ROUTINES` and `information_schema.TRIGGERS`.
- **Binlog on COMMIT**: Transaction commits append events to `{data_dir}/binlog/binlog.NNNNNN`. `INSERT` writes `TABLE_MAP` then `WRITE_ROWS` (v1, UTF-8 cells); `UPDATE`/`DELETE` write `TABLE_MAP` then `UPDATE_ROWS`/`DELETE_ROWS` (v1).
- **Replication**: `COM_BINLOG_DUMP` with flags `0` sends one packet per event (`0x00` + event) from the requested position and stays open so later COMMITs are streamed; `BINLOG_DUMP_NON_BLOCK` (`0x01`) dumps the current file then OK. `COM_REGISTER_SLAVE` returns OK. `SHOW MASTER STATUS` / `SHOW SLAVE STATUS` return MVP rows. `apply_binlog_file` reconstructs INSERT SQL from row events. Replica tables must already exist.

See [adr-replication.md](specs/adr-replication.md).

## Development sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/doc-parity.mjs
node scripts/check-changelog.mjs
node scripts/metrics.mjs
```

## Performance benchmark (PERF-B1)

Persistent-connection micro-benchmark (same 7 workloads as [performance-benchmark-2026-08-11.md](reports/performance-benchmark-2026-08-11.md), without per-query CLI spawn):

```bash
cargo build --release -p rusql-server
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench

node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql \
  --output target/bench-rusql.json

node scripts/bench-rusql-vs-mysql.mjs --compare --rusql-port 3307 --mysql-port 3308
```

JSON output includes QPS and p50/p95 latency per workload plus host/platform metadata. Local artifacts: `target/bench-*.json` (gitignored).

### Collation (M59 / M62)

String `ORDER BY` and `WHERE =` respect per-column collation (`COLLATE` on `CREATE TABLE`). Default catalog collation is `utf8mb4_unicode_ci`; MySQL 8.0's `utf8mb4_0900_ai_ci` is also supported (no ß→ss expansion). Verify:

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation
```

```sql
SHOW COLLATION;
CREATE TABLE t (name VARCHAR(64) COLLATE utf8mb4_0900_ai_ci);
SELECT * FROM information_schema.columns WHERE table_name = 't';
```

Supported collations: `utf8mb4_unicode_ci` (rusql default), `utf8mb4_0900_ai_ci`.

### LAST_INSERT_ID (M75)

```sql
CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16));
INSERT INTO t (name) VALUES ('alice');
SELECT LAST_INSERT_ID();
```

Session-scoped: returns the first generated `AUTO_INCREMENT` value of the last successful `INSERT` on that connection (`0` if none). The INSERT OK packet `last_insert_id` matches. Explicit inserted ids do not update it; `LAST_INSERT_ID(expr)` setter is not implemented.

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```

### CONNECTION_ID / ROW_COUNT (M76)

```sql
SELECT CONNECTION_ID();
INSERT INTO t (id, name) VALUES (1, 'alice');
SELECT ROW_COUNT();
SELECT id FROM t;
SELECT ROW_COUNT();
```

`CONNECTION_ID()` is the handshake thread id for this session (same as `SHOW PROCESSLIST` `Id`). `ROW_COUNT()` is the affected-row count of the last `INSERT`/`UPDATE`/`DELETE` on this connection; after a `SELECT` (or any result-set statement) it is `-1`. Values are not shared across connections; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset `ROW_COUNT()` to `-1`. `FOUND_ROWS()` is not implemented.

```bash
cargo test -p rusql-executor connection_id
cargo test -p rusql-executor row_count
cargo test -p rusql-server connection_id
cargo test -p rusql-server row_count
```

### Session variables (M77 / M79 / M80 / M81 / M82 / M83 / M84)

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

Documented stub set for client/ORM probes. `@@version` matches `VERSION()` (`8.0.33-rusql`). Default `@@autocommit` is `1`. Charset variables return `utf8mb4`. `@@collation_connection` is `utf8mb4_0900_ai_ci`. `@@sql_mode` is a MySQL 8.0-like mode string (not enforced). Connector handshake stubs: `@@auto_increment_increment` is `1`; `@@time_zone` is `SYSTEM`; `@@system_time_zone` is `UTC` (not the host TZ); `@@transaction_isolation` / `@@tx_isolation` is `REPEATABLE-READ`; `@@max_allowed_packet` is `67108864`; `@@license` is `GPL`. `@@session.var` equals `@@var` for this set. Unknown names return errno 1193. `SHOW VARIABLES` / `SHOW SESSION VARIABLES` list the stub catalog (`Variable_name`, `Value`) including per-connection `SET` overlays; `SHOW GLOBAL VARIABLES` stays at documented defaults. `LIKE` filters that set; a non-matching pattern returns zero rows. This is not the full MySQL 8.0 catalog.

`SET @@var`, `SET @@session.var`, `SET SESSION var`, and `SET var` persist in memory on that connection (not WAL). Setting `transaction_isolation` also updates `tx_isolation` (and vice versa). `COM_RESET_CONNECTION` and `COM_CHANGE_USER` restore documented defaults. `SET GLOBAL` is rejected (errno 1229). Read-only stubs `version`, `version_comment`, `license`, and `system_time_zone` reject SET (errno 1238). Autocommit DML engine behavior is unchanged (still autocommit-on).

`SET NAMES charset [COLLATE collation]` overlays `@@character_set_client` / `connection` / `results` (packet encoding is unchanged). `utf8mb4` without `COLLATE` sets `@@collation_connection` to `utf8mb4_0900_ai_ci`. `SET NAMES DEFAULT` restores those stubs. `SET CHARACTER SET charset` and `SET CHARSET charset` are aliases of `SET NAMES` for this overlay. `SET @foo = expr` then `SELECT @foo` returns the value on that connection; `SELECT @foo := expr` assigns and returns the value. Unset user variables are an empty cell (NULL).

`SET TRANSACTION ISOLATION LEVEL …` and `SET SESSION TRANSACTION ISOLATION LEVEL …` overlay `@@transaction_isolation` / `@@tx_isolation` with hyphenated names (`READ-COMMITTED`, `REPEATABLE-READ`, `SERIALIZABLE`, `READ-UNCOMMITTED`). rusql treats both forms as the same in-memory session overlay (not MySQL next-transaction-only scope). DML engine isolation stays snapshot. `SET GLOBAL TRANSACTION` is rejected (errno 1229). `SET TRANSACTION READ ONLY` / `READ WRITE` is a no-op.

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

### SELECT … FOR UPDATE (M85)

```sql
BEGIN;
SELECT id FROM t FOR UPDATE;
SELECT id FROM t FOR SHARE;
SELECT id FROM t LOCK IN SHARE MODE;
SELECT id FROM t FOR UPDATE NOWAIT;
SELECT id FROM t FOR UPDATE SKIP LOCKED;
COMMIT;
```

Accepted as a documented no-op: rows match the unlocked `SELECT`. rusql does not take InnoDB-style row locks, wait, or skip locked rows. Concurrent connections can both `SELECT … FOR UPDATE` the same row. `SET TRANSACTION ISOLATION LEVEL` overlays from M84 are unchanged; DML stays snapshot isolation. `GET_LOCK()` and column-level `FOR UPDATE OF col` are not implemented (`OF table` is ignored).

```bash
cargo test -p rusql-executor for_update
cargo test -p rusql-server for_update
```

### SHOW TABLE STATUS (M87)

```sql
SHOW TABLE STATUS;
SHOW TABLE STATUS LIKE 't%';
SHOW TABLE STATUS LIKE 'no_such%';
SHOW TABLE STATUS FROM rusql;
```

One row per table in the current database (same `Name` set as `SHOW TABLES`) with MySQL-shaped columns: `Name`, `Engine`, `Version`, `Row_format`, `Rows`, `Avg_row_length`, `Data_length`, `Max_data_length`, `Index_length`, `Data_free`, `Auto_increment`, `Create_time`, `Update_time`, `Check_time`, `Collation`, `Checksum`, `Create_options`, `Comment`. `Engine` is the documented stub `InnoDB`; `Version` is `10`; `Row_format` is `Dynamic`; `Rows` is the heap row count; `Auto_increment` follows the table counter when present; `Collation` is `utf8mb4_unicode_ci`. Other numeric/time/comment cells are `0` or empty (not InnoDB tablespace stats). `LIKE` filters on `Name`; a non-matching pattern returns zero rows. Optional `FROM`/`IN` lists another existing database. `SHOW STATUS` from M86 is unchanged. `SHOW ENGINE INNODB STATUS` and `WHERE` filtering are not implemented.

```bash
cargo test -p rusql-sql table_status
cargo test -p rusql-executor table_status
cargo test -p rusql-server table_status
```

### SHOW ENGINES (M88)

```sql
SHOW ENGINES;
SHOW STORAGE ENGINES;
```

Documented stub catalog for client/GUI probes (`Engine`, `Support`, `Comment`, `Transactions`, `XA`, `Savepoints`): `InnoDB` (`DEFAULT`, matching M87 `SHOW TABLE STATUS`), `MEMORY`, `MyISAM`, and `PERFORMANCE_SCHEMA` (`YES`). Comments and YES/NO flags are constants; rusql does not switch engines. This is not the full MySQL 8.0 plugin list. `SHOW TABLE STATUS` from M87 and `SHOW STATUS` from M86 are unchanged. `SHOW ENGINE INNODB STATUS` and `ENGINE=` switching are not implemented.

```bash
cargo test -p rusql-sql show_engines
cargo test -p rusql-executor show_engines
cargo test -p rusql-server show_engines
```

### SHOW CHARACTER SET (M89)

```sql
SHOW CHARACTER SET;
SHOW CHARSET;
SHOW CHARACTER SET LIKE 'utf8%';
SHOW CHARACTER SET LIKE 'no_such_charset%';
```

Documented stub catalog for client/GUI probes (`Charset`, `Description`, `Default collation`, `Maxlen`): `utf8mb4` (`Maxlen` `4`, `Default collation` `utf8mb4_unicode_ci`, matching M87 `SHOW TABLE STATUS` `Collation`). `LIKE` filters on `Charset`; a non-matching pattern returns zero rows. This is not the full MySQL 8.0 charset catalog and not wire encoding conversion. `SHOW COLLATION` from M59, `SHOW ENGINES` from M88, and `SET CHARACTER SET` from M83 are unchanged.

```bash
cargo test -p rusql-sql character_set
cargo test -p rusql-executor character_set
cargo test -p rusql-server character_set
```

### SHOW WARNINGS (M90)

```sql
SELECT 1;
SHOW WARNINGS;
SHOW ERRORS;
```

Documented empty diagnostic list for client/driver probes (`Level`, `Code`, `Message`). After a successful statement with no diagnostics the result is zero rows, not an unsupported-statement error. `SHOW ERRORS` is equivalent for this slice. This is not live truncation / sql_mode / note generation. `SHOW COUNT(*) WARNINGS`, `LIMIT`, and `WHERE` are not implemented. `SHOW CHARACTER SET` from M89 and `SHOW ENGINES` from M88 are unchanged.

```bash
cargo test -p rusql-sql warnings
cargo test -p rusql-executor warnings
cargo test -p rusql-server warnings
```

### SHOW CREATE DATABASE (M91)

```sql
SHOW CREATE DATABASE rusql;
SHOW CREATE SCHEMA rusql;
```

Documented stub DDL for client/GUI probes (`Database`, `Create Database`). The `Create Database` cell includes `utf8mb4` and rusql's documented default collation `utf8mb4_unicode_ci` (constants, not a live per-schema charset catalog). `SHOW CREATE SCHEMA` is equivalent for this slice. Unknown databases return errno 1049. This is not `CREATE DATABASE … CHARACTER SET` / `COLLATE` and not a full mysqld dump. `SHOW CREATE TABLE` from M13 and `SHOW WARNINGS` from M90 are unchanged.

```bash
cargo test -p rusql-sql show_create_database
cargo test -p rusql-executor show_create_database
cargo test -p rusql-server show_create_database
```

### SHOW CREATE VIEW (M92)

```sql
CREATE VIEW v_ids AS SELECT id FROM users;
SHOW CREATE VIEW v_ids;
```

Reconstructed catalog DDL for client/GUI probes (`View`, `Create View`, `character_set_client`, `collation_connection`). The `Create View` cell is `CREATE VIEW … AS` plus the stored SELECT from M33; charset/collation cells are documented stubs (`utf8mb4` / `utf8mb4_unicode_ci`). Unknown views return errno 1146. This is not ALGORITHM / DEFINER / SQL SECURITY. `SHOW CREATE TABLE` from M13, `SHOW CREATE DATABASE` from M91, and `SHOW WARNINGS` from M90 are unchanged.

```bash
cargo test -p rusql-sql show_create_view
cargo test -p rusql-executor show_create_view
cargo test -p rusql-server show_create_view
```

### SHOW TRIGGERS (M93)

```sql
CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id;
SHOW TRIGGERS;
SHOW TRIGGERS LIKE 'tr_s%';
SHOW TRIGGERS FROM rusql;
```

Catalog trigger list for client/GUI probes (`Trigger`, `Event`, `Table`, `Statement`, `Timing`, plus documented stubs `Created`, `sql_mode`, `Definer`, `character_set_client`, `collation_connection`, `Database Collation`). `Trigger` / `Event` / `Table` / `Timing` / `Statement` come from M48 `TriggerMeta`; Definer is the stub `root@%`, charset/collation are `utf8mb4` / `utf8mb4_unicode_ci`, and `Created` / `sql_mode` are empty. `LIKE` filters on trigger name; unmatched patterns return zero rows. Unknown `FROM`/`IN` databases return errno 1049. This is not `SHOW CREATE TRIGGER` and not live DEFINER / sql_mode persistence. `SHOW CREATE VIEW` from M92, `SHOW CREATE TABLE` from M13, and `SHOW CREATE DATABASE` from M91 are unchanged.

```bash
cargo test -p rusql-sql show_triggers
cargo test -p rusql-executor show_triggers
cargo test -p rusql-server show_triggers
```

### SHOW CREATE TRIGGER (M94)

```sql
CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id;
SHOW CREATE TRIGGER tr_src;
```

Reconstructed catalog DDL for client/GUI probes (`Trigger`, `sql_mode`, `SQL Original Statement`, `character_set_client`, `collation_connection`, `Database Collation`, `Created`). The `SQL Original Statement` cell is `CREATE TRIGGER … {BEFORE|AFTER} {INSERT|UPDATE|DELETE} ON … FOR EACH ROW …` from stored M48 `TriggerMeta`; charset/collation cells are documented stubs (`utf8mb4` / `utf8mb4_unicode_ci`) and `sql_mode` / `Created` are empty. Unknown triggers return errno 1360. This is not DEFINER / sql_mode dump. `SHOW TRIGGERS` from M93, `SHOW CREATE VIEW` from M92, and `SHOW CREATE TABLE` from M13 are unchanged.

```bash
cargo test -p rusql-sql show_create_trigger
cargo test -p rusql-executor show_create_trigger
cargo test -p rusql-server show_create_trigger
```

### SHOW CREATE PROCEDURE (M95)

```sql
CREATE PROCEDURE p() BEGIN INSERT INTO src VALUES (42); END;
SHOW CREATE PROCEDURE p;
```

Reconstructed catalog DDL for client/GUI probes (`Procedure`, `sql_mode`, `Create Procedure`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Procedure` cell is `CREATE PROCEDURE …() BEGIN … END` from stored P3 `ProcedureMeta` with an empty parameter list; charset/collation cells are documented stubs (`utf8mb4` / `utf8mb4_unicode_ci`) and `sql_mode` is empty. Unknown procedures return errno 1305. This is not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE TRIGGER` from M94, `SHOW TRIGGERS` from M93, and `SHOW CREATE VIEW` from M92 are unchanged.

```bash
cargo test -p rusql-sql show_create_procedure
cargo test -p rusql-executor show_create_procedure
cargo test -p rusql-server show_create_procedure
```

### SHOW CREATE FUNCTION (M96)

```sql
CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END;
SHOW CREATE FUNCTION f;
```

Reconstructed catalog DDL for client/GUI probes (`Function`, `sql_mode`, `Create Function`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Function` cell is `CREATE FUNCTION …() RETURNS … BEGIN RETURN … END` from stored M63 `FunctionMeta` with an empty parameter list; charset/collation cells are documented stubs (`utf8mb4` / `utf8mb4_unicode_ci`) and `sql_mode` is empty. Unknown functions return errno 1305. This is not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE PROCEDURE` from M95, `SHOW CREATE TRIGGER` from M94, and `SHOW CREATE VIEW` from M92 are unchanged.

```bash
cargo test -p rusql-sql show_create_function
cargo test -p rusql-executor show_create_function
cargo test -p rusql-server show_create_function
```

### SHOW PROCEDURE STATUS (M97)

```sql
CREATE PROCEDURE p() BEGIN INSERT INTO src VALUES (42); END;
SHOW PROCEDURE STATUS;
SHOW PROCEDURE STATUS LIKE 'p%';
```

Catalog procedure list for client/GUI probes (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from stored P3 `ProcedureMeta`; `Type` is `PROCEDURE`. Definer / timestamps / charset cells are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. This is not live DEFINER persistence and not `SHOW FUNCTION STATUS`. `SHOW CREATE FUNCTION` from M96, `SHOW CREATE PROCEDURE` from M95, and `SHOW CREATE TRIGGER` from M94 are unchanged.

```bash
cargo test -p rusql-sql show_procedure_status
cargo test -p rusql-executor show_procedure_status
cargo test -p rusql-server show_procedure_status
```

### SHOW FUNCTION STATUS (M98)

```sql
CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END;
SHOW FUNCTION STATUS;
SHOW FUNCTION STATUS LIKE 'f%';
```

Catalog function list for client/GUI probes (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from stored M63 `FunctionMeta`; `Type` is `FUNCTION`. Definer / timestamps / charset cells are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. This is not live DEFINER persistence. `SHOW PROCEDURE STATUS` from M97, `SHOW CREATE FUNCTION` from M96, and `SHOW CREATE PROCEDURE` from M95 are unchanged.

```bash
cargo test -p rusql-sql show_function_status
cargo test -p rusql-executor show_function_status
cargo test -p rusql-server show_function_status
```

### SHOW CREATE USER (M99)

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
SHOW CREATE USER 'app'@'%';
SHOW CREATE USER CURRENT_USER();
```

Reconstructed catalog DDL for client/GUI probes (`CREATE USER for {user}@{host}`). The cell is `CREATE USER \`u\`@\`h\` IDENTIFIED WITH '{plugin}'` from the M55 account catalog; password hashes, `BY`, and `AS` stay out of the cell. Named `'u'@'h'` / `user@host` and session `CURRENT_USER` forms are accepted. Unknown accounts return errno 3162. This is not TLS / resource-limit / DEFAULT ROLE dump. `SHOW FUNCTION STATUS` from M98, `SHOW PROCEDURE STATUS` from M97, and `SHOW CREATE FUNCTION` from M96 are unchanged.

```bash
cargo test -p rusql-sql show_create_user
cargo test -p rusql-executor show_create_user
cargo test -p rusql-server show_create_user
```

### SHOW STATUS (M86)

```sql
SHOW STATUS;
SHOW SESSION STATUS;
SHOW GLOBAL STATUS;
SHOW STATUS LIKE 'Threads%';
SHOW STATUS LIKE 'not_a_real_status%';
```

Documented stub catalog for client/monitor probes (`Variable_name`, `Value`): `Uptime` (`0`), `Threads_connected` (current connection count when the process list is available, otherwise `1`), `Threads_running` (`1`), `Questions` (`0`), `Slow_queries` (`0`), `Open_tables` (`0`), `Connections` (`1`), `Aborted_connects` (`0`), `Bytes_received` (`0`), `Bytes_sent` (`0`). `SHOW SESSION STATUS` and `SHOW GLOBAL STATUS` return the same rows for this slice. `LIKE` filters that set; a non-matching pattern returns zero rows. This is not the full MySQL 8.0 catalog and not live InnoDB/`performance_schema` counters. `FLUSH STATUS` and `SHOW ENGINE INNODB STATUS` are not implemented. `SELECT … FOR UPDATE` and `SET TRANSACTION ISOLATION LEVEL` are unchanged.

```bash
cargo test -p rusql-executor show_status
cargo test -p rusql-server show_status
```

### FOUND_ROWS / SQL_CALC_FOUND_ROWS (M78)

```sql
SELECT id FROM t LIMIT 1;
SELECT FOUND_ROWS();
SELECT SQL_CALC_FOUND_ROWS id FROM t LIMIT 1;
SELECT FOUND_ROWS();
```

After a plain `SELECT`, `FOUND_ROWS()` is the number of rows in that result (after `LIMIT`). `SELECT SQL_CALC_FOUND_ROWS … LIMIT n` then `FOUND_ROWS()` returns the match count **without** `LIMIT` (MySQL 8.0.17-deprecated pagination helper; prefer `COUNT(*)`). After `INSERT`/`UPDATE`/`DELETE` it matches `ROW_COUNT()`. Values are session-scoped; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset to `0`.

```bash
cargo test -p rusql-executor found_rows
cargo test -p rusql-server found_rows
```

### Sysbench comparison (M61 / PERF-B6)

Industry-standard `oltp_point_select` against rusql and Docker MySQL 8.0:

```bash
sudo apt-get install -y sysbench   # or choco install sysbench on Windows
docker run -d --name rusql-mysql80-bench -e MYSQL_ALLOW_EMPTY_PASSWORD=yes -p 3308:3306 mysql:8.0
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-sysbench
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308 --threshold 0.7
```

Soft-fails when Sysbench or Docker are missing. CI: `.github/workflows/sysbench.yml` (`workflow_dispatch`).

## Performance optimizations (PERF-B2 / PERF-B3)

- **`SELECT … ORDER BY indexed_col LIMIT n`** (no `WHERE`): secondary-index ordered scan with early stop.
- **`UPDATE … WHERE pk = ?`**: PK index lookup + incremental index maintenance.

```bash
cargo test -p rusql-storage scan_index_ordered_with_limit pk_update_without_index_rebuild
cargo test -p rusql-executor select_order_by_indexed_limit update_pk_by_index
```

### Multi-threaded benchmark (PERF-B4)

```bash
node scripts/bench-rusql-vs-mysql.mjs --threads 8 --duration 30 --workloads read-heavy \
  --host 127.0.0.1 --port 3307 --label rusql
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare \
  --rusql-port 3307 --mysql-port 3308 --duration 10
```

### WAL sync policy (PERF-B5)

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
cargo run -p rusql-server -- --wal-sync none --port 3307 --data-dir ./.test-data-bench
```

**Warning**: `batch` and `none` trade durability for speed.
