# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-18 |
| Branch | main |
| Next step | Implement [M102 CREATE EVENT catalog](https://github.com/tanbamboo/rusql/issues/238) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-18)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #237) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M101 | Merged (#162–#237) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `CREATE EVENT` catalog — [M102 #238](https://github.com/tanbamboo/rusql/issues/238)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#237 merged** — M101 `SHOW EVENTS` (#236): MySQL-shaped columns over an empty catalog; unmatched `LIKE` is zero rows; unknown `FROM` db errno 1049; `SHOW CREATE EVENT` / `SHOW CREATE USER` / `SHOW FUNCTION STATUS` unchanged
- **#235 merged** — M100 `SHOW CREATE EVENT` (#234): statement accepted; no event catalog yet so every name is errno 1539; `SHOW CREATE USER` / `SHOW FUNCTION STATUS` / `SHOW PROCEDURE STATUS` unchanged
- **#233 merged** — M99 `SHOW CREATE USER` (#232): MySQL-shaped `CREATE USER for {user}@{host}` reconstructed from M55 accounts (`IDENTIFIED WITH '{plugin}'`, no hash/`BY`/`AS`); unknown accounts errno 3162

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```
