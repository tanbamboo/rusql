# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-26 |
| Branch | `issue-328-m176-gtid-executed` — [PR #429](https://github.com/tanbamboo/rusql/pull/429) (rebased onto `main` after M180) |
| Next step | Merge M176 when CI is green. Phase W is then complete on `main`. Next sequenced issue is Phase X M181 ([#333](https://github.com/tanbamboo/rusql/issues/333)), which stays `needs-human` until a TLS ADR is accepted. Do not mass-label later phases. Ultimate MySQL 8.0 goal is **not** complete (Phases X–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-26)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M180 (PR #433, `4cf80bd`). M176 PR #429 updated on this branch, **not merged** until CI is green |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–V | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |
| Phase W M177 | **Complete** on `main` via PR #430 (manual `read_only` promote). Not a SQL thread |
| Phase W M178 | **Complete** on `main` via PR #431. Semi-sync dump or ACK is errno **1524**. No ACK wait. Durability stays async WAL |
| Phase W M179 | **Complete** on `main` via PR #432. `CHANGE MASTER TO` / `START SLAVE` connects, dumps, and applies. Errnos 1200 / 1198 / 1231 / 2003 |
| Phase W M180 | **Complete** on `main` via PR #433. Row events v2 (types 30/31/32). `binlog_row_image` FULL (default) / MINIMAL / NOBLOB. Partial UPDATE does not rewrite omitted columns. Errnos 1231 / 1610 |
| Phase W M176 | This PR. `@@gtid_executed` persisted in `gtid.json`. Not on `main` until #429 merges |
| Phase X–Z | Open. Phase X stays `needs-human` until an ADR is accepted |

| Estimated surface | ~45–70% client-visible; remaining work is Phase X through Z (including M209/M210) |

## Recent Progress

- **M176 (this PR)** — `@@gtid_executed` / `Executed_Gtid_Set` from applied GTIDs in `{data_dir}/gtid.json`. Empty string on a fresh server. `COMMIT` records produced GTIDs. `START SLAVE` records applied GTIDs. Restart reloads the set. `@@session.gtid_executed` errno **1238**. SET errno **1238**. Not `gtid_mode` UI or Group Replication (#328 / PR #429)
- **M180 merged** — `WRITE_ROWS` / `UPDATE_ROWS` / `DELETE_ROWS` v2 with extra-data length. `binlog_row_image` MINIMAL/NOBLOB/FULL. INSERT still logs the full row. Invalid image errno **1231**. Corrupt v2 header errno **1610** (#332 / PR #433)
- **M179 merged** — `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` (and `START REPLICA` synonyms). Replica connects, dumps, and applies row events. `SHOW SLAVE STATUS` IO/SQL flags. Errnos 1200, 1198, 1231, 2003 (#331 / PR #432)
- **M178 merged** — Semi-sync dump request or ACK is errno 1524 (`rpl_semi_sync_master` not loaded). No ACK wait. Durability stays async WAL (#330 / PR #431)
- **M177 merged** — Manual promote: `SET GLOBAL super_read_only` / `read_only`, persisted in `replica-role.json`. Errno 1290 / 1229 / 1231. No automatic failover and no cross-directory fencing (#329 / PR #430)
- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (#327 / PR #428)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p rusql-storage gtid_executed
cargo test -p rusql-executor gtid_executed
cargo test -p rusql-server gtid_executed
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
```

## Notes

- M176 format is MySQL `uuid:intervals` (comma-separated groups, `:` between intervals). Bare `@@gtid_executed` and `@@global.gtid_executed` read the persisted set. `@@session.` / `@@local.` are errno **1238**. `SHOW VARIABLES` lists the name. `Retrieved_Gtid_Set` stays empty. Not `gtid_mode` switching, Group Replication, or `mysql.gtid_executed`.
- M180 default `binlog_row_image` is **FULL**. `MINIMAL` before-image is the primary key (or every column when there is no key); the after-image is changed columns only. `NOBLOB` drops unchanged BLOB/TEXT columns. `INSERT` logs every column in every mode. Value persists in `{data_dir}/binlog-row-image.json`.
- M179 does **not** implement `MASTER_DELAY`, GTID auto-position, or SSL replica options (ignored). Passwords live in `{data_dir}/replica.json` and are not logged.
- Do not mass-label Phase X–Z. M181 ([#333](https://github.com/tanbamboo/rusql/issues/333)) is the next sequenced issue and stays `needs-human` until a TLS ADR is accepted.
- Overall MySQL 8.0 goal is **not** complete.
