# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-26 |
| Branch | `main` after M180 merge — [PR #433](https://github.com/tanbamboo/rusql/pull/433) |
| Next step | Do not steal M176 ([#328](https://github.com/tanbamboo/rusql/issues/328) / [PR #429](https://github.com/tanbamboo/rusql/pull/429), not on `main`). Do not mass-label later phases. Phase W has no issue after M180 to mark `agent-ready`. Ultimate MySQL 8.0 goal is **not** complete (Phases W–Z including M209/M210 remain; M176 is still open). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-26)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M180 (PR #433) once that PR merges. M176 PR #429 is open, **not merged** |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–V | **Complete** on `main` |
| Phase W M173–M175 | **Complete** on `main` |
| Phase W M177 | **Complete** on `main` via PR #430 (manual `read_only` promote). Not a SQL thread |
| Phase W M178 | **Complete** on `main` via PR #431. Semi-sync dump or ACK is errno **1524**. No ACK wait. Durability stays async WAL |
| Phase W M179 | **Complete** on `main` via PR #432. `CHANGE MASTER TO` / `START SLAVE` connects, dumps, and applies. Errnos 1200 / 1198 / 1231 / 2003 |
| Phase W M180 | **Complete** via PR #433. Row events v2 (types 30/31/32). `binlog_row_image` FULL (default) / MINIMAL / NOBLOB. Partial UPDATE does not rewrite omitted columns. Errnos 1231 / 1610 |
| Phase W M176 | Open (do not steal #328) |

| Estimated surface | ~45–70% client-visible; remaining work is Phase W through Z (including M209/M210) |

## Recent Progress

- **M180 merged** — `WRITE_ROWS` / `UPDATE_ROWS` / `DELETE_ROWS` v2 with extra-data length. `binlog_row_image` MINIMAL/NOBLOB/FULL. INSERT still logs the full row. Invalid image errno **1231**. Corrupt v2 header errno **1610** (#332 / PR #433)
- **M179 merged** — `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` (and `START REPLICA` synonyms). Replica connects, dumps, and applies row events. `SHOW SLAVE STATUS` IO/SQL flags. Errnos 1200, 1198, 1231, 2003 (#331 / PR #432)
- **M178 merged** — Semi-sync dump request or ACK is errno 1524 (`rpl_semi_sync_master` not loaded). No ACK wait. Durability stays async WAL (#330 / PR #431)
- **M177 merged** — Manual promote: `SET GLOBAL super_read_only` / `read_only`, persisted in `replica-role.json`. Errno 1290 / 1229 / 1231. No automatic failover and no cross-directory fencing (#329 / PR #430)
- **M175 merged** — Live `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (#327 / PR #428)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p rusql-storage row_event_v2
cargo test -p rusql-storage update_rows_partial
cargo test -p rusql-protocol row_event_v2
cargo test -p rusql-server row_event_v2_partial
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
```

## Notes

- M180 default `binlog_row_image` is **FULL**. `MINIMAL` before-image is the primary key (or every column when there is no key); the after-image is changed columns only. `NOBLOB` drops unchanged BLOB/TEXT columns. `INSERT` logs every column in every mode. Value persists in `{data_dir}/binlog-row-image.json`. `SET` and `SET GLOBAL` share that process-wide value. `SELECT @@binlog_row_image` reads the session copy. `SHOW VARIABLES` does not list the name. `@@GLOBAL.binlog_row_image` is not a separate read. Not `binlog_row_value_options`, not statement-based replication, not compressed row events. Cells stay rusql length-prefixed UTF-8 (not a mysqlbinlog byte oracle). v1 types 23–25 still apply.
- M179 does **not** implement `MASTER_DELAY`, GTID auto-position, or SSL replica options (ignored). Passwords live in `{data_dir}/replica.json` and are not logged.
- M176 ([#328](https://github.com/tanbamboo/rusql/pull/429)) is not on `main`. Do not treat `@@gtid_executed` as shipped. Do not label it `agent-ready` from this session.
- Phase W ends at M180. Do not label Phase X (#333 and later).
- Overall MySQL 8.0 goal is **not** complete.
