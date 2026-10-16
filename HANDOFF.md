# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `issue-317-m165-procedure-out-inout` (PR pending — M165 #317) |
| Next step | **M166 SIGNAL / RESIGNAL** ([#318](https://github.com/tanbamboo/rusql/issues/318)) — label `agent-ready` after M165 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases V–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M164 merge (#415) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **M165 #317** in PR; M166+ not started |

| Estimated surface | ~45–70% client-visible; remaining work is Phase V through Z (including M209/M210) |

## Recent Progress

- **M165 in flight** — `OUT`/`INOUT` on `CREATE PROCEDURE`; `CALL p(@var)` assigns user variables; errno 1414 for non-variable OUT/INOUT args; `SHOW CREATE PROCEDURE` / `PARAMETERS`; mysql-diff `procedure_out_inout` (#317)
- **M164 merged** — `GET_LOCK` blocking wait (`timeout>0`), `IS_FREE_LOCK` / `IS_USED_LOCK`; Condvar registry in `rusql-core`; tests `get_lock_timeout`; mysql-diff `get_lock` (#316 / PR #415)
- **M163 merged** — `LOCK TABLES` / `UNLOCK TABLES`; READ/WRITE table locks; implicit commit; errno 1146/1100/1099/1205 (i18n); two-connection wait tests; mysql-diff `lock_tables` (#315 / PR #414)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M165: `SET @var = expr` in procedure bodies evaluates expressions via `eval_expr` (needed for `SET x = x + 1` after INOUT substitution).
- M164: User-lock waits block inside `execute()` (long `GET_LOCK` occupies a Tokio worker; `spawn_blocking` not applied to arbitrary SELECT yet).
- Phase V next after merge: M166 (#318) SIGNAL / RESIGNAL.
