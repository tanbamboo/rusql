# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-18 |
| Branch | main |
| Next step | Implement [M98 SHOW FUNCTION STATUS](https://github.com/tanbamboo/rusql/issues/229) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-18)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #228) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M97 | Merged (#162–#228) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SHOW FUNCTION STATUS` stubs — [M98 #229](https://github.com/tanbamboo/rusql/issues/229)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#228 merged** — M97 `SHOW PROCEDURE STATUS` (#227): MySQL-shaped columns from catalog `ProcedureMeta`; `Type` is `PROCEDURE`; stub Definer/timestamps/charset; unmatched `LIKE` returns zero rows; `SHOW CREATE FUNCTION` / `PROCEDURE` / `TRIGGER` unchanged
- **#226 merged** — M96 `SHOW CREATE FUNCTION` (#225): MySQL-shaped columns from catalog `FunctionMeta`; reconstructed `CREATE FUNCTION …() RETURNS … BEGIN RETURN … END`; empty params; stub sql_mode/charset; unknown function errno 1305
- **#224 merged** — M95 `SHOW CREATE PROCEDURE` (#223)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```
