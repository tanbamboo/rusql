# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | issue-255-m113-substring-round-date-add |
| Next step | Reassess gap probe / Phase Q exit criteria. Filed Phase Q table M62–M113 is implemented (M113 on this branch). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #262) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M113 | M62–M112 merged (#162–#262); M113 `SUBSTRING`/`ROUND`/`DATE_ADD` implemented on this branch (#255) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

Filed M108–M113 are implemented. After this PR merges, reassess `scripts/mysql-gap-probe.mjs` and Phase Q exit (CLI/ORM session introspection without `unsupported function`).

Later (probe 2026-09-20, not filed): `DISABLE ON SLAVE`, `CREATE DATABASE … CHARACTER SET`, `JSON_EXTRACT`, `UUID()`, `LAST_INSERT_ID(expr)`, `GET_LOCK`, `information_schema.TABLE_CONSTRAINTS` / `PARAMETERS` / `PROCESSLIST`, `SHOW ENGINE INNODB STATUS`, `SHOW BINARY LOGS` / `SHOW BINLOG EVENTS`, `CREATE OR REPLACE VIEW`, `PREPARE`/`EXECUTE` (text), window frames, `WITH RECURSIVE`, `INTERSECT`, `SAVEPOINT`. GTID event 33 / heartbeat stay later. Do not add a last-executed column to `SHOW EVENTS` (MySQL 8.0 has 15 columns).

## Recent Progress

- **M113** — `SUBSTRING`/`SUBSTR` (MySQL 1-based), `ROUND` half-away-from-zero, `DATE_ADD` INTERVAL (`DAY` date-only result is `YYYY-MM-DD`; `MONTH`/`YEAR` 30/365-day) (#255)
- **#262 merged** — M112 `INSERT IGNORE` (#254)
- **M111** — `REPLACE INTO` inserts on a new PK (`affected_rows` 1) and delete-then-inserts on single-column PK conflict (`affected_rows` 2); ODKU and `INSERT IGNORE` unchanged (#253)
- **M110** — `TRUNCATE TABLE` / `TRUNCATE t` empties the heap and resets `AUTO_INCREMENT` to 1; unknown table errno 1146; no DELETE triggers (#252, PR #260)
- **#259 merged** — M109 `information_schema.EVENTS` (#251)
- **M108** — Event `COMMENT` persists; `SHOW CREATE EVENT` reconstructs `COMMENT '…'` when set and omits when empty (#250)
- **User-facing report** — [rusql vs MySQL](docs/en/reports/rusql-vs-mysql.md): not a production drop-in; `mysql-diff` suites plus gap probe; works/stub/missing matrix
- **#249 merged** — M107 event `DEFINER` / `ON COMPLETION` (#248): persist definer and completion; `PRESERVE` keeps due `AT` as `DISABLED`
- **Gap probe** — `scripts/mysql-gap-probe.mjs` vs Docker MySQL 8.0: 29 probes, 26 rusql gaps; `CREATE TEMPORARY TABLE` and `UNIQUE` already work
- **#247 merged** — M106 event `STARTS`/`ENDS` (#246)

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
