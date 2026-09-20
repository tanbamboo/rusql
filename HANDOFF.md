# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | issue-254-m112-insert-ignore |
| Next step | Implement [M113 SUBSTRING / ROUND / DATE_ADD](https://github.com/tanbamboo/rusql/issues/255) (`agent-ready` after M112 merge) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #261) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M112 | M62–M111 merged (#162–#261); M112 `INSERT IGNORE` implemented on this branch (#254) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SUBSTRING` / `ROUND` / `DATE_ADD` — [M113 #255](https://github.com/tanbamboo/rusql/issues/255)

Later (probe 2026-09-20, not filed): `DISABLE ON SLAVE`, `CREATE DATABASE … CHARACTER SET`, `JSON_EXTRACT`, `UUID()`, `LAST_INSERT_ID(expr)`, `GET_LOCK`, `information_schema.TABLE_CONSTRAINTS` / `PARAMETERS` / `PROCESSLIST`, `SHOW ENGINE INNODB STATUS`, `SHOW BINARY LOGS` / `SHOW BINLOG EVENTS`, `CREATE OR REPLACE VIEW`, `PREPARE`/`EXECUTE` (text), window frames, `WITH RECURSIVE`, `INTERSECT`, `SAVEPOINT`. GTID event 33 / heartbeat stay later. Do not add a last-executed column to `SHOW EVENTS` (MySQL 8.0 has 15 columns).

## Recent Progress

- **M112** — `INSERT IGNORE` skips PRIMARY KEY conflicts without changing the existing row (`affected_rows` = rows actually inserted); new PKs insert as usual; multi-row skips duplicates and inserts the rest. Plain `INSERT` 1062, M68 ODKU, and M111 REPLACE unchanged (#254)
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
