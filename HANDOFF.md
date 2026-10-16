# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `issue-316-m164-get-lock-timeout` (M164 — PR pending merge) |
| Next step | After M164 merge: **M165** first Phase V issue ([#317](https://github.com/tanbamboo/rusql/issues/317) — verify on GitHub). Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M163 merge (#414) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **M164 implemented** on branch — merge + label #317 `agent-ready` |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M164 (branch)** — `GET_LOCK` blocking wait (`timeout>0`), `IS_FREE_LOCK` / `IS_USED_LOCK`; Condvar registry in `rusql-core`; tests `get_lock_timeout`; mysql-diff `get_lock` (#316)
- **M163 merged** — `LOCK TABLES` / `UNLOCK TABLES`; READ/WRITE table locks; implicit commit; errno 1146/1100/1099/1205 (i18n); two-connection wait tests; mysql-diff `lock_tables` (#315 / PR #414)
- **M162 merged** — XA START/END/PREPARE/COMMIT/ROLLBACK/RECOVER; WAL-persisted prepare; restart + `XA RECOVER`; errno 1399; tests `xa` (#314 / PR #413)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M164: User-lock waits block inside `execute()` (like pre–spawn_blocking row locks on simple SELECT). Long `GET_LOCK` timeouts occupy a Tokio worker; table-lock pattern (`spawn_blocking`) not applied to arbitrary SELECT yet.
- M163: Table lock waits on `spawn_blocking` (avoid Tokio deadlock). Not `READ LOCAL`, global read lock, or views/temp tables.
- Phase U exit after M164 merge; Phase V starts at roadmap M165 (#317).
