# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-17 |
| Branch | feat/m95-show-create-procedure |
| Next step | Merge M95 SHOW CREATE PROCEDURE stubs (issue #223) then file M96 |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #222) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M94 | Merged (#162–#222) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SHOW CREATE FUNCTION` stubs — next Phase Q slice after M95
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M95 in progress** — `SHOW CREATE PROCEDURE` (#223): MySQL-shaped columns from catalog `ProcedureMeta`; reconstructed `CREATE PROCEDURE …() BEGIN … END`; empty params; stub sql_mode/charset; unknown procedure errno 1305
- **#222 merged** — M94 `SHOW CREATE TRIGGER` (#221): MySQL-shaped columns `Trigger`/`sql_mode`/`SQL Original Statement`/charset stubs; DDL reconstructed from catalog `TriggerMeta`; unknown trigger errno 1360; `SHOW TRIGGERS` / `SHOW CREATE VIEW` / `SHOW CREATE TABLE` unchanged
- **#220 merged** — M93 `SHOW TRIGGERS` (#219): MySQL-shaped columns from catalog `TriggerMeta`; stub Definer/sql_mode/charset; `LIKE` / optional `FROM`/`IN` db; unknown db errno 1049; `SHOW CREATE VIEW` / `SHOW CREATE TABLE` / `SHOW CREATE DATABASE` unchanged
- **#218 merged** — M92 `SHOW CREATE VIEW` (#217): MySQL-shaped columns `View`/`Create View`/`character_set_client`/`collation_connection`; DDL reconstructed from catalog SELECT; unknown view errno 1146; `SHOW CREATE TABLE` / `SHOW CREATE DATABASE` / `SHOW WARNINGS` unchanged
- **#216 merged** — M91 `SHOW CREATE DATABASE` (#215): MySQL-shaped columns `Database`/`Create Database`; documented stub DDL with `utf8mb4` / `utf8mb4_unicode_ci`; unknown database errno 1049; `SHOW CREATE SCHEMA` equivalent; `SHOW CREATE TABLE` / `SHOW WARNINGS` unchanged
- **#214 merged** — M90 `SHOW WARNINGS` (#213): MySQL-shaped columns `Level`/`Code`/`Message`; documented empty list after a successful statement; `SHOW ERRORS` equivalent; `SHOW CHARACTER SET` / `SHOW ENGINES` unchanged
- **#212 merged** — M89 `SHOW CHARACTER SET` (#211): MySQL-shaped columns; documented stub `utf8mb4` with `utf8mb4_unicode_ci` / `Maxlen` 4; `SHOW CHARSET` equivalent; `LIKE` on `Charset`; `SHOW COLLATION` / `SHOW ENGINES` / `SET CHARACTER SET` unchanged
- **#210 merged** — M88 `SHOW ENGINES` (#209): MySQL-shaped columns; documented stub set `InnoDB` DEFAULT plus `MEMORY` / `MyISAM` / `PERFORMANCE_SCHEMA`; `SHOW STORAGE ENGINES` equivalent; `SHOW TABLE STATUS` / `SHOW STATUS` unchanged
- **#208 merged** — M87 `SHOW TABLE STATUS` (#207): MySQL-shaped columns; real `Name` set matching `SHOW TABLES`; stub `Engine=InnoDB`; `Rows`/optional `Auto_increment` from catalog; `LIKE` / optional `FROM` db
- **#206 merged** — M86 `SHOW STATUS` (#205): `SHOW STATUS` / `SHOW SESSION STATUS` / `SHOW GLOBAL STATUS` documented stub catalog (`LIKE` filter; session=global; `Threads_connected` follows the connection registry)
- **#204 merged** — M85 `SELECT … FOR UPDATE` (#203): `FOR UPDATE` / `FOR SHARE` / `LOCK IN SHARE MODE` (+ `NOWAIT` / `SKIP LOCKED`) documented no-op; same rows as unlocked SELECT; no row locks; concurrent connections both see the row
- **#202 merged** — M84 `SET TRANSACTION ISOLATION LEVEL` (#201): session overlay of `@@transaction_isolation` / `@@tx_isolation`; `SET GLOBAL TRANSACTION` errno 1229; DML stays snapshot; reset on `COM_RESET_CONNECTION` / `COM_CHANGE_USER`
- **#200 merged** — M83 `SET CHARACTER SET` / `SET CHARSET` / `SELECT @foo := expr` (#199): charset aliases of `SET NAMES`; inline user-var assignment; reset on `COM_RESET_CONNECTION` / `COM_CHANGE_USER`
- **#198 merged** — M82 `SET NAMES` / `@foo` (#197): charset overlays and user variables; reset on `COM_RESET_CONNECTION` / `COM_CHANGE_USER`
- **#196 merged** — M81 `SET @@` session overlays (#195): per-connection in-memory SET; `SET GLOBAL` errno 1229; read-only stubs errno 1238
- **#194 merged** — M80 `SHOW VARIABLES` stub catalog (#193): session/global lists and `LIKE` over the M77+M79 `@@` set
- **#192 merged** — M79 more `@@` connector probes (#191): `auto_increment_increment`, `time_zone`/`system_time_zone`, `transaction_isolation`/`tx_isolation`, `max_allowed_packet`, `license`
- **#190 merged** — M78 `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` (#189)
- **#188 merged** — M77 `@@` session/system variable stubs (#187)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```
