# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `feat/m168-procedure-control-flow` (M168 PR pending) |
| Next step | **M169 Cursors** ([#321](https://github.com/tanbamboo/rusql/issues/321)) — label `agent-ready` after M168 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M167 merge (#420) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **M165–M168** implemented on branch; **M169 #321** next after merge |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M168 (branch)** — `IF`/`ELSEIF`/`ELSE`, `WHILE`, labeled `LOOP`/`LEAVE` execute on `CALL`; compound-aware body split; iteration cap 1_000_000; errno 1308 bad label; mysql-diff `procedure_control_flow` (#320)
- **M167 merged** — `DECLARE` locals in procedure bodies (#319 / PR #420)
- **M166 merged** — `SIGNAL SQLSTATE` in procedure/event bodies (#318 / PR #418)
- **M165 merged** — `OUT`/`INOUT` on `CREATE PROCEDURE` (#317 / PR #416)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M168: conditions via `SELECT IF(cond, 1, 0)`; not REPEAT/CASE statement/GOTO/nested BEGIN blocks.
- M169 next: cursors (`DECLARE CURSOR` / `FETCH`).
