# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `main` (after M155 merge — PR #405, `bdddacd`) |
| Next step | **M156 information_schema.COLUMNS extras** ([#308](https://github.com/tanbamboo/rusql/issues/308)) (`agent-ready`, last Phase T slice). Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M155 merge (mysql-diff `rename_table_multi`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M146–M155 merged; next sequenced: [M156 #308](https://github.com/tanbamboo/rusql/issues/308) (`agent-ready`). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M155 merged** — RENAME TABLE multi-pair atomic swap; mysql-diff `rename_table_multi` (#307 / PR #405)
- **M154 merged** — CREATE TABLE LIKE / AS SELECT; mysql-diff `create_table_like_ctas` (#306 / PR #404)
- **M153 merged** — ALTER TABLE ADD/DROP INDEX dump forms; mysql-diff `alter_table_index` (#305 / PR #403)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M155: sqlparser 0.53 lacks standalone `RENAME TABLE`; `rusql-sql` parses pairs + pending rewrite to `ALTER TABLE … RENAME TO`. Storage `rename_tables` batch-validates then applies (swap-safe). errno 1050 via `ExecError::Mysql` for name conflicts.
- M154: LIKE skips FK/partition/secondary indexes; CTAS infers INT/DOUBLE/VARCHAR; `IF NOT EXISTS` no-op when target exists.
- Phase T exit (mysqldump simple schema) still open after M156 slice.
