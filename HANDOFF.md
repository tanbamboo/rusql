# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M167 merge — PR #420, `3f1f50c`; HANDOFF `868d0d2`) |
| Next step | **M168 IF/WHILE/LOOP/LEAVE** ([#320](https://github.com/tanbamboo/rusql/issues/320)) (`agent-ready`). Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

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
| Phase V (M165–M172) | **M165–M167** on `main`; **M168 #320** next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M167 merged** — `DECLARE` locals in procedure bodies: `DEFAULT`, `SET`, `SELECT … INTO`; bare vs `@` user vars; misplaced `DECLARE` errno 1337; mysql-diff `procedure_declare_var` (#319 / PR #420)
- **M166 merged** — `SIGNAL SQLSTATE` in procedure/event bodies → ERR with SQLSTATE + optional `MYSQL_ERRNO` / `MESSAGE_TEXT`; default errno 1644; `RESIGNAL` outside handler errno 1645; mysql-diff `procedure_signal` (#318 / PR #418)
- **M165 merged** — `OUT`/`INOUT` on `CREATE PROCEDURE`; `CALL p(@var)` assigns user variables; errno 1414 for non-variable OUT/INOUT args; `SHOW CREATE PROCEDURE` / `PARAMETERS`; mysql-diff `procedure_out_inout` (#317 / PR #416)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M167: locals live only for the `CALL`; `programs.json` body still stores full `DECLARE` text; not cursors/handlers (M169/M170).
- M168 next: control flow (`IF` / `WHILE` / `LOOP` / `LEAVE`) in BEGIN…END.
