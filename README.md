# rusql

A MySQL 8.0-compatible database written in Rust, built with [Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) for AI-native development.

**简体中文**: [docs/zh-CN/README.md](docs/zh-CN/README.md)

## Status

Active development toward MySQL 8.0 compatibility. **Not a production drop-in for MySQL 8.0.** User-facing comparison (what works, what does not, test counts vs MySQL): [rusql vs MySQL test report](docs/en/reports/rusql-vs-mysql.md) · [中文](docs/zh-CN/reports/rusql-vs-mysql.md).

| Milestone | Status |
|-----------|--------|
| M0 Harness | Done |
| M1 Wire protocol handshake | Done |
| M2 COM_QUERY (CREATE/INSERT/SELECT) | Done |
| M3 WAL persistence | Done | `--data-dir` (default `rusql-data`) |
| M4 Indexes | Done | `CREATE INDEX`, `WHERE col = literal` |
| M5 Compat test subset | Done | JSON fixtures in `crates/rusql-server/compat/` |
| M6 Auth + DROP/DELETE | Done | `--auth-password`; see [adr-m6-auth-and-dml.md](docs/en/specs/adr-m6-auth-and-dml.md) |
| M7 caching_sha2 | Done | Default auth plugin; [adr-m7-caching-sha2.md](docs/en/specs/adr-m7-caching-sha2.md) |
| M8 UPDATE | Done | `UPDATE … SET … WHERE` |
| M9 Transactions | Done | `BEGIN` / `COMMIT` / `ROLLBACK`; savepoints (M126) |
| M10 SHOW TABLES | Done | `SHOW TABLES`, `SHOW DATABASES` |
| M11 Prepared statements | Done | `COM_STMT_PREPARE` / `EXECUTE` / `CLOSE`; text `PREPARE` (M125) |
| M12 DESCRIBE / information_schema | Done | `DESCRIBE`, `SHOW COLUMNS`, `information_schema.tables/columns` |
| M13 SHOW CREATE TABLE | Done | MySQL-style DDL export |
| M14 SELECT projection | Done | `SELECT col1, col2 FROM …` |
| M15 USE database | Done | `USE rusql` |
| M16 SELECT LIMIT | Done | `LIMIT n` on table SELECT |
| M17 ORDER BY | Done | `ORDER BY col [ASC|DESC]` |
| M18 column aliases | Done | `SELECT col AS alias` |
| M19 LIMIT OFFSET | Done | `LIMIT n OFFSET m` |
| M20 WHERE ops | Done | `<`, `>`, `<=`, `>=`, `<>`, `AND` |
| M21 IS NULL | Done | `IS NULL` / `IS NOT NULL` |
| M22 INNER JOIN | Done | two-table `INNER JOIN ... ON` |
| M23 PRIMARY KEY | Done | `PRIMARY KEY` / `NOT NULL` in DESCRIBE |
| M24 ALTER ADD COLUMN | Done | `ALTER TABLE … ADD COLUMN` |
| M25 Binary resultset | Done | `COM_STMT_EXECUTE` binary rows |
| M26 caching_sha2 RSA | Done | Full RSA auth when `--auth-password` |
| M27 info_schema++ | Done | `SCHEMATA`, `STATISTICS` |
| M28 SHOW INDEX | Done | `SHOW INDEX FROM tbl` |
| M29 mysql-diff | Done | Docker MySQL differential runner |
| M30 mysql-test subset | Done | 100-case wire harness (`tests/mysql-test/`) |
| M31 Durable COMMIT WAL | Done | `COMMIT` → WAL replay survives restart |
| M32 MVCC | Done | Snapshot isolation |
| M33 Views | Done | `CREATE VIEW` / `CREATE OR REPLACE VIEW` |
| M35 Charset metadata | Done | utf8mb4 collation in information_schema |
| M36 Multi-schema | Done | `CREATE/DROP DATABASE`, `USE` |
| M37 AUTO_INCREMENT | Done | `AUTO_INCREMENT` columns |
| M38 ALTER extended | Done | `DROP/MODIFY/RENAME COLUMN`, `RENAME TABLE` |
| M40 Extended types | Done | `DECIMAL`, `DATETIME`, `TEXT`, `BLOB`, `JSON` |
| M41 OUTER JOIN | Done | `LEFT`/`RIGHT OUTER JOIN` |
| M43 GROUP BY | Done | `GROUP BY`, `HAVING`, aggregates |
| M42 Subqueries | Done | `IN (SELECT …)`, `EXISTS`, derived tables |
| M46 Expressions | Done | arithmetic, `CONCAT`, `COALESCE`, `CAST`, builtins |
| M45 Extended WHERE | Done | `OR`/`NOT`/`LIKE`/`BETWEEN`/`IN` |
| M39 FOREIGN KEY | Done | `FOREIGN KEY`, RESTRICT on DML |
| M44 UNION | Done | `UNION` / `UNION ALL` |
| M49 Cost planner | Done | `EXPLAIN`, index/range access paths |
| M50 Composite indexes | Done | multi-column secondary indexes |
| M54 GRANT/REVOKE | Done | privilege checks, `SHOW GRANTS` |
| M182 Roles | Done | `CREATE ROLE`, `GRANT role`, `SET ROLE`, `CURRENT_ROLE()` (#334) |
| M183 Password policy | Done | `PASSWORD EXPIRE` (errno 1820), reuse (3638), `validate_password` (1819) (#335) |
| M184 REQUIRE SSL | Done | Per-account `REQUIRE SSL`; plaintext login errno 1045 (#336) |
| M55 Multi-user auth | Done | `CREATE USER` / `DROP USER`, `mysql_native_password` |
| M60 mysql-test 100+ | Done | 100-case wire harness + CI pass floor |
| M51 CHANGE_USER | Done | `COM_CHANGE_USER`, `COM_RESET_CONNECTION` |
| M52 Field list / stmt reset | Done | `COM_FIELD_LIST`, `COM_STMT_RESET`, long data |
| M53 PROCESSLIST | Done | `SHOW PROCESSLIST`, `COM_PROCESS_INFO` |
| M59 Collation | Done | `utf8mb4_unicode_ci` compare/sort for ORDER BY and WHERE |
| M61 Sysbench harness | Done | `oltp_point_select` vs MySQL; `scripts/sysbench-rusql.mjs` |
| PERF-B1–B6 | Done | Persistent bench, scan/PK tuning, WAL sync, sysbench CI |
| M47 Procedures | Done | `CREATE PROCEDURE` / `CALL` / `DROP` |
| M48 Triggers | Done | BEFORE INSERT; AFTER UPDATE/DELETE |
| M63 Functions | Done | `CREATE FUNCTION` scalar in `SELECT` |
| M56–M58 Replication | Done | Binlog on COMMIT, replica applier, GTID set (MVP) |
| M173 GTID event 33 | Done | `GTID_LOG_EVENT` before row events; dump + applier (#325) |
| M174 Binlog heartbeat | Done | Idle `COM_BINLOG_DUMP` follow sends type 27; applier ignores (#326) |
| M175 SHOW MASTER STATUS | Done | Live `File`/`Position`; `SHOW BINARY LOG STATUS` synonym (#327) |
| M177 Replica promote | Done | Manual `SET GLOBAL read_only=OFF` after fencing; no auto failover (#329) |
| M178 Semi-sync | Done | Dump request or ACK is errno 1524; no ACK wait; durability stays async WAL (#330) |
| M179 CHANGE MASTER | Done | `CHANGE MASTER TO` / `START SLAVE` connect, dump, and apply; errnos 1200/1198/1231/2003 (#331) |
| M180 Row event v2 | Done | `WRITE`/`UPDATE`/`DELETE` rows v2; `binlog_row_image` FULL/MINIMAL/NOBLOB; partial UPDATE keeps other columns (#332) |
| M72 INSERT row events | Done | `TABLE_MAP` + `WRITE_ROWS` for committed INSERT |
| M73 Live dump follow | Done | `COM_BINLOG_DUMP` flag 0 streams later COMMITs |
| M74 UPDATE/DELETE row events | Done | `TABLE_MAP` + `UPDATE_ROWS`/`DELETE_ROWS` |
| M62–M70 Query SQL | Done | Collation 0900, session funcs, CASE/IF, DISTINCT, INSERT SELECT, CTE, windows |
| M71–M81 Protocol/session | Done | Binlog dump/rows, LAST_INSERT_ID, @@vars, SHOW/SET VARIABLES |
| M82–M99 Client SHOW | Done | SET NAMES, FOR UPDATE syntax (M157 row waits), SHOW STATUS/ENGINES/WARNINGS/CREATE * |
| M100–M109 Events | Done | SHOW/CREATE/ALTER EVENT; AT/EVERY scheduler; DEFINER / ON COMPLETION; COMMENT; `information_schema.EVENTS` |
| M110 TRUNCATE | Done | Heap delete-all + `AUTO_INCREMENT` reset |
| M111 REPLACE INTO | Done | PK conflict delete-then-insert (`affected_rows` 1 or 2) |
| M112 INSERT IGNORE | Done | Skip PRIMARY KEY conflicts; insert the rest |
| M113 SUBSTRING / ROUND / DATE_ADD | Done | 1-based `SUBSTRING`/`SUBSTR`; `ROUND` half-away-from-zero; `DATE_ADD` INTERVAL |
| Phase Q (M62–M113) | Done | Filed table on `main`; official MySQL CLI session introspection has no `unsupported function` |
| M114–M119 | Done | charset DDL, `JSON_EXTRACT`, `UUID()`, `LAST_INSERT_ID(expr)`, `GET_LOCK`, `TABLE_CONSTRAINTS` |
| M120 PROCESSLIST I_S | Done | `information_schema.PROCESSLIST` live session rows (`ID` = `CONNECTION_ID()`) |
| M121 PARAMETERS I_S | Done | `information_schema.PARAMETERS` catalog view (M132 persists `IN` rows) |
| M122 SHOW BINARY LOGS | Done | Lists known `binlog.NNNNNN` files (`Log_name`, `File_size`) |
| M123 SHOW BINLOG EVENTS | Done | Lists real events from known binlog files (`Log_name`, `Pos`, `Event_type`, …) |
| M124 CREATE OR REPLACE VIEW | Done | Creates or replaces the stored SELECT; table of the same name is errno 1347 |
| M125 Text PREPARE / EXECUTE | Done | Session-scoped `PREPARE FROM` / `EXECUTE` / `DEALLOCATE PREPARE`; unknown name errno 1243 |
| M126 SAVEPOINT | Done | Named savepoints; `ROLLBACK TO` restores overlay; `RELEASE` then `ROLLBACK TO` is errno 1305 |
| M127 WITH RECURSIVE | Done | Counting `UNION ALL` recursion; cap 1000 is errno 3636; not SEARCH/CYCLE |
| M128 INTERSECT | Done | Distinct set intersection of SELECT results; column mismatch errno 1222; not INTERSECT ALL / EXCEPT |
| M129 Window ROWS BETWEEN | Done | Ranking windows honor `ROWS BETWEEN` bounds (`UNBOUNDED PRECEDING` / `CURRENT ROW` / `n PRECEDING` / `FOLLOWING`); `RANGE` still errors |
| M130 DISABLE ON SLAVE | Done | Persist `CREATE EVENT … DISABLE ON SLAVE`; scheduler skips; `SHOW CREATE EVENT` reconstructs |
| M131 SHOW ENGINE INNODB STATUS | Done | Documented stub (`Type`/`Name`/`Status`); unknown engine errno 1286; not live mutex/lock stats |
| M132 Procedure IN parameters | Done | `CREATE PROCEDURE (IN x INT)` / `CALL p(literal)`; `SHOW CREATE PROCEDURE` lists `IN x INT` |
| M165 OUT / INOUT procedure params | Done | `CALL p(@var)` assigns `OUT`/`INOUT`; literal OUT arg errno 1414 |
| M166 SIGNAL / RESIGNAL | Done | `SIGNAL SQLSTATE` in procedures; `RESIGNAL` outside handler errno 1645; `RESIGNAL` in handler re-raises (M170) |
| M167 DECLARE variables | Done | Procedure-local `DECLARE` + `SET` / `SELECT INTO`; misplaced DECLARE errno 1337 |
| M168 IF / WHILE / LOOP / LEAVE | Done | Control flow in `CALL`; bad `LEAVE` errno 1308; 1M iteration cap per `CALL`; not REPEAT/CASE/GOTO |
| M169 Cursors | Done | `DECLARE CURSOR` / `OPEN` / `FETCH INTO` / `CLOSE`; one open cursor per `CALL`; FETCH past end errno 1329 (M170 NOT FOUND handler) |
| M170 Condition handlers | Done | `CONTINUE`/`EXIT HANDLER FOR NOT FOUND / SQLEXCEPTION / errno; not UNDO / GET DIAGNOSTICS |
| M171 Trigger timings | Done | All six row timings; `BEFORE` `SIGNAL` abort; `AFTER INSERT`; not statement-level / FOLLOWS |
| M133 JSON_UNQUOTE / -> / ->> | Done | Unquote JSON strings; `col->'$.a'` is extract; `col->>'$.a'` is unquote(extract); not JSON_SET |
| M134 JSON_OBJECT / JSON_ARRAY / JSON_SET | Done | Construct/update JSON; `$.key` / `$.a.b` paths; odd `JSON_OBJECT` args errno 1582 |
| M135 LAG / LEAD / SUM() OVER | Done | Offset windows (default 1; first LAG / last LEAD NULL); running `SUM` per partition; not NTH_VALUE / RANGE |
| M136 EXCEPT / EXCEPT ALL | Done | Distinct set difference; `EXCEPT ALL` keeps leftover left duplicates; column mismatch errno 1222; not MINUS |
| M137 FULL OUTER JOIN | Done | Two-table `FULL OUTER JOIN` / `FULL JOIN` NULL-pads unmatched rows on both sides; `ON` required; not USING / natural |
| M138 VALUES row constructor | Done | `VALUES ROW(1), ROW(2)` top-level or in `FROM`; default columns `column_0`, … |
| M139 CAST / CONVERT charset | Done | `CONVERT(… USING utf8mb4)` identity; unknown charset errno 1115 |
| M140 Date pack | Done | `DATE_SUB`/`DATEDIFF`/`DATE_FORMAT` portable subset; not `STR_TO_DATE` |
| M141 String pack | Done | `TRIM`/`LTRIM`/`RTRIM`, `REPLACE`, `SUBSTRING_INDEX`; not `REGEXP` |
| M142 GREATEST / LEAST | Done | Vararg max/min; any NULL → NULL; arity &lt; 2 errno 1582; not window fns |
| M143 INSERT SET | Done | `INSERT INTO t SET col=expr, …` one-row shorthand |
| M144 Multi-table DML | Done | Two-table `INNER JOIN` `UPDATE` / `DELETE` |
| M145 EXPLAIN FORMAT=JSON | Done | Documented JSON stub; plain EXPLAIN unchanged |
| M150 Invisible columns / indexes | Done | `SELECT *` omits invisible cols; invisible indexes skipped by planner |
| M152 RANGE partitioning | Done | `PARTITION BY RANGE` CREATE + INSERT route + SELECT/EXPLAIN prune |
| M153 ALTER index DDL | Done | `ALTER TABLE ADD/DROP INDEX`, `DROP INDEX ON`, `ADD UNIQUE` dump forms |
| M155 RENAME TABLE multi-pair | Done | `RENAME TABLE a TO b, c TO a` atomic swap; errno 1146/1050 |
| M156 COLUMNS extras | Done | `information_schema.COLUMNS`: `COLUMN_DEFAULT`, `EXTRA`, `COLUMN_KEY` |
| M154 CREATE TABLE LIKE / CTAS | Done | Schema copy via `LIKE`; `AS SELECT` creates + loads rows |
| M151 Functional indexes | Done | `INDEX ((expr))` equality lookup + EXPLAIN ref |
| M149 DEFAULT expressions | Done | `DEFAULT (expr)` on INSERT omit; literal defaults; `SET = DEFAULT` |
| M148 ENUM / SET types | Done | Member validation; SET canonical order; strict errno 1265 |
| M147 CHECK constraints | Done | CREATE TABLE CHECK; INSERT/UPDATE errno 3819 |
| M146 VIRTUAL generated columns | Done | `AS (expr) VIRTUAL`; SELECT compute; errno 3105 on write |

**Roadmap**: [mysql-compat-roadmap.md](docs/en/specs/mysql-compat-roadmap.md) · **Book**: [docs/book/README.md](docs/book/README.md)

## Quick Start

```bash
cargo build --release
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

Data is persisted to `rusql-data/rusql.wal` and survives server restarts.

With MySQL client:

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

Optional TLS (M181): `--ssl-cert` and `--ssl-key` together. The server presents that certificate so `mysql --ssl-mode=REQUIRED` connects. Without the flags, `REQUIRED` fails and local plaintext tests stay unchanged. See [adr-m181-tls.md](docs/en/architecture/adr-m181-tls.md).

Account `REQUIRE SSL` (M184): that account’s plaintext login is errno 1045; TLS login succeeds. Other accounts stay plaintext. Not `REQUIRE X509`. See [adr-m184-require-ssl.md](docs/en/architecture/adr-m184-require-ssl.md).

Roles (M182): `CREATE ROLE`, `GRANT role TO user`, and `SET ROLE` activate that role's privileges. `CURRENT_ROLE()` reports them. See [adr-m182-roles.md](docs/en/architecture/adr-m182-roles.md).

Password policy (M183): `ALTER USER … PASSWORD EXPIRE` still allows login, then `COM_QUERY` is errno 1820 until the password changes. Reuse is errno 3638. `validate_password` is off until `SET GLOBAL`. See [adr-m183-password-policy.md](docs/en/architecture/adr-m183-password-policy.md).

## Architecture

```
crates/
├── rusql-i18n       # Internationalization (en-US, zh-CN)
├── rusql-protocol   # MySQL wire protocol
├── rusql-sql        # SQL parsing (sqlparser)
├── rusql-core       # Catalog, session, types
├── rusql-storage    # Storage engine trait + implementations
├── rusql-executor   # Query executor
├── rusql-planner    # Query planner (pass-through MVP)
├── rusql-server     # TCP server binary
└── rusql-cli        # Admin CLI
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```

See [AGENTS.md](AGENTS.md) and [docs/en/workflows/spec-to-ship.md](docs/en/workflows/spec-to-ship.md).

## License

Apache-2.0
