# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `issue-315-m163-lock-tables` (PR pending — M163 #315) |
| Next step | **M164 GET_LOCK wait** ([#316](https://github.com/tanbamboo/rusql/issues/316)) after M163 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

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
| Phase U (M157–M164) | **In progress** — M157–M162 on `main`; **M163 #315** in PR |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M163 (in PR)** — `LOCK TABLES` / `UNLOCK TABLES`; READ/WRITE metadata locks; implicit commit; errno 1146/1100/1099/1205 (i18n); two-connection wait tests; mysql-diff `lock_tables` (#315)
- **M162 merged** — XA START/END/PREPARE/COMMIT/ROLLBACK/RECOVER; WAL-persisted prepare; restart + `XA RECOVER`; errno 1399; tests `xa` (#314 / PR #413)
- **M161 merged** — Gap / next-key locks (ADR); RR/SER `FOR UPDATE` PK range gap lock; INSERT phantom block; tests `gap_lock` (#313)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M163: Table lock waits run on `spawn_blocking` (avoid Tokio deadlock with row-lock waits). Not `READ LOCAL`, global read lock, or views/temp tables.
- M162: Single quoted xid only; additive WAL `xa_prepare`/`xa_commit`/`xa_rollback`; not JOIN/SUSPEND/ONE PHASE.
- Phase T exit (mysqldump simple schema) partially addressed by M163 table locks; full dump parity still open.
