# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-26 |
| Branch | `issue-330-m178-semisync` — [PR #431](https://github.com/tanbamboo/rusql/pull/431) |
| Next step | Merge **M178** ([#330](https://github.com/tanbamboo/rusql/issues/330) / [PR #431](https://github.com/tanbamboo/rusql/pull/431)) when CI is green, then label **M179** ([#331](https://github.com/tanbamboo/rusql/issues/331)) `agent-ready` if it is not already. Do not steal M176 ([#328](https://github.com/tanbamboo/rusql/pull/429), not on `main`). Do not mass-label later phases. Ultimate MySQL 8.0 goal is **not** complete (Phases W–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-26)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M175 (PR #428). M176 PR #429 is open, CI green, **not merged** |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–V | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |
| Phase W M177 | **Complete** on `main` via PR #430 (manual `read_only` promote). Not a SQL thread |
| Phase W M178 | In review — [PR #431](https://github.com/tanbamboo/rusql/pull/431): semi-sync dump or ACK is errno **1524**; no ACK wait; durability stays async WAL |
| Phase W M176 / M179–M180 | Open. M179 has a local unpushed branch `issue-331-m179-change-master` — do not overwrite it |

| Estimated surface | ~45–70% client-visible; remaining work is Phase W through Z (including M209/M210) |

## Recent Progress

- **M178** — Semi-sync handshake is errno 1524 (`rpl_semi_sync_master` not loaded). No ACK wait. Async WAL only (#330 / PR #431)
- **M177 merged** — Manual promote: `SET GLOBAL super_read_only` / `read_only`, persisted in `replica-role.json`. Errno 1290 / 1229 / 1231. No automatic failover and no cross-directory fencing (#329 / PR #430)
- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (#327 / PR #428)
- **M174 merged** — Idle dump follow sends `HEARTBEAT_LOG_EVENT` (27) (#326 / PR #427)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p rusql-protocol semisync
cargo test -p rusql-server semisync
cargo test -p rusql-storage replica_promote
cargo test -p rusql-executor global_read_only_assignment
cargo test -p rusql-server replica_promote
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
```

## Notes

- M177 does **not** implement `START SLAVE` / SQL thread. That is M179 ([#331](https://github.com/tanbamboo/rusql/issues/331)), already `agent-ready`, with local work on `issue-331-m179-change-master`.
- M176 ([#328](https://github.com/tanbamboo/rusql/pull/429)) is not on `main`. Do not treat `@@gtid_executed` as shipped.
- Local branch `issue-342-m190-show-status-live` has an unpushed M190 commit. Leave it alone.
- Overall MySQL 8.0 goal is **not** complete.
