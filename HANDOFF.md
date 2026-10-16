# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M161 merge — PR for #313) |
| Next step | **M162 XA transactions** ([#314](https://github.com/tanbamboo/rusql/issues/314)) (`agent-ready` after M161 merge). Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M161 merge (#313) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **In progress** — M157–M161 merged; **M162 #314** next |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M161 merged** — Gap / next-key locks (ADR); RR/SER `FOR UPDATE` PK range gap lock; INSERT phantom block; tests `gap_lock` (#313)
- **M160 merged** — Row-lock wait-for cycle detection; errno 1213 (i18n); victim = max connection id in cycle; txn rollback on server; tests `deadlock` (#312 / PR #411)
- **M159 merged** — `SERIALIZABLE` = RR snapshot + implicit `FOR UPDATE` on plain base-table `SELECT` (single-column PK); tests `serializable` (#311 / PR #410)
- **M158 merged** — `READ COMMITTED` per-statement refresh vs RR snapshot; `session_tx_isolation`; tests `read_committed` (#310 / PR #409)
- **M157 merged** — `SELECT … FOR UPDATE` waits (#309 / PR #407)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M161: MVP gap locks on single-column PK range scans (`BETWEEN` / bounded `AND`); literal `INSERT … VALUES` waits via server `spawn_blocking` (avoids engine read lock during wait). Not full InnoDB lock matrix, `FOR SHARE` gaps, RC gap locks, or INSERT-from-SELECT gap wait.
- M160: Not InnoDB victim weights or `SHOW ENGINE INNODB STATUS`. Connections stay open on 1213.
- M159: JOIN/derived `SELECT` does not implicit-lock. Index-only read paths may bypass MVCC snapshot (pre-existing).
- Phase T exit (mysqldump simple schema) still open after M156 slice.
