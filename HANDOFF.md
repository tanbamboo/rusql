# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `issue-312-m160-deadlock` (PR pending for M160 #312) |
| Next step | After M160 merge: **M161 Gap / next-key locks** ([#313](https://github.com/tanbamboo/rusql/issues/313)). Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M159 merge (#410) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **In progress** — M157–M159 on `main`; **M160 #312** in PR |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M160 (in flight)** — Row-lock deadlock detection on `RowLockRegistry`; errno 1213 (i18n); victim = max connection id in cycle; txn rollback on server (#312)
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

- M160: Not gap-lock deadlocks (M161), `SHOW ENGINE INNODB STATUS`, or InnoDB victim weights. Connections stay open on 1213.
- M159: Documented MVP — not InnoDB next-key/gap locks (M161). JOIN/derived `SELECT` does not implicit-lock. Index-only read paths may bypass MVCC snapshot (pre-existing); phantom acceptance tests use heap scans (`WHERE` on non-indexed columns).
- M158: Isolation from M84 applies at `BEGIN`; RC clears pinned base snapshot at each statement boundary.
- M157: Row locks release on `COMMIT`/`ROLLBACK`/disconnect/reset; autocommit statements release at statement end. `FOR SHARE` / gap locks / SKIP LOCKED errno 3572 remain future slices.
- Phase T exit (mysqldump simple schema) still open after M156 slice.
