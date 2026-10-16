# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `issue-309-m157-select-for-update-wait` (PR pending merge for #309) |
| Next step | **M158 READ COMMITTED vs snapshot** ([#310](https://github.com/tanbamboo/rusql/issues/310)) after M157 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M156 merge (await M157 PR) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **In progress** — M157 #309 PR open; M158 #310 next |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M157 (in PR #309)** — `SELECT … FOR UPDATE` exclusive row locks (single-column PK); conflicting `UPDATE` waits; errno 1205 + `innodb_lock_wait_timeout`; storage `RowLockRegistry`; server two-connection tests `for_update_wait`
- **M156 merged** — `information_schema.COLUMNS` extras (#308 / PR #406)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M157: Row locks release on `COMMIT`/`ROLLBACK`/disconnect/reset; autocommit statements release at statement end. `FOR SHARE` / gap locks / SKIP LOCKED errno 3572 remain future slices. Tables without single-column PK skip row locking.
- Phase T exit (mysqldump simple schema) still open after M156 slice.
