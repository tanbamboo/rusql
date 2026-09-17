# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-17 |
| Branch | feat/m91-show-create-database |
| Next step | After merge: file next Phase Q gap (M92 `SHOW CREATE VIEW` stubs) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #214); M91 this branch |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M90 | Merged (#162–#214) |
| M91 | This PR — `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` stubs (#215) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SHOW CREATE VIEW` stubs — file as M92 after this PR merges
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M91 this PR** — `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` (#215): MySQL-shaped columns `Database`/`Create Database`; documented stub DDL with `utf8mb4` / `utf8mb4_unicode_ci`; unknown database errno 1049; `SHOW CREATE TABLE` / `SHOW WARNINGS` unchanged
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
