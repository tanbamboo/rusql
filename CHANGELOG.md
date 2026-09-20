# Changelog

All notable changes to **rusql** are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).  
User-friendly summaries and verification steps: [docs/en/release-notes.md](docs/en/release-notes.md).

## [Unreleased]

### Added

- **M108** — Event `COMMENT` persists on `EventMeta` (`serde(default)`). `CREATE EVENT … COMMENT 'text' DO …` and `ALTER EVENT … COMMENT 'text'` update the catalog. `SHOW CREATE EVENT` reconstructs `COMMENT '…'` when set and omits the clause when empty/unset. `SHOW EVENTS` stays 15 columns. M107 DEFINER / ON COMPLETION, M106 `STARTS`/`ENDS`, and `SHOW CREATE USER` from M99 are unchanged. `programs.json` without `comment` still loads (#250).
- **M109** — `information_schema.EVENTS` / `information_schema.events` lists catalog events (not errno 1146). Columns: `EVENT_SCHEMA`, `EVENT_NAME`, `DEFINER` (empty → stub `root@%`, same as `SHOW EVENTS`), `EVENT_TYPE`, `EXECUTE_AT`, `INTERVAL_VALUE`, `INTERVAL_FIELD`, `STARTS`, `ENDS`, `STATUS`, `ON_COMPLETION` (unset → `NOT PRESERVE`), `LAST_EXECUTED` (from `EventMeta.last_executed`, empty when unset), `EVENT_COMMENT` (from `EventMeta.comment`, empty when unset). No invented timestamps. `SHOW EVENTS` stays 15 columns. M107 DEFINER / ON COMPLETION reconstruction is unchanged (#251).
- **Docs** — User-facing rusql vs MySQL 8.0 compatibility test report: production verdict, function matrix (works / stub / missing), and test inventory (`mysql-diff` 297/297, gap probe, mysql-test subset). [en](docs/en/reports/rusql-vs-mysql.md) · [zh-CN](docs/zh-CN/reports/rusql-vs-mysql.md).
- **Harness** — `scripts/mysql-gap-probe.mjs` inventories client SQL vs rusql (optional Docker MySQL 8.0 status/column compare). Not a CI gate. Phase Q issues M108–M113 filed from the 2026-09-20 probe (#250–#255).
- **M107** — Event `DEFINER` and `ON COMPLETION` persist on `EventMeta`. `SHOW EVENTS` `Definer` and `SHOW CREATE EVENT` reconstruct `DEFINER=\`u\`@\`h\`` and `ON COMPLETION PRESERVE|NOT PRESERVE`. Due `AT` with `PRESERVE` stays `DISABLED` after run; `NOT PRESERVE` still drops (M104). Column count stays 15. M106 `STARTS`/`ENDS`, M105 watermark, and `SHOW CREATE USER` from M99 are unchanged (#248).
- **M106** — Event scheduler honors `STARTS` / `ENDS` on ENABLED `EVERY` events (`now < starts` or `now > ends` is not due; the window is inclusive). `CREATE EVENT` / `ALTER EVENT` persist the timestamps; `SHOW EVENTS` `Starts` / `Ends` and `SHOW CREATE EVENT` reconstruct them. Column count stays 15. M105 interval watermark, M104 one-time `AT` drop-after-run, and `SHOW CREATE USER` from M99 are unchanged (#246).
- **M105** — Event scheduler executes ENABLED `RECURRING` `EVERY n UNIT` events: first fire on the next COM_QUERY, then after `last_executed + interval` (internal watermark, not a `SHOW EVENTS` column). The catalog row stays. `MONTH`/`YEAR` use 30/365-day approximations. DISABLED `EVERY` is skipped. M104 one-time `AT` drop-after-run and `SHOW CREATE USER` from M99 are unchanged (#244).
- **M104** — Event scheduler executes ENABLED `ONE TIME` `AT` events when `execute_at` is due (UTC `YYYY-MM-DD HH:MM:SS`), then drops the catalog row (`ON COMPLETION NOT PRESERVE`). `@@event_scheduler` / `SHOW VARIABLES LIKE 'event_scheduler'` is a read-only `ON` stub (SET errno 1238; `SET GLOBAL` stays 1229). DISABLED and future `AT` events are not run. `DO` errors do not fail the client statement. Not a timer thread or DEFINER. `ALTER EVENT` from M103 and `SHOW CREATE USER` from M99 are unchanged (#242).
- **M103** — `ALTER EVENT name` updates catalog `EventMeta`: `ON SCHEDULE AT` / `EVERY n UNIT`, `ENABLE` / `DISABLE`, `RENAME TO`, and `DO stmt`. Unknown names are errno 1539. `SHOW EVENTS` / `SHOW CREATE EVENT` reflect the row. The scheduler does not run `DO`. Not DEFINER / ON COMPLETION / COMMENT. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged (#240).
- **M102** — `CREATE EVENT name ON SCHEDULE AT 'timestamp' DO stmt` and `EVERY n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR} DO stmt` persist `EventMeta` in `programs.json`. `SHOW EVENTS` lists catalog rows; `SHOW CREATE EVENT` reconstructs documented DDL. Duplicate names are errno 1537; unknown `SHOW CREATE EVENT` stays errno 1539. `DROP EVENT` / `IF NOT EXISTS` are accepted. The scheduler does not run `DO`. Not `ALTER EVENT`. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged (#238).
- **M101** — `SHOW EVENTS` / `SHOW EVENTS LIKE` (optional `FROM`/`IN` db) returns MySQL-shaped columns (`Db`, `Name`, `Definer`, `Time zone`, `Type`, `Execute at`, `Interval value`, `Interval field`, `Starts`, `Ends`, `Status`, `Originator`, `character_set_client`, `collation_connection`, `Database Collation`) over an empty catalog until `CREATE EVENT` exists. Unmatched `LIKE` returns zero rows. Unknown `FROM` db is errno 1049. Not reconstructed event DDL. `SHOW CREATE EVENT` from M100, `SHOW CREATE USER` from M99, and `SHOW FUNCTION STATUS` from M98 are unchanged (#236).
- **M100** — `SHOW CREATE EVENT` / `SHOW CREATE EVENT db.name` is accepted (not a parse error). With no event scheduler catalog yet, every name returns errno 1539 (`ER_EVENT_DOES_NOT_EXIST`). Not reconstructed event DDL, not `CREATE EVENT`, and not `SHOW EVENTS`. `SHOW CREATE USER` from M99, `SHOW FUNCTION STATUS` from M98, and `SHOW PROCEDURE STATUS` from M97 are unchanged (#234).
- **M99** — `SHOW CREATE USER` (named `'u'@'h'` / `user@host` / `CURRENT_USER`) returns a MySQL-shaped column (`CREATE USER for {user}@{host}`). The cell is reconstructed from the M55 account catalog (`CREATE USER \`u\`@\`h\` IDENTIFIED WITH '{plugin}'`) without a password hash, `BY`, or `AS` clause. Unknown accounts are errno 3162. Not TLS / resource-limit / DEFAULT ROLE dump. `SHOW FUNCTION STATUS` from M98, `SHOW PROCEDURE STATUS` from M97, and `SHOW CREATE FUNCTION` from M96 are unchanged (#232).
- **M98** — `SHOW FUNCTION STATUS` / `SHOW FUNCTION STATUS LIKE` returns MySQL-shaped columns (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from the M63 catalog; `Type` is `FUNCTION`. Definer / timestamps / charset cells are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. Not live DEFINER persistence. `SHOW PROCEDURE STATUS` from M97, `SHOW CREATE FUNCTION` from M96, and `SHOW CREATE PROCEDURE` from M95 are unchanged (#229).
- **M97** — `SHOW PROCEDURE STATUS` / `SHOW PROCEDURE STATUS LIKE` returns MySQL-shaped columns (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from the P3 catalog; `Type` is `PROCEDURE`. Definer / timestamps / charset cells are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. Not live DEFINER persistence and not `SHOW FUNCTION STATUS`. `SHOW CREATE FUNCTION` from M96, `SHOW CREATE PROCEDURE` from M95, and `SHOW CREATE TRIGGER` from M94 are unchanged (#227).
- **M96** — `SHOW CREATE FUNCTION` returns MySQL-shaped columns (`Function`, `sql_mode`, `Create Function`, `character_set_client`, `collation_connection`, `Database Collation`). `Create Function` is reconstructed from the M63 catalog (`CREATE FUNCTION …() RETURNS … BEGIN RETURN … END` with an empty parameter list); sql_mode / charset cells are documented stubs (empty sql_mode, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown functions are errno 1305. Not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE PROCEDURE` from M95, `SHOW CREATE TRIGGER` from M94, and `SHOW CREATE VIEW` from M92 are unchanged (#225).
- **M95** — `SHOW CREATE PROCEDURE` returns MySQL-shaped columns (`Procedure`, `sql_mode`, `Create Procedure`, `character_set_client`, `collation_connection`, `Database Collation`). `Create Procedure` is reconstructed from the P3 catalog (`CREATE PROCEDURE …() BEGIN … END` with an empty parameter list); sql_mode / charset cells are documented stubs (empty sql_mode, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown procedures are errno 1305. Not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE TRIGGER` from M94, `SHOW TRIGGERS` from M93, and `SHOW CREATE VIEW` from M92 are unchanged (#223).
- **M94** — `SHOW CREATE TRIGGER` returns MySQL-shaped columns (`Trigger`, `sql_mode`, `SQL Original Statement`, `character_set_client`, `collation_connection`, `Database Collation`, `Created`). `SQL Original Statement` is reconstructed from the M48 catalog (`CREATE TRIGGER … {BEFORE|AFTER} {INSERT|UPDATE|DELETE} ON … FOR EACH ROW …`); sql_mode / charset / Created cells are documented stubs (empty sql_mode/`Created`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown triggers are errno 1360. Not DEFINER / sql_mode dump. `SHOW TRIGGERS` from M93, `SHOW CREATE VIEW` from M92, and `SHOW CREATE TABLE` from M13 are unchanged (#221).
- **M93** — `SHOW TRIGGERS` (optional `FROM`/`IN` db and `LIKE`) returns MySQL-shaped columns (`Trigger`, `Event`, `Table`, `Statement`, `Timing`, `Created`, `sql_mode`, `Definer`, `character_set_client`, `collation_connection`, `Database Collation`). `Trigger` / `Event` / `Table` / `Timing` / `Statement` come from the M48 catalog; Definer / sql_mode / charset cells are documented stubs (`root@%`, empty sql_mode/`Created`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows; unknown `FROM` db is errno 1049. Not `SHOW CREATE TRIGGER` and not live DEFINER persistence. `SHOW CREATE VIEW` from M92, `SHOW CREATE TABLE` from M13, and `SHOW CREATE DATABASE` from M91 are unchanged (#219).
- **M92** — `SHOW CREATE VIEW` returns MySQL-shaped columns (`View`, `Create View`, `character_set_client`, `collation_connection`) with DDL reconstructed from the catalog SELECT (`CREATE VIEW … AS …`). Unknown views use missing-table handling (errno 1146). Not ALGORITHM / DEFINER / SQL SECURITY. `SHOW CREATE TABLE` from M13, `SHOW CREATE DATABASE` from M91, and `SHOW WARNINGS` from M90 are unchanged (#217).
- **M91** — `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` return MySQL-shaped columns (`Database`, `Create Database`) with documented stub DDL (`utf8mb4` / `utf8mb4_unicode_ci`). Unknown databases are errno 1049. This is not a live per-schema charset catalog and not `CREATE DATABASE … CHARACTER SET`. `SHOW CREATE TABLE` from M13 and `SHOW WARNINGS` from M90 are unchanged (#215).
- **M90** — `SHOW WARNINGS` / `SHOW ERRORS` return MySQL-shaped columns (`Level`, `Code`, `Message`) over a documented empty diagnostic list. After a successful statement with no diagnostics the result is zero rows (not an unsupported-statement error). This is not live truncation / sql_mode / note generation. `SHOW COUNT(*) WARNINGS`, `LIMIT`, and `WHERE` are not implemented. `SHOW CHARACTER SET` from M89 and `SHOW ENGINES` from M88 are unchanged (#213).
- **M89** — `SHOW CHARACTER SET` / `SHOW CHARSET` returns MySQL-shaped columns (`Charset`, `Description`, `Default collation`, `Maxlen`) over a documented stub set: `utf8mb4` (`Maxlen` `4`, default collation `utf8mb4_unicode_ci` matching M87 `SHOW TABLE STATUS`). `LIKE` filters on `Charset`. `SHOW COLLATION` from M59, `SHOW ENGINES` from M88, and `SET CHARACTER SET` from M83 are unchanged (#211).
- **M88** — `SHOW ENGINES` / `SHOW STORAGE ENGINES` returns MySQL-shaped columns (`Engine`, `Support`, `Comment`, `Transactions`, `XA`, `Savepoints`) over a documented stub set: `InnoDB` (`DEFAULT`), `MEMORY`, `MyISAM`, `PERFORMANCE_SCHEMA`. Values are constants (not live plugins). `SHOW TABLE STATUS` from M87 and `SHOW STATUS` from M86 are unchanged. `SHOW ENGINE INNODB STATUS` is not implemented (#209).
- **M87** — `SHOW TABLE STATUS` / `SHOW TABLE STATUS LIKE` (optional `FROM`/`IN` db) returns MySQL-shaped columns (`Name`, `Engine`, `Version`, `Row_format`, `Rows`, … `Collation`, `Comment`). `Name` matches `SHOW TABLES` in the current database; `Engine` is the stub `InnoDB`; `Rows` is the heap row count and `Auto_increment` follows the catalog counter when present. Other cells are documented stubs (not InnoDB tablespace stats). Unmatched `LIKE` returns zero rows. `SHOW STATUS` from M86 is unchanged (#207).
- **M86** — `SHOW STATUS` / `SHOW SESSION STATUS` / `SHOW GLOBAL STATUS` over a documented stub catalog (`Uptime`, `Threads_connected`, `Threads_running`, `Questions`, `Slow_queries`, `Open_tables`, `Connections`, `Aborted_connects`, `Bytes_received`, `Bytes_sent`). `LIKE` filters that set; session=global for this slice. Values are constants except `Threads_connected` (active connection count). Not the full MySQL 8.0 catalog (#205).
- **M85** — `SELECT … FOR UPDATE` / `FOR SHARE` / `LOCK IN SHARE MODE` (including `NOWAIT` and `SKIP LOCKED`) accepted as a documented no-op that returns the same rows as the unlocked `SELECT`. No row locks or wait. Concurrent connections both see the row. Engine isolation stays snapshot (#203).
- **M84** — `SET TRANSACTION ISOLATION LEVEL` / `SET SESSION TRANSACTION ISOLATION LEVEL` overlay `@@transaction_isolation` and `@@tx_isolation` in memory per connection (`READ-COMMITTED`, `REPEATABLE-READ`, `SERIALIZABLE`, `READ-UNCOMMITTED`). `SET GLOBAL TRANSACTION` is rejected (errno 1229). Engine isolation stays snapshot. `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore defaults (#201).
- **M83** — `SET CHARACTER SET` / `SET CHARSET` overlay the same charset stubs as `SET NAMES`; `SELECT @foo := expr` assigns and returns the value on that connection (#199).
- **M82** — `SET NAMES` charset/collation overlays and user variables `@foo` persist in memory per connection; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore defaults (#197).
- **M81** — `SET @@` / `SET SESSION` persistence for the documented stub catalog (in-memory per connection; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore defaults). `SET GLOBAL` is rejected (errno 1229). Read-only stubs `version`, `version_comment`, `license`, and `system_time_zone` reject SET (errno 1238). `SET NAMES` and user variables `@foo` remain unimplemented (#195).

- **M80** — `SHOW VARIABLES` / `SHOW SESSION VARIABLES` / `SHOW GLOBAL VARIABLES` over the documented M77+M79 stub catalog (`Variable_name`/`Value`); `LIKE` filters that set; session=global for this slice (#193).
- **M79** — Additional `@@` stubs for JDBC/ORM handshake probes: `auto_increment_increment` (`1`), `time_zone` (`SYSTEM`), `system_time_zone` (`UTC`), `transaction_isolation` / `tx_isolation` (`REPEATABLE-READ`), `max_allowed_packet` (`67108864`), `license` (`GPL`) (#191).
- **M78** — `FOUND_ROWS()` and `SELECT SQL_CALC_FOUND_ROWS … LIMIT` (un-LIMITed match count; deprecated MySQL 8.0.17 API) (#189).
- **M77** — `SELECT @@version`, `@@autocommit`, charset/collation, and `@@sql_mode` stubs for client probes; unknown names return errno 1193 (#187).
- **M76** — `CONNECTION_ID()` returns the session handshake thread id (same as `SHOW PROCESSLIST` `Id`); `ROW_COUNT()` returns the last DML affected-row count and `-1` after a `SELECT` (#185).
- **M75** — `LAST_INSERT_ID()` returns the first generated `AUTO_INCREMENT` value of the last successful `INSERT` on the connection; INSERT OK packets set `last_insert_id` to the same value (#183).
- **M74** — Committed `UPDATE`/`DELETE` write `TABLE_MAP` then `UPDATE_ROWS`/`DELETE_ROWS` (v1) instead of QUERY_EVENT (#181).
- **M73** — `COM_BINLOG_DUMP` with flags `0` stays open and streams later COMMITs; `BINLOG_DUMP_NON_BLOCK` (0x01) remains one-shot + OK (#179).
- **M72** — Committed `INSERT`s write `TABLE_MAP` then `WRITE_ROWS` (v1) instead of QUERY_EVENT; UPDATE/DELETE stay QUERY (#177).
- **M71** — `COM_BINLOG_DUMP` emits per-event packets (`0x00` + event) from the requested file position (#175).
- **M70** — Window ranking: `ROW_NUMBER()`, `RANK()`, `DENSE_RANK()` with `OVER (PARTITION BY … ORDER BY …)` (#173).
- **M69** — Non-recursive `WITH` CTEs (inlined as derived tables; `WITH RECURSIVE` rejected) (#171).
- **M68** — `INSERT … SELECT` and `ON DUPLICATE KEY UPDATE` (PRIMARY KEY upsert; `VALUES(col)` in the UPDATE clause) (#169).
- **M67** — `SELECT DISTINCT` (full-row dedupe; works with `ORDER BY` / `LIMIT` / JOINs) (#167).
- **M66** — `CASE` (searched + simple) and `IF(cond, then, else)` expressions (#165).
- **M65** — Session info functions: `DATABASE()`/`SCHEMA()`, `USER()`/`CURRENT_USER()`/`SESSION_USER()`, `VERSION()` (#163).
- **M63** — `CREATE FUNCTION` / `DROP FUNCTION`; scalar UDF calls in `SELECT` and expressions (#154).
- **M64** — `AFTER UPDATE` and `AFTER DELETE` triggers with `OLD`/`NEW` column substitution in side-effect DML (#155).
- **P3 MVP** — Stored procedures/triggers persisted in `programs.json`; `information_schema.ROUTINES` / `TRIGGERS`; binlog QUERY events on transaction `COMMIT`; `COM_BINLOG_DUMP` and `COM_REGISTER_SLAVE` protocol stubs; `apply_binlog_file` for replica replay.
- **M59** — `utf8mb4_unicode_ci` collation-aware `ORDER BY` and `WHERE =`; `SHOW COLLATION`; portable string corpus tests (#123).
- **M62** — `utf8mb4_0900_ai_ci` collation (MySQL 8.0 default) for `CREATE TABLE … COLLATE`, `ORDER BY`, and `WHERE =`; corpus tests (#153).
- **M61** — Sysbench `sbtest` schema docs and `scripts/sysbench-rusql.mjs` for `oltp_point_select` vs MySQL; optional CI workflow (#125).
- **PERF-B2** — Index-ordered scan for `ORDER BY` single indexed column + `LIMIT` without `WHERE`; avoids full in-memory sort (#127).
- **PERF-B3** — Primary-key `UPDATE` uses index lookup and incremental index maintenance instead of full index rebuild (#128).
- **M51** — `COM_CHANGE_USER` (0x11) re-auth with stored scramble; `COM_RESET_CONNECTION` (0x1f) clears prepared statements and transactions (#115).
- **M52** — `COM_FIELD_LIST` (0x04), `COM_STMT_RESET` (0x1A), `COM_STMT_SEND_LONG_DATA` (0x18) with long-parameter merge at execute (#116).
- **M53** — `SHOW PROCESSLIST`, `COM_PROCESS_INFO` (0x0A), shared `ConnectionRegistry` (#117).
- **PERF-B1** — Persistent-connection benchmark harness `scripts/bench-rusql-vs-mysql.mjs` + `wire-bench-client.mjs`; 7 baseline workloads with QPS/p50/p95 JSON output (#126).
- **PERF-B4** — Multi-threaded benchmark (`--threads`, `--duration`, `--thread-matrix`); per-thread QPS and read/write mix summaries (#129).
- **PERF-B5** — WAL sync policy (`--wal-sync=always|batch|none`); configurable durability vs throughput (#130).
- **PERF-B6** — Sysbench `oltp_point_select` gate script + optional CI workflow (#131).
- **Harness** — Full parity roadmap, performance benchmark report, GitHub issue body templates, `create-parity-issues.mjs`, and Vitess reference docs (en + zh-CN); gitignore `.bench-*.json` and `.test-data-*/`.
- **M40** — Extended column types (`DECIMAL`, `DATETIME`, `TEXT`, `BLOB`, `JSON`) with wire/DESCRIBE/`DATA_TYPE` metadata (#104).
- **M41** — `LEFT OUTER JOIN` / `RIGHT OUTER JOIN` with NULL padding (#105).
- **M43** — `GROUP BY`, `HAVING`, and `COUNT`/`SUM`/`MIN`/`MAX`/`AVG` aggregates (#107).
- **M42** — Subqueries: `IN (SELECT …)`, `EXISTS`, scalar subqueries, derived tables (#106).
- **M46** — SQL expressions: arithmetic, `CONCAT`, `COALESCE`/`IFNULL`/`NULLIF`, `CAST`, `NOW`/`CURDATE`, `LENGTH`/`LOWER`/`UPPER` (#110).
- **M60** — mysql-test wire subset expanded to 100+ portable cases with CI pass floor (#124).
- **M44** — `UNION` / `UNION ALL` result set combination (#108).
- **M39** — `FOREIGN KEY` constraints: CREATE TABLE declaration, INSERT/UPDATE/DELETE RESTRICT enforcement (errno 1451/1452), `information_schema.KEY_COLUMN_USAGE` stub.
- **M49** — Cost-based access paths: `EXPLAIN SELECT`, PK/secondary index point lookup, `BETWEEN` range scan (#113).
- **M54** — `GRANT` / `REVOKE` / `SHOW GRANTS`, privilege checks (errno 1142), `mysql.user.json` persistence (#118).
- **M50** — Composite secondary indexes `(a, b)`, prefix lookups, SHOW INDEX / STATISTICS seq (#114).
- **M55-auth** — Multi-user accounts: `CREATE USER` / `DROP USER`, persisted passwords in `mysql.user.json`, `mysql_native_password` login path (#119).
- **M32** — MVCC snapshot isolation: pinned read snapshots + `RwLock` for non-blocking reads (#55).
- **M33** — `CREATE VIEW` + `information_schema.VIEWS` (#56).
- **M34** — Binlog format spike (`FORMAT_DESCRIPTION` + `QUERY_EVENT`) and ADR update (#57).
- **M35** — utf8mb4 handshake charset and `COLUMN_COLLATION` in information_schema (#58).
- **Book (#28)** — mdBook en/zh-CN complete through M35 (*Building a MySQL-like Database with AI and Harness Engineering*).
- **M12** — `DESCRIBE` / `SHOW COLUMNS` and minimal `information_schema.tables` / `information_schema.columns`.
- **M13** — `SHOW CREATE TABLE` with MySQL-style DDL output.
- **M14** — `SELECT col1, col2 FROM tbl` column projection (not only `*`).
- **M15** — `USE rusql` / `USE DATABASE rusql` session default database.
- **M16** — `SELECT … LIMIT n` row cap on table queries.
- **M17** — `SELECT … ORDER BY col [ASC|DESC]` on table queries.
- **M18** — `SELECT col AS alias` output column names.
- **M19** — `SELECT … LIMIT n OFFSET m` pagination.
- **M20** — `WHERE` comparisons (`<`, `>`, `<=`, `>=`, `<>`) and `AND`.
- **M21** — `IS NULL` / `IS NOT NULL` in `WHERE`.
- **M22** — `INNER JOIN` two tables with `ON` equality.
- **M23** — `PRIMARY KEY` and `NOT NULL` catalog metadata in DESCRIBE.
- **M24** — `ALTER TABLE … ADD COLUMN` with WAL replay and catalog sync.
- **M25** — Binary resultset for `COM_STMT_EXECUTE` with typed column metadata.
- **M26** — `caching_sha2_password` RSA full-auth exchange for non-TLS clients.
- **M27** — `information_schema.SCHEMATA` and `STATISTICS` virtual tables.
- **M28** — `SHOW INDEX` / `SHOW INDEXES` / `SHOW KEYS FROM tbl` with MySQL-style columns.
- **M29** — `scripts/mysql-diff.mjs` differential compat vs Docker MySQL 8.0 (`compat/mysql-diff.json`).
- **M30** — Oracle mysql-test inspired wire subset (`tests/mysql-test/manifest.json`, `scripts/mysql-test-subset.mjs`).
- **M31** — `COMMIT` flushes transaction overlay to WAL; `ROLLBACK` discards without WAL append.

### Fixed

- **Issue #158** — `cargo fmt --check` on `projection_needs_eval` in `rusql-executor`.
- **Issue #159** — `mysql-diff` `multi_schema` suite: dynamic port per suite; USE via `-D` handshake (MySQL 8.0 CLI rejects `-e USE` on fresh TCP); status-only USE compare; spawn_blocking oracle tests; regression wire tests for `CREATE DATABASE` + `COM_INIT_DB`.
- **M62** — `utf8mb4_0900_ai_ci` collation for column-level `COLLATE`, `ORDER BY`, and `WHERE =`; `SHOW COLLATION` lists both utf8mb4 collations (#153).
- **Issue #77** — `COM_INIT_DB` (0x02) for official client `USE rusql`.
- **Issue #73 / #79 / #80** — Metadata EOF/OK after resultset column definitions; `CLIENT_SESSION_TRACK` session-state trailer on OK and OK-as-EOF packets; command-phase OK packets use negotiated client capabilities.
- **Issue #73** — Strip WL#12542 `COM_QUERY` query-attributes when `CLIENT_QUERY_ATTRIBUTES` is negotiated; OK-as-EOF resultset trailers for MySQL 8.0 (`CLIENT_DEPRECATE_EOF`).
- Text resultset rows encode SQL NULL as `0xFB` (MySQL-compatible `NULL` display in `mysql` client).

## [0.2.0] - 2026-06-30

### Added

- **M11** — `COM_STMT_PREPARE` / `COM_STMT_EXECUTE` / `COM_STMT_CLOSE` with `?` placeholder binding (VARCHAR / integer params).

## [0.1.0] - 2026-06-30

Milestone batch through M9 on `main` (pre-semver tagging; version tracks first public API slice).

### Added

- **M0** — Harness engineering bootstrap (sensors, issue loop, workspace crates).
- **M1** — MySQL wire protocol v10 handshake and OK/ERR packets.
- **M2** — `COM_QUERY` with `CREATE TABLE`, `INSERT`, `SELECT`.
- **M3** — WAL persistence (`--data-dir`, `rusql.wal` replay).
- **M4** — Secondary B+Tree index, `CREATE INDEX`, `WHERE col = literal`.
- **M5** — JSON compat fixture suite over wire protocol.
- **M6** — Optional password auth (`mysql_native_password`), `DROP TABLE`, `DELETE`.
- **M7** — Default `caching_sha2_password` auth plugin (fast-path verify).
- **M8** — `UPDATE … SET … WHERE` with WAL.
- **M9** — `BEGIN` / `COMMIT` / `ROLLBACK` (connection overlay, deferred WAL).
- Harness sensors: `metrics.mjs`, `doc-parity.mjs`, `check-handoff.mjs`, `mysql-diff.mjs`.

### Changed

- Issue loop and PR template require user-guide updates per milestone.

[Unreleased]: https://github.com/tanbamboo/rusql/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tanbamboo/rusql/releases/tag/v0.1.0
