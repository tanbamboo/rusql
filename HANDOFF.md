# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `issue-310-m158-read-committed` (M158 PR for #310) |
| Next step | Merge M158; label **M159 SERIALIZABLE** ([#311](https://github.com/tanbamboo/rusql/issues/311)) `agent-ready`. Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M157 merge (#407) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **In progress** — M157 merged (#407); M158 #310 in PR; M159 #311 next |
| Phases V–Z (M165–M210) | Not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M158 (in PR #310)** — `READ COMMITTED` per-statement refresh vs RR snapshot; `session_tx_isolation`; tests `read_committed`
- **M157 merged** — `SELECT … FOR UPDATE` waits (#309 / PR #407)
- **M156 merged** — `information_schema.COLUMNS` extras (#308 / PR #406)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M158: Isolation from M84 applies at `BEGIN`; RC clears pinned base snapshot at each statement boundary. RU/SERIALIZABLE still map to RR snapshot until M159. Index-only read paths may bypass snapshot (pre-existing); acceptance tests use table scans.
- M157: Row locks release on `COMMIT`/`ROLLBACK`/disconnect/reset; autocommit statements release at statement end. `FOR SHARE` / gap locks / SKIP LOCKED errno 3572 remain future slices. Tables without single-column PK skip row locking.
- Phase T exit (mysqldump simple schema) still open after M156 slice.
