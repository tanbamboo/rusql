# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M170 merge — PR #423, `ba98e2d`) |
| Next step | **M171 Trigger timings completeness** ([#323](https://github.com/tanbamboo/rusql/issues/323)) (`agent-ready`). Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M170 merge (#423) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **M165–M170** on `main`; **M171 #323** next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M170 merged** — `DECLARE CONTINUE|EXIT HANDLER FOR` NOT FOUND / SQLEXCEPTION / mysql errno; cursor **1329** catchable; EXIT leaves block without aborting handled `CALL`; `RESIGNAL` in handler (#322 / PR #423)
- **M169 merged** — `DECLARE CURSOR FOR` / `OPEN` / `FETCH INTO` / `CLOSE`; one open cursor per `CALL`; mysql-diff `procedure_cursors` (#321 / PR #422)
- **M168 merged** — `IF`/`ELSEIF`/`ELSE`, `WHILE`, labeled `LOOP`/`LEAVE` execute on `CALL` (#320 / PR #421)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M170 gaps (documented): no `UNDO` handlers, no dedicated `SQLWARNING` handlers, no `GET DIAGNOSTICS`, procedure-level handler scope only, simplified precedence (reverse declaration order among NOT FOUND / SQLEXCEPTION / errno / `SQLSTATE`).
- M171 next: six trigger timings BEFORE/AFTER × INSERT/UPDATE/DELETE.
