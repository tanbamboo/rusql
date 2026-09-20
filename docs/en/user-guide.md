# User Guide — Testing rusql

This guide describes **what works today** on `main` and how to verify it.

## Compatibility vs MySQL 8.0

**Verdict (2026-09-20):** rusql is **not** a production drop-in for MySQL 8.0. Phase Q (M62–M113) is complete: the official `mysql` CLI can introspect session state without `unsupported function`. The live comparison is **327/327** `mysql-diff` steps vs Docker MySQL 8.0.

Full matrix (what works, what is a stub, what is missing, and when you might use rusql): [rusql vs MySQL test report](reports/rusql-vs-mysql.md).

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

Compare the same portable SQL on rusql and Docker MySQL 8.0 (CI gate):

```bash
node scripts/mysql-diff.mjs
node scripts/mysql-test-subset.mjs
```

Results and the production verdict: [rusql vs MySQL test report](reports/rusql-vs-mysql.md).

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

-- Expressions (M46 / M65 / M66 / M67 / M77 / M113)
SELECT id + 1, CONCAT(name, '!'), COALESCE(note, 'n/a'), LOWER(name) FROM t;
SELECT SUBSTRING(name, 1, 2), ROUND(1.5), DATE_ADD('2026-01-01', INTERVAL 1 DAY);
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

-- REPLACE INTO (M111)
REPLACE INTO t VALUES (1, 10);
REPLACE INTO t VALUES (1, 20);

-- INSERT IGNORE (M112)
INSERT IGNORE INTO t VALUES (1, 99), (2, 20);

-- TRUNCATE TABLE (M110)
TRUNCATE TABLE t;
TRUNCATE t;

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
CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1;
ALTER EVENT e ON SCHEDULE EVERY 1 DAY;
SHOW CREATE EVENT e;
SHOW EVENTS;
SHOW EVENTS LIKE 'e%';
DROP EVENT e;
USE rusql;
DESCRIBE users;
SHOW COLUMNS FROM users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
SELECT EVENT_NAME, EVENT_COMMENT, LAST_EXECUTED FROM information_schema.EVENTS;
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
| SHOW CREATE DATABASE | Done | M91/M114 live per-schema charset (`utf8mb4` / catalog collations) |
| SHOW CREATE VIEW | Done | M92 catalog SELECT reconstruction |
| SHOW TRIGGERS | Done | M93 catalog rows; stub Definer/sql_mode/charset |
| SHOW CREATE TRIGGER | Done | M94 catalog DDL reconstruction; stub sql_mode/charset |
| SHOW CREATE PROCEDURE | Done | M95 catalog DDL reconstruction; empty params; stub charset |
| SHOW CREATE FUNCTION | Done | M96 catalog DDL reconstruction; empty params; stub charset |
| SHOW PROCEDURE STATUS | Done | M97 catalog rows; stub Definer/timestamps/charset |
| SHOW FUNCTION STATUS | Done | M98 catalog rows; stub Definer/timestamps/charset |
| SHOW CREATE USER | Done | M99 catalog DDL reconstruction; plugin name, no hash |
| SHOW CREATE EVENT | Done | M102 catalog DDL reconstruction; unknown errno 1539 |
| SHOW EVENTS | Done | M102 catalog rows; unmatched `LIKE` is zero rows |
| CREATE EVENT / DROP EVENT | Done | M102 catalog persistence; M104–M108 scheduler / DEFINER / ON COMPLETION / COMMENT |
| ALTER EVENT | Done | M103 catalog schedule/status/rename/DO; M106 `STARTS`/`ENDS`; M107 DEFINER / ON COMPLETION; M108 COMMENT |
| Event scheduler (due AT) | Done | M104 executes ENABLED `ONE TIME` `AT` when due; `@@event_scheduler` is `ON` |
| Event scheduler (`EVERY`) | Done | M105 first fire on next COM_QUERY, then `last_executed + interval` |
| Event scheduler (`STARTS`/`ENDS`) | Done | M106 gates `EVERY`; `SHOW EVENTS` Starts/Ends from catalog |
| Event DEFINER / ON COMPLETION | Done | M107 catalog + PRESERVE keeps AT events DISABLED after run |
| Event COMMENT | Done | M108 catalog; `SHOW CREATE EVENT` reconstructs `COMMENT '…'`; omitted when empty |
| information_schema.EVENTS | Done | M109 catalog rows; `LAST_EXECUTED` / `EVENT_COMMENT` empty when unset |
| DESCRIBE / information_schema | Done | M12; [m12-describe-info-schema.md](specs/m12-describe-info-schema.md) |
| SHOW CREATE TABLE | Done | M13 schema export DDL |
| ALTER TABLE ADD COLUMN | Done | M24 schema evolution |
| Indexes | Done | `CREATE INDEX`, point lookup via `WHERE col = literal` |
| Compat fixture suite | Done | `cargo test -p rusql-server compat` |
| DROP TABLE | Done | |
| DELETE | Done | `WHERE col = literal` or all rows |
| UPDATE | Done | `SET col = literal` with optional `WHERE` |
| TRUNCATE TABLE | Done | M110 heap delete-all + `AUTO_INCREMENT` reset; `affected_rows` 0; no DELETE triggers |
| REPLACE INTO | Done | M111 PK conflict delete-then-insert; `affected_rows` 1 (insert) or 2 (replace) |
| INSERT IGNORE | Done | M112 PK conflict skip; `affected_rows` = rows actually inserted; existing row unchanged |
| SUBSTRING / ROUND / DATE_ADD | Done | M113 1-based `SUBSTRING`/`SUBSTR`; `ROUND` half-away-from-zero; `DATE_ADD` INTERVAL (MONTH/YEAR 30/365-day) |
| CREATE DATABASE CHARACTER SET | Done | M114 persist charset/collation; `SHOW CREATE DATABASE` / SCHEMATA use catalog |

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Connection refused | Check server is running and port matches |
| Auth plugin error | Try without `--default-auth`; or use `mysql_native_password` |
| SQL syntax error | See [adr-sql-parser.md](specs/adr-sql-parser.md); we use `sqlparser` MySQL dialect |

## Stored programs and replication (P3 MVP)

- **Procedures / triggers / functions / events**: `CREATE PROCEDURE … BEGIN … END`, `CALL proc()`, `CREATE FUNCTION … RETURNS … BEGIN RETURN … END` (scalar in `SELECT`), `CREATE TRIGGER` (BEFORE INSERT with `SET NEW.col`; AFTER UPDATE/DELETE with `OLD.col`/`NEW.col` in DML body), `CREATE EVENT … ON SCHEDULE … DO …` / `ALTER EVENT` (catalog; due one-time `AT` and `EVERY` events run `DO` on COM_QUERY, gated by `STARTS`/`ENDS`; `DEFINER` / `ON COMPLETION`), `DROP PROCEDURE` / `DROP FUNCTION` / `DROP TRIGGER` / `DROP EVENT`. Metadata persists in `{data_dir}/programs.json`.
- **Catalog views**: `SELECT * FROM information_schema.ROUTINES`, `information_schema.TRIGGERS`, and `information_schema.EVENTS`.
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
node scripts/mysql-gap-probe.mjs
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

### TRUNCATE TABLE (M110)

```sql
CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16));
INSERT INTO t (name) VALUES ('alice');
TRUNCATE TABLE t;
TRUNCATE t;
INSERT INTO t (name) VALUES ('bob');
SELECT id FROM t;
```

`TRUNCATE TABLE t` and `TRUNCATE t` remove all rows from base table `t` via heap `delete_rows` with no filter (not InnoDB tablespace reuse). If the table has `AUTO_INCREMENT`, the next generated id is 1 (`StorageEngine::set_auto_increment` and session catalog `TableMeta.auto_increment_next`). The OK packet reports `affected_rows = 0` (MySQL-shaped; not the deleted count). `AFTER DELETE` triggers do not fire. Unknown tables are errno 1146. `information_schema` and SHOW virtual tables (`__rusql_*`) are rejected. `TRUNCATE … PARTITION` / `CASCADE` are not implemented. `DELETE` / `DROP TABLE` are unchanged.

```bash
cargo test -p rusql-executor truncate
cargo test -p rusql-server truncate
```

### REPLACE INTO (M111)

```sql
CREATE TABLE t (id INT PRIMARY KEY, v INT);
REPLACE INTO t VALUES (1, 10);
REPLACE INTO t VALUES (1, 20);
SELECT id, v FROM t;
```

`REPLACE INTO t VALUES (…)` inserts when the PRIMARY KEY is new (`affected_rows` 1). On a single-column PK conflict it deletes the old row then inserts the new values (`affected_rows` 2; old row gone). Composite PRIMARY KEY is rejected (same single-column limit as M68). Plain `INSERT` of a duplicate PK stays errno 1062. `INSERT … ON DUPLICATE KEY UPDATE` is unchanged. Multi-table REPLACE and UNIQUE-not-PK conflicts are not implemented.

```bash
cargo test -p rusql-executor replace
cargo test -p rusql-server replace
```

### INSERT IGNORE (M112)

```sql
CREATE TABLE t (id INT PRIMARY KEY, v INT);
INSERT INTO t VALUES (1, 10);
INSERT IGNORE INTO t VALUES (1, 99), (2, 20);
SELECT id, v FROM t ORDER BY id;
```

`INSERT IGNORE INTO t VALUES (…)` skips a PRIMARY KEY conflict (`affected_rows` is the number of rows actually inserted; the existing row is unchanged). A new PK inserts as usual. Multi-row `INSERT IGNORE` inserts non-conflicting rows and skips duplicates. Plain `INSERT` of a duplicate PK stays errno 1062. M68 `ON DUPLICATE KEY UPDATE` and M111 `REPLACE INTO` are unchanged. UNIQUE-not-PK IGNORE, sql_mode truncation IGNORE, and `SHOW WARNINGS` notes are not implemented.

```bash
cargo test -p rusql-executor insert_ignore
cargo test -p rusql-server insert_ignore
```

### SUBSTRING / ROUND / DATE_ADD (M113)

```sql
SELECT SUBSTRING('abc', 1, 2);
SELECT SUBSTR('abc', 1, 2);
SELECT ROUND(1.4);
SELECT ROUND(1.5);
SELECT DATE_ADD('2026-01-01', INTERVAL 1 DAY);
```

`SUBSTRING`/`SUBSTR` is MySQL 1-based (`SUBSTRING('abc', 1, 2)` → `ab`). `SUBSTRING(s, pos)` runs to the end of the string; a negative `pos` counts from the end. `ROUND(x)` and `ROUND(x, d)` use **half away from zero** (so `ROUND(1.5)` is `2`, not banker's `2`/`0` even-rule); values are parsed as `f64`. `DATE_ADD`/`ADDDATE` accept `INTERVAL n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR}`. Date-only input plus `DAY`/`WEEK`/`MONTH`/`YEAR` returns `YYYY-MM-DD` (so `DATE_ADD('2026-01-01', INTERVAL 1 DAY)` is `2026-01-02`); otherwise the result is `YYYY-MM-DD HH:MM:SS`. `MONTH`/`YEAR` reuse the M105 30/365-day approximation, not calendar months. `DATE_SUB`, `SUBSTRING_INDEX`, `JSON_EXTRACT`, `UUID()`, `GET_LOCK`, and `LAST_INSERT_ID(expr)` are not implemented.

```bash
cargo test -p rusql-executor substring
cargo test -p rusql-executor round
cargo test -p rusql-executor date_add
cargo test -p rusql-server substring
```

### CREATE DATABASE CHARACTER SET (M114)

```sql
CREATE DATABASE gap_cs CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
CREATE DATABASE gap_cs2 CHARSET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
CREATE DATABASE gap_def DEFAULT CHARACTER SET utf8mb4;
SHOW CREATE DATABASE gap_cs;
SELECT SCHEMA_NAME, DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME
  FROM information_schema.SCHEMATA;
```

`CREATE DATABASE … CHARACTER SET … COLLATE …` persists per-schema charset/collation. `CHARSET` is a synonym; `DEFAULT` before either clause is optional. Omitting the clauses keeps rusql defaults (`utf8mb4` / `utf8mb4_unicode_ci`). Supported collations are `utf8mb4_unicode_ci` and `utf8mb4_0900_ai_ci`. Unknown charset is errno 1115; unknown collation is errno 1273. `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` use the catalog (M91 columns unchanged). This is not `ALTER DATABASE … CHARACTER SET` and not every MySQL charset.

```bash
cargo test -p rusql-sql create_database
cargo test -p rusql-storage create_database
cargo test -p rusql-executor create_database
cargo test -p rusql-server create_database
```

### CONNECTION_ID / ROW_COUNT (M76)

```sql
SELECT CONNECTION_ID();
INSERT INTO t (id, name) VALUES (1, 'alice');
SELECT ROW_COUNT();
SELECT id FROM t;
SELECT ROW_COUNT();
```

`CONNECTION_ID()` is the handshake thread id for this session (same as `SHOW PROCESSLIST` `Id`). `ROW_COUNT()` is the affected-row count of the last `INSERT`/`UPDATE`/`DELETE` on this connection; after a `SELECT` (or any result-set statement) it is `-1`. Values are not shared across connections; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset `ROW_COUNT()` to `-1`. `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` are M78.

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

Documented stub set for client/ORM probes. `@@version` matches `VERSION()` (`8.0.33-rusql`). Default `@@autocommit` is `1`. Charset variables return `utf8mb4`. `@@collation_connection` is `utf8mb4_0900_ai_ci`. `@@sql_mode` is a MySQL 8.0-like mode string (not enforced). Connector handshake stubs: `@@auto_increment_increment` is `1`; `@@time_zone` is `SYSTEM`; `@@system_time_zone` is `UTC` (not the host TZ); `@@transaction_isolation` / `@@tx_isolation` is `REPEATABLE-READ`; `@@max_allowed_packet` is `67108864`; `@@license` is `GPL`; `@@event_scheduler` is `ON` (read-only). `@@session.var` equals `@@var` for this set. Unknown names return errno 1193. `SHOW VARIABLES` / `SHOW SESSION VARIABLES` list the stub catalog (`Variable_name`, `Value`) including per-connection `SET` overlays; `SHOW GLOBAL VARIABLES` stays at documented defaults. `LIKE` filters that set; a non-matching pattern returns zero rows. This is not the full MySQL 8.0 catalog.

`SET @@var`, `SET @@session.var`, `SET SESSION var`, and `SET var` persist in memory on that connection (not WAL). Setting `transaction_isolation` also updates `tx_isolation` (and vice versa). `COM_RESET_CONNECTION` and `COM_CHANGE_USER` restore documented defaults. `SET GLOBAL` is rejected (errno 1229). Read-only stubs `version`, `version_comment`, `license`, `system_time_zone`, and `event_scheduler` reject SET (errno 1238). Autocommit DML engine behavior is unchanged (still autocommit-on).

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

Documented DDL for client/GUI probes (`Database`, `Create Database`). The `Create Database` cell includes the schema's stored charset and collation (M114). Databases created without clauses, and `rusql`, use `utf8mb4` / `utf8mb4_unicode_ci`. `SHOW CREATE SCHEMA` is equivalent for this slice. Unknown databases return errno 1049. This is not `ALTER DATABASE … CHARACTER SET` and not a full mysqld dump. `SHOW CREATE TABLE` from M13 and `SHOW WARNINGS` from M90 are unchanged.

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

### SHOW CREATE EVENT (M100 / M102)

```sql
CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1;
SHOW CREATE EVENT e;
```

Reconstructed catalog DDL for client/GUI probes (`Event`, `sql_mode`, `time_zone`, `Create Event`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Event` cell is `CREATE DEFINER=\`u\`@\`h\` EVENT \`name\` ON SCHEDULE AT '…' ON COMPLETION NOT PRESERVE DO …` or `EVERY n UNIT [STARTS '…'] [ENDS '…']` from stored `EventMeta`. `sql_mode` / charset cells are documented stubs (empty `sql_mode`, `SYSTEM` time zone, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown names return errno 1539. This is not COMMENT dump and not a timer thread. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql show_create_event
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server show_create_event
```

### SHOW EVENTS (M101 / M102)

```sql
SHOW EVENTS;
SHOW EVENTS LIKE 'e%';
SHOW EVENTS FROM rusql;
```

Catalog event list for client/GUI probes (`Db`, `Name`, `Definer`, `Time zone`, `Type`, `Execute at`, `Interval value`, `Interval field`, `Starts`, `Ends`, `Status`, `Originator`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` / `Type` / schedule / `Starts` / `Ends` / `Definer` cells come from `EventMeta`; timezone / charset / Originator are documented stubs (`SYSTEM`, `1`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. Unknown `FROM` databases return errno 1049. This is not live last-executed timestamps. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql show_events
cargo test -p rusql-executor show_events
cargo test -p rusql-server show_events
```

### CREATE EVENT catalog (M102)

```sql
CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1;
CREATE EVENT r ON SCHEDULE EVERY 1 HOUR DO SELECT 1;
DROP EVENT e;
```

Persist `EventMeta` in `{data_dir}/programs.json` (and the session catalog) for client/GUI probes. Duplicate names return errno 1537. `IF NOT EXISTS` / `DROP EVENT IF EXISTS` are accepted. Due ENABLED one-time `AT` events run `DO` on the next COM_QUERY (M104). Recurring `EVERY` runs on the next COM_QUERY and then after the interval (M105), gated by `STARTS`/`ENDS` (M106). `ALTER EVENT` from M103 can change schedule / status / name / body / window. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-executor create_event
cargo test -p rusql-server create_event
```

### ALTER EVENT catalog (M103)

```sql
ALTER EVENT e ON SCHEDULE EVERY 1 DAY;
ALTER EVENT e DISABLE;
ALTER EVENT e RENAME TO e2 DO SELECT 2;
```

Update stored `EventMeta` for client/GUI probes. Unknown names return errno 1539. `SHOW EVENTS` and `SHOW CREATE EVENT` reflect the change. Due one-time `AT` events run `DO` on the next COM_QUERY (M104). Recurring `EVERY` runs on the next COM_QUERY and then after the interval (M105), gated by `STARTS`/`ENDS` (M106). DEFINER / ON COMPLETION persist (M107). Event COMMENT persists (M108). `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql alter_event
cargo test -p rusql-executor alter_event
cargo test -p rusql-server alter_event
```

### Event scheduler due AT (M104)

```sql
CREATE TABLE t (id INT PRIMARY KEY);
CREATE EVENT e ON SCHEDULE AT '2000-01-01 00:00:00' DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SELECT @@event_scheduler;
SHOW VARIABLES LIKE 'event_scheduler';
```

ENABLED one-time `AT` events whose `execute_at` is due (UTC `YYYY-MM-DD HH:MM:SS`) run `DO` on the next COM_QUERY of that server, then the catalog row is dropped (MySQL default `ON COMPLETION NOT PRESERVE`). `@@event_scheduler` is a read-only `ON` stub (SET errno 1238; `SET GLOBAL` stays 1229). DISABLED events and future `AT` timestamps are not executed. Recurring `EVERY` is M105. A `DO` error does not fail the client statement. This is not a background timer thread, last-executed timestamps on `SHOW EVENTS`, or DEFINER. `ALTER EVENT` from M103 and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor session_var
cargo test -p rusql-server event_scheduler
```

### Event scheduler EVERY (M105)

```sql
CREATE TABLE t (id INT);
CREATE EVENT e ON SCHEDULE EVERY 1 HOUR DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SHOW EVENTS LIKE 'e';
```

ENABLED `RECURRING` `EVERY n UNIT` events run `DO` on the next COM_QUERY, then again after `last_executed + interval` (internal watermark; not a `SHOW EVENTS` column). The catalog row stays. `MONTH`/`YEAR` use 30/365-day approximations. DISABLED `EVERY` is skipped. `STARTS`/`ENDS` gates are M106. M104 one-time `AT` (run then drop) and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```

### Event scheduler STARTS / ENDS (M106)

```sql
CREATE TABLE t (id INT);
CREATE EVENT e ON SCHEDULE EVERY 1 HOUR STARTS '2000-01-01 00:00:00' ENDS '2038-01-01 00:00:00' DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SHOW EVENTS LIKE 'e';
ALTER EVENT e ON SCHEDULE EVERY 1 HOUR STARTS '2001-01-01 00:00:00';
ALTER EVENT e STARTS '2001-01-01 00:00:00';
```

ENABLED `EVERY` events are not due when `now < starts` or `now > ends` (inclusive window). First fire still happens on the next COM_QUERY once `starts` is due (injected `now` in unit tests; no sleep). `SHOW EVENTS` `Starts` / `Ends` come from the catalog (empty when unset). `SHOW CREATE EVENT` reconstructs `STARTS` / `ENDS`. Column count stays 15. MySQL requires `ON SCHEDULE EVERY n UNIT` before `STARTS`/`ENDS`; rusql also accepts standalone `ALTER EVENT name STARTS` / `ENDS` as a catalog-window shorthand. This is not a timer thread. M105 interval watermark, M104 one-time `AT` drop-after-run, and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```

### Event DEFINER / ON COMPLETION (M107)

```sql
CREATE DEFINER=`app`@`%` EVENT e ON SCHEDULE AT '2000-01-01 00:00:00' ON COMPLETION PRESERVE DO INSERT INTO t VALUES (1);
SELECT id FROM t;
SHOW EVENTS LIKE 'e';
SHOW CREATE EVENT e;
ALTER EVENT e ON COMPLETION NOT PRESERVE;
```

`DEFINER` (`user@host`; omitted → session user/host) and `ON COMPLETION` (`PRESERVE` / `NOT PRESERVE`; omitted → `NOT PRESERVE`) persist in `{data_dir}/programs.json`. `SHOW EVENTS` `Definer` comes from the catalog. `SHOW CREATE EVENT` reconstructs both clauses. Due ENABLED `AT` with `NOT PRESERVE` still drops after a successful `DO` (M104). `PRESERVE` keeps the row and sets `DISABLED`. Event COMMENT is [M108](#event-comment-m108). This is not `DISABLE ON SLAVE` / last-executed on `SHOW EVENTS`. M106 `STARTS`/`ENDS`, M105 watermark, and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server event_scheduler
```

### Event COMMENT (M108)

```sql
CREATE EVENT e ON SCHEDULE EVERY 1 HOUR COMMENT 'hi' DO SELECT 1;
SHOW CREATE EVENT e;
ALTER EVENT e COMMENT 'bye';
SHOW CREATE EVENT e;
SHOW EVENTS LIKE 'e';
```

`COMMENT` text persists in `{data_dir}/programs.json` (`EventMeta.comment`, `serde(default)`). `SHOW CREATE EVENT` reconstructs `COMMENT '…'` when set and omits the clause when empty or unset (MySQL default). `SHOW EVENTS` stays 15 columns (no Comment / last-executed column). Read `EVENT_COMMENT` / `LAST_EXECUTED` from [`information_schema.EVENTS`](#information_schemaevents-m109). This is not COMMENT on procedures / functions / triggers / views. M107 DEFINER / ON COMPLETION, M106 `STARTS`/`ENDS`, and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server create_event
```

### information_schema.EVENTS (M109)

```sql
CREATE EVENT e ON SCHEDULE EVERY 1 HOUR COMMENT 'hi' DO SELECT 1;
SELECT EVENT_NAME, EVENT_COMMENT, LAST_EXECUTED FROM information_schema.EVENTS;
SELECT EVENT_NAME FROM information_schema.events;
SHOW EVENTS LIKE 'e';
DROP EVENT e;
```

Catalog events appear as rows in `information_schema.EVENTS` (also `information_schema.events`; sqlparser preserves identifier case). Documented columns: `EVENT_SCHEMA`, `EVENT_NAME`, `DEFINER` (empty catalog definer → stub `root@%`, same as `SHOW EVENTS`), `EVENT_TYPE` (`ONE TIME` / `RECURRING`), `EXECUTE_AT`, `INTERVAL_VALUE`, `INTERVAL_FIELD`, `STARTS`, `ENDS`, `STATUS`, `ON_COMPLETION` (unset → `NOT PRESERVE`), `LAST_EXECUTED` (from `EventMeta.last_executed`; empty when never run), `EVENT_COMMENT` (from `EventMeta.comment`; empty when unset). rusql does not invent timestamps. `SHOW EVENTS` stays 15 columns. M107 DEFINER / ON COMPLETION reconstruction on `SHOW CREATE EVENT` is unchanged. This is not `information_schema.PARAMETERS` / `TABLE_CONSTRAINTS` / `PROCESSLIST`.

```bash
cargo test -p rusql-executor information_schema
cargo test -p rusql-executor events
cargo test -p rusql-server information_schema
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
