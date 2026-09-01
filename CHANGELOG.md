# Changelog

All notable changes to **rusql** are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).  
User-friendly summaries and verification steps: [docs/en/release-notes.md](docs/en/release-notes.md).

## [Unreleased]

### Fixed

- **Issue #87** — `COM_PING` (0x0E) returns OK; `mysqladmin ping` succeeds.
- **Issue #77** — `COM_INIT_DB` (0x02) for official client `USE rusql`.
- **Issue #73 / #79 / #80** — Metadata EOF/OK after resultset column definitions; `CLIENT_SESSION_TRACK` session-state trailer on OK and OK-as-EOF packets; command-phase OK packets use negotiated client capabilities.
- **Issue #73** — Strip WL#12542 `COM_QUERY` query-attributes when `CLIENT_QUERY_ATTRIBUTES` is negotiated; OK-as-EOF resultset trailers for MySQL 8.0 (`CLIENT_DEPRECATE_EOF`).

### Added

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
