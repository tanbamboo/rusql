# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-26 |
| Branch | `main` after M179 merge — [PR #432](https://github.com/tanbamboo/rusql/pull/432) |
| Next step | **M180** row event v2 / partial images ([#332](https://github.com/tanbamboo/rusql/issues/332)) is `agent-ready`. Do not steal M176 ([#328](https://github.com/tanbamboo/rusql/pull/429), not on `main`). Do not mass-label later phases. Ultimate MySQL 8.0 goal is **not** complete (Phases W–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-26)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M179 (PR #432). M176 PR #429 is open, **not merged** |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–V | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |
| Phase W M177 | **Complete** on `main` via PR #430 (manual `read_only` promote). Not a SQL thread |
| Phase W M178 | **Complete** on `main` via PR #431. Semi-sync dump or ACK is errno **1524**. No ACK wait. Durability stays async WAL |
| Phase W M179 | **Complete** on `main` via PR #432. `CHANGE MASTER TO` / `START SLAVE` connects, dumps, and applies. Errnos 1200 / 1198 / 1231 / 2003 |
| Phase W M176 / M180 | M176 open (do not steal #328). M180 ([#332](https://github.com/tanbamboo/rusql/issues/332)) is `agent-ready` |

| Estimated surface | ~45–70% client-visible; remaining work is Phase W through Z (including M209/M210) |

## Recent Progress

- **M179 merged** — `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` (and `START REPLICA` synonyms). Replica connects, dumps, and applies row events. `SHOW SLAVE STATUS` IO/SQL flags. Errnos 1200, 1198, 1231, 2003 (#331 / PR #432)
- **M178 merged** — Semi-sync dump request or ACK is errno 1524 (`rpl_semi_sync_master` not loaded). No ACK wait. Durability stays async WAL (#330 / PR #431)
- **M177 merged** — Manual promote: `SET GLOBAL super_read_only` / `read_only`, persisted in `replica-role.json`. Errno 1290 / 1229 / 1231. No automatic failover and no cross-directory fencing (#329 / PR #430)
- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (#327 / PR #428)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p rusql-sql replica_control
cargo test -p rusql-storage start_slave
cargo test -p rusql-executor change_master
cargo test -p rusql-server change_master_start_stop_slave_status
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
```

## Notes

- M179 does **not** implement `MASTER_DELAY`, GTID auto-position, or SSL replica options (ignored). Passwords live in `{data_dir}/replica.json` and are not logged.
- M176 ([#328](https://github.com/tanbamboo/rusql/pull/429)) is not on `main`. Do not treat `@@gtid_executed` as shipped.
- Local branch `issue-342-m190-show-status-live` has an unpushed M190 commit. Leave it alone.
- Overall MySQL 8.0 goal is **not** complete.
