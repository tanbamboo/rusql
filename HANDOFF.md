# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-17 |
| Branch | `main` (after M175 merge — PR #428, `6e2ce5c`) |
| Next step | **M176 `@@gtid_executed` persistence** ([#328](https://github.com/tanbamboo/rusql/issues/328)) — `agent-ready`. Ultimate MySQL 8.0 goal is **not** complete (Phases W–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-17)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M175 merge |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phase U (M157–M164) | **Complete** on `main` |
| Phase V (M165–M172) | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |

| Estimated surface | ~45–70% client-visible; remaining work is Phase W through Z (including M209/M210) |

## Recent Progress

- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS`; `File`/`Position` from binlog (#327 / PR #428)
- **M174 merged** — Idle dump follow sends `HEARTBEAT_LOG_EVENT` (27); applier ignores (#326 / PR #427)
- **M173 merged** — `GTID_LOG_EVENT` (33) before row events; `gtid_event` / `gtid_dump` tests (#325 / PR #426)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M175 gaps: `Executed_Gtid_Set` / filter columns empty (M176); `SHOW BINARY LOG STATUS` tested in Rust only (CI MySQL 8.0 image lacks 8.4 synonym).
- M174 gaps: no semi-sync (M178), `COM_BINLOG_DUMP_GTID`, or full replica heartbeat bookkeeping.
- Phase W exit (primary → replica row consistency + GTID failover doc) not complete.
