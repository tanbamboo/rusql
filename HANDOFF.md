# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | docs/mysql-gap-issues |
| Next step | Implement [M108 Event COMMENT](https://github.com/tanbamboo/rusql/issues/250) (`agent-ready`) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #249) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M107 | Merged (#162–#249) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Event `COMMENT` — [M108 #250](https://github.com/tanbamboo/rusql/issues/250) (`agent-ready`)
2. `information_schema.EVENTS` — [M109 #251](https://github.com/tanbamboo/rusql/issues/251)
3. `TRUNCATE TABLE` — [M110 #252](https://github.com/tanbamboo/rusql/issues/252)
4. `REPLACE INTO` — [M111 #253](https://github.com/tanbamboo/rusql/issues/253)
5. `INSERT IGNORE` — [M112 #254](https://github.com/tanbamboo/rusql/issues/254)
6. `SUBSTRING` / `ROUND` / `DATE_ADD` — [M113 #255](https://github.com/tanbamboo/rusql/issues/255)

Later (probe 2026-09-20, not filed): `DISABLE ON SLAVE`, `CREATE DATABASE … CHARACTER SET`, `JSON_EXTRACT`, `UUID()`, `LAST_INSERT_ID(expr)`, `GET_LOCK`, `information_schema.TABLE_CONSTRAINTS` / `PARAMETERS` / `PROCESSLIST`, `SHOW ENGINE INNODB STATUS`, `SHOW BINARY LOGS` / `SHOW BINLOG EVENTS`, `CREATE OR REPLACE VIEW`, `PREPARE`/`EXECUTE` (text), window frames, `WITH RECURSIVE`, `INTERSECT`, `SAVEPOINT`. GTID event 33 / heartbeat stay later. Do not add a last-executed column to `SHOW EVENTS` (MySQL 8.0 has 15 columns).

## Recent Progress

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
