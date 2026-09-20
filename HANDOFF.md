# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | issue-252-m110-truncate-table |
| Next step | Implement [M111 REPLACE INTO](https://github.com/tanbamboo/rusql/issues/253) (`agent-ready` after M110 merge) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #259) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M110 | M62–M109 merged (#162–#259); M110 `TRUNCATE TABLE` implemented on this branch (#252) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `REPLACE INTO` — [M111 #253](https://github.com/tanbamboo/rusql/issues/253)
2. `INSERT IGNORE` — [M112 #254](https://github.com/tanbamboo/rusql/issues/254)
3. `SUBSTRING` / `ROUND` / `DATE_ADD` — [M113 #255](https://github.com/tanbamboo/rusql/issues/255)

Later (probe 2026-09-20, not filed): `DISABLE ON SLAVE`, `CREATE DATABASE … CHARACTER SET`, `JSON_EXTRACT`, `UUID()`, `LAST_INSERT_ID(expr)`, `GET_LOCK`, `information_schema.TABLE_CONSTRAINTS` / `PARAMETERS` / `PROCESSLIST`, `SHOW ENGINE INNODB STATUS`, `SHOW BINARY LOGS` / `SHOW BINLOG EVENTS`, `CREATE OR REPLACE VIEW`, `PREPARE`/`EXECUTE` (text), window frames, `WITH RECURSIVE`, `INTERSECT`, `SAVEPOINT`. GTID event 33 / heartbeat stay later. Do not add a last-executed column to `SHOW EVENTS` (MySQL 8.0 has 15 columns).

## Recent Progress

- **M110** — `TRUNCATE TABLE` / `TRUNCATE t` empties the heap and resets `AUTO_INCREMENT` to 1; unknown table errno 1146; no DELETE triggers (#252)
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
