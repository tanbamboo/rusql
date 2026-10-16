# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M172 merge — PR pending) |
| Next step | **M173 GTID event type 33** ([#325](https://github.com/tanbamboo/rusql/issues/325)) — first Phase W issue; label `agent-ready` after M172 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases W–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M171 merge (#424); M172 PR pending |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **Complete** on branch (M172 #324); merge pending |

| Estimated surface | ~45–70% client-visible; remaining work is Phase W through Z (including M209/M210) |

## Recent Progress

- **M172 (branch)** — COM_QUERY `DELIMITER` session splitting; wire tests `delimiter_stub`; mysql-test `delimiter_procedure` (#324)
- **M171 merged** — All six row trigger timings on DML; mysql-diff `trigger_timings` (#323 / PR #424)
- **M170 merged** — Condition handlers; mysql-diff `procedure_handlers` (#322 / PR #423)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M172 gaps: not a mysql client; Docker MySQL wire `DELIMITER` still errors (documented); multi-result COM_QUERY for arbitrary scripts beyond delimiter-driven procedure create unchanged.
- Phase V exit (`mysql-test` sp*/trigger-* first 10 portable) not fully re-verified this session — subset grew with `delimiter_procedure`.
