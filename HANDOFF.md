# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `feat/m166-signal-resignal` (PR pending for #318) |
| Next step | **M167 DECLARE variables** ([#319](https://github.com/tanbamboo/rusql/issues/319)) after M166 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M165 merge (#416) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **M165** on `main`; **M166 #318** in PR |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M166 (in flight)** — `SIGNAL SQLSTATE` in procedure bodies → ERR with custom SQLSTATE/errno/message; default errno 1644; `RESIGNAL` outside handler errno 1645; mysql-diff `procedure_signal` (#318)
- **M165 merged** — `OUT`/`INOUT` on `CREATE PROCEDURE`; `CALL p(@var)` assigns user variables; errno 1414 for non-variable OUT/INOUT args; `SHOW CREATE PROCEDURE` / `PARAMETERS`; mysql-diff `procedure_out_inout` (#317 / PR #416)
- **M164 merged** — `GET_LOCK` blocking wait (`timeout>0`), `IS_FREE_LOCK` / `IS_USED_LOCK`; Condvar registry in `rusql-core`; tests `get_lock_timeout`; mysql-diff `get_lock` (#316 / PR #415)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M166: `ExecError::SpSignal` carries SQLSTATE for ERR packets; existing errors still use `HY000`.
- M165: `SET @var = expr` in procedure bodies evaluates expressions via `eval_expr` (needed for `SET x = x + 1` after INOUT substitution).
- Phase V next after M166: M167 (#319) DECLARE variables in BEGIN…END.
