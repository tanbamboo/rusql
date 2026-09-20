# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | main |
| Next step | Post-Q: file the next high-ROI gap-probe slice (`CREATE DATABASE … CHARACTER SET` or `JSON_EXTRACT` / `UUID()`). Phase Q (M62–M113 + session-introspection exit) is complete. |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #263 / run [35499832912](https://github.com/tanbamboo/rusql/actions/runs/35499832912)) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — all filed issues closed; last merge M113 PR #263 |
| Phase Q exit | **Met** — official MySQL CLI session introspection has no `unsupported function` |
| Estimated surface | ~45–70% client-visible; remaining work is post-Q |

## Gaps (post-Q)

Gap probe `scripts/mysql-gap-probe.mjs` on `main` after M113: **29 probes, 19 rusql gaps**, 9 ok, 1 both-fail (`CREATE PROCEDURE … IN` / `DELIMITER`).

Session exit check (Docker `mysql:8.0` client → rusql): `DATABASE`/`SCHEMA`/`USER`/`CURRENT_USER`/`SESSION_USER`/`VERSION`/`CONNECTION_ID`/`ROW_COUNT`/`LAST_INSERT_ID()`/`FOUND_ROWS()`, `@@version`/`@@autocommit`/`@@sql_mode`/`@@character_set_client`, `SHOW VARIABLES`, `SET NAMES`, `SET @@autocommit` — all OK, no `unsupported function`.

Remaining probe gaps (not Phase Q): `DISABLE ON SLAVE`, `CREATE DATABASE … CHARACTER SET`, `JSON_EXTRACT`, `UUID()`, `LAST_INSERT_ID(expr)`, `GET_LOCK`, `information_schema.TABLE_CONSTRAINTS` / `PARAMETERS` / `PROCESSLIST`, `SHOW ENGINE INNODB STATUS`, `SHOW BINARY LOGS` / `SHOW BINLOG EVENTS`, `CREATE OR REPLACE VIEW`, `PREPARE`/`EXECUTE` (text), window frames, `WITH RECURSIVE`, `INTERSECT`, `SAVEPOINT`. `information_schema.EVENTS` query succeeds (not 1146) but empty-catalog batch headers can `shape_mismatch` vs MySQL. `SHOW EVENTS` stays 15 columns. GTID event 33 / heartbeat stay later.

## Recent Progress

- **Phase Q complete** — filed table M62–M113 on `main`; session CLI exit verified (2026-09-20)
- **#263 merged** — M113 `SUBSTRING`/`ROUND`/`DATE_ADD` (#255)
- **#262 merged** — M112 `INSERT IGNORE` (#254)
- **#261 merged** — M111 `REPLACE INTO` (#253)
- **#260 merged** — M110 `TRUNCATE TABLE` (#252)
- **#259 merged** — M109 `information_schema.EVENTS` (#251)
- **#258 merged** — M108 Event `COMMENT` (#250)
- **User-facing report** — [rusql vs MySQL](docs/en/reports/rusql-vs-mysql.md): not a production drop-in; `mysql-diff` suites plus gap probe; works/stub/missing matrix

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
node scripts/mysql-gap-probe.mjs   # inventory only; not a CI gate
```
