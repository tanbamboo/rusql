# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-21 |
| Branch | `issue-331-m179-change-master` (M179 ready to commit / PR) |
| Next step | **Ship M179** ([#331](https://github.com/tanbamboo/rusql/issues/331)) — open PR / merge when CI green. Parallel: ship M176 ([#328](https://github.com/tanbamboo/rusql/issues/328)) if still open. Then label M180 `agent-ready`; Phase X needs human ADR. Ultimate goal **not** complete until Phase Z M209/M210. |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-21)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M175 merge |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–V | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |
| Phase W M176 | In flight / ready for PR on separate branch |
| Phase W M179 | **Implemented** on `issue-331-m179-change-master` (this session) |
| Phase W M177–M178, M180 | Open (M177 `needs-human`) |
| Phase X–Z | Open; X mostly `needs-human` until ADR |

### Milestone backlog (open issues)

| Phase | Milestone | Notes |
|-------|-----------|-------|
| W | M176 | `@@gtid_executed` — separate branch/PR |
| W | M179 | `CHANGE MASTER` / `START SLAVE` — ship this branch |
| W | M180 | Row event v2 / partial images — next after M179 |
| W | M178 | Semi-sync ACK stub |
| W | M177 | Failover — **needs-human** ADR |

## Recent Progress

- **M179 implemented** — `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` (+ 8.0.23 synonyms), `replica.json` persistence, `SHOW SLAVE/REPLICA STATUS` IO/SQL flags, bad-host error, docs/tests (#331)
- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (#327)
- **M174 merged** — Idle dump follow sends `HEARTBEAT_LOG_EVENT` (27) (#326)
- **M173 merged** — `GTID_LOG_EVENT` (33) before row events (#325)

## Sensors

Run before merge: `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test -- --skip release_binary`, `node scripts/harness-validate.mjs`.
