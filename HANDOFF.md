# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M173 merge — PR pending for #325) |
| Next step | **M174 Heartbeat events** ([#326](https://github.com/tanbamboo/rusql/issues/326)) — label `agent-ready` after M173 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases W–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M172 merge (#425); M173 PR pending |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **Complete** on `main` |
| Phase W M173 | **In PR** — GTID event type 33 (#325) |

| Estimated surface | ~45–70% client-visible; remaining work is Phase W through Z (including M209/M210) |

## Recent Progress

- **M173 in flight** — `GTID_LOG_EVENT` (33) before row events; `gtid_event` / `gtid_dump` tests (#325)
- **M172 merged** — COM_QUERY `DELIMITER` session splitting; wire tests `delimiter_stub`; mysql-test `delimiter_procedure` (#324 / PR #425)
- **M171 merged** — All six row trigger timings on DML; mysql-diff `trigger_timings` (#323 / PR #424)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M173 gaps: no `@@gtid_executed` (M176), heartbeat (M174), anonymous GTID, checksum bytes, or `COM_BINLOG_DUMP_GTID`.
- Phase W exit (primary → replica row consistency + GTID failover doc) not complete.
- Phase V exit (`mysql-test` sp*/trigger-* first 10 portable) not fully re-verified this session.
