# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-16 |
| Branch | `issue-306-m154-create-table-like-ctas` (PR pending merge for M154) |
| Next step | **M155 RENAME TABLE multi-pair** ([#307](https://github.com/tanbamboo/rusql/issues/307)) after M154 merge. Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M153 merge; M154 PR pending |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` |
| Phases T–Z (M146–M210) | **In progress** — M146–M153 on `main`; M154 in PR ([#306](https://github.com/tanbamboo/rusql/issues/306)); next sequenced after merge: [M155 #307](https://github.com/tanbamboo/rusql/issues/307). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M154 (in PR)** — CREATE TABLE LIKE / AS SELECT; mysql-diff `create_table_like_ctas` (#306)
- **M153 merged** — ALTER TABLE ADD/DROP INDEX dump forms; ADD UNIQUE enforcement; WAL `drop_index`; mysql-diff `alter_table_index` (#305 / PR #403)
- **M152 merged** — RANGE table partitioning; mysql-diff `partition_range` (#304 / PR #402)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
```

## Notes

- M154: LIKE skips FK/partition/secondary indexes; CTAS infers INT/DOUBLE/VARCHAR from result values; `IF NOT EXISTS` no-op when target exists.
- M153: sqlparser 0.53 rewrites `DROP INDEX … ON` / `ALTER … DROP INDEX` to `DROP CONSTRAINT` in `rusql-sql`.
- Phase T exit (mysqldump simple schema) still open after M155+ slices.
