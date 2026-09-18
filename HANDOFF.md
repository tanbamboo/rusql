# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-18 |
| Branch | main |
| Next step | Implement [M96 SHOW CREATE FUNCTION](https://github.com/tanbamboo/rusql/issues/225) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-18)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #224) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M95 | Merged (#162–#224) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SHOW CREATE FUNCTION` stubs — [M96 #225](https://github.com/tanbamboo/rusql/issues/225)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#224 merged** — M95 `SHOW CREATE PROCEDURE` (#223): MySQL-shaped columns from catalog `ProcedureMeta`; reconstructed `CREATE PROCEDURE …() BEGIN … END`; empty params; stub sql_mode/charset; unknown procedure errno 1305; `SHOW CREATE TRIGGER` / `SHOW TRIGGERS` / `SHOW CREATE VIEW` unchanged
- **#222 merged** — M94 `SHOW CREATE TRIGGER` (#221): MySQL-shaped columns `Trigger`/`sql_mode`/`SQL Original Statement`/charset stubs; DDL reconstructed from catalog `TriggerMeta`; unknown trigger errno 1360
- **#220 merged** — M93 `SHOW TRIGGERS` (#219)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```
