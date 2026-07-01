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
