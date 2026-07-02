# Changelog

All notable changes to **rusql** are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).  
User-friendly summaries and verification steps: [docs/en/release-notes.md](docs/en/release-notes.md).

## [Unreleased]

### Added

- **M12** — `DESCRIBE` / `SHOW COLUMNS` and minimal `information_schema.tables` / `information_schema.columns`.
- **M13** — `SHOW CREATE TABLE` with MySQL-style DDL output.
- **Book** — mdBook en/zh-CN: *Building a MySQL-like Database with AI and Harness Engineering* (#28).
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
