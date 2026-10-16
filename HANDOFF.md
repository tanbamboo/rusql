# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `main` (after M156 merge — PR #406, `d7ed865`) |
| Next step | **M157 `SELECT … FOR UPDATE` waits** ([#309](https://github.com/tanbamboo/rusql/issues/309)) (`agent-ready`, first Phase U slice). Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M156 merge (mysql-diff `information_schema_columns_extra`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phase T (M146–M156) | **Complete** on `main` — Phase T exit (simple mysqldump load) still open |
| Phases U–Z (M157–M210) | **Next** — [M157 #309](https://github.com/tanbamboo/rusql/issues/309) (`agent-ready`); milestones [U](https://github.com/tanbamboo/rusql/milestone/12)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #309–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |

## Recent Progress

- **M156 merged** — `information_schema.COLUMNS` adds `COLUMN_DEFAULT`, `EXTRA`, `COLUMN_KEY`; mysql-diff `information_schema_columns_extra` (#308 / PR #406)
- **M155 merged** — RENAME TABLE multi-pair atomic swap; mysql-diff `rename_table_multi` (#307 / PR #405)
- **M154 merged** — CREATE TABLE LIKE / AS SELECT; mysql-diff `create_table_like_ctas` (#306 / PR #404)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M156: String literal defaults unquoted in `COLUMN_DEFAULT` (MySQL-style). Empty `COLUMN_KEY`/`EXTRA` encode as SQL NULL on wire (rusql empty-string sentinel); non-empty keys/extras match MySQL in mysql-diff. Not full MySQL 8.0 `COLUMNS` column set.
- M155: sqlparser 0.53 lacks standalone `RENAME TABLE`; `rusql-sql` parses pairs + pending rewrite to `ALTER TABLE … RENAME TO`. Storage `rename_tables` batch-validates then applies (swap-safe). errno 1050 via `ExecError::Mysql` for name conflicts.
- Phase T exit (mysqldump simple schema) still open after M156 slice.
