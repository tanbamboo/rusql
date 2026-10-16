# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M162 merge — PR #413, `398d9d0`) |
| Next step | **M163 LOCK TABLES** ([#315](https://github.com/tanbamboo/rusql/issues/315)) (`agent-ready` after M162 merge). Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M162 merge (#413) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **In progress** — M157–M162 merged; **M163 #315** next |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M162 merged** — XA START/END/PREPARE/COMMIT/ROLLBACK/RECOVER; WAL-persisted prepare; restart + `XA RECOVER`; errno 1399; tests `xa` (#314 / PR #413)
- **M161 merged** — Gap / next-key locks (ADR); RR/SER `FOR UPDATE` PK range gap lock; INSERT phantom block; tests `gap_lock` (#313)
- **M160 merged** — Row-lock wait-for cycle detection; errno 1213 (i18n); victim = max connection id in cycle; txn rollback on server; tests `deadlock` (#312 / PR #411)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M162: Single quoted xid only; additive WAL `xa_prepare`/`xa_commit`/`xa_rollback`; not JOIN/SUSPEND/ONE PHASE.
- M161: MVP gap locks on single-column PK range scans; not full InnoDB lock matrix.
- Phase T exit (mysqldump simple schema) still open after M156 slice.
