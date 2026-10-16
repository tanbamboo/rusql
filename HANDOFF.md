# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M169 merge — PR #422, `b1f274b`) |
| Next step | **M170 Condition HANDLER** ([#322](https://github.com/tanbamboo/rusql/issues/322)) (`agent-ready`). Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M169 merge (#422) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **M165–M169** on `main`; **M170 #322** next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M169 merged** — `DECLARE CURSOR FOR` / `OPEN` / `FETCH INTO` / `CLOSE`; one open cursor per `CALL`; errno 1325/1326/1328/1329; mysql-diff `procedure_cursors` (#321 / PR #422)
- **M168 merged** — `IF`/`ELSEIF`/`ELSE`, `WHILE`, labeled `LOOP`/`LEAVE` execute on `CALL` (#320 / PR #421)
- **M167 merged** — `DECLARE` locals in procedure bodies (#319 / PR #420)
- **M166 merged** — `SIGNAL SQLSTATE` in procedure/event bodies (#318 / PR #418)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M169: cursor rows buffered on `OPEN` for the `CALL`; `FETCH` past end errno **1329** until M170 `NOT FOUND` handlers. Not nested/updatable cursors.
- M170 next: `DECLARE … HANDLER` (CONTINUE/EXIT); enables graceful cursor end-of-data handling.
