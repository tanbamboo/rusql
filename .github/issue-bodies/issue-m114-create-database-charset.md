## Goal

Accept `CREATE DATABASE … CHARACTER SET … COLLATE …` and persist per-schema charset/collation so `SHOW CREATE DATABASE` and `information_schema.SCHEMATA` reflect the requested values instead of constants.

## Category

Phase R — Post-Q high-ROI client SQL. See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Probe (`scripts/mysql-gap-probe.mjs`): `create_database_charset` → parse/exec error. sqlparser 0.53 `CreateDatabase` has only `db_name` / `if_not_exists` / `location` / `managed_location` (no MySQL charset). M36 creates databases by name only. M91 `SHOW CREATE DATABASE` emits stub `utf8mb4` / `utf8mb4_unicode_ci`. `information_schema.SCHEMATA` already has `DEFAULT_CHARACTER_SET_NAME` / `DEFAULT_COLLATION_NAME` as constants.

## Acceptance Criteria

- [ ] `CREATE DATABASE gap_cs CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci` succeeds (also `CHARSET` synonym and optional `DEFAULT` keywords)
- [ ] `SHOW CREATE DATABASE gap_cs` includes the stored charset and collation (not a hardcoded stub for this database)
- [ ] `information_schema.SCHEMATA` for that schema reports the same charset/collation
- [ ] Omitted charset/collation keeps rusql documented defaults (`utf8mb4` / `utf8mb4_unicode_ci`)
- [ ] Supported collations are the existing catalog set (`utf8mb4_unicode_ci`, `utf8mb4_0900_ai_ci`). Unknown charset → errno 1115; unknown collation → errno 1273 (i18n messages)
- [ ] Existing `CREATE DATABASE name` without clauses still works. M91 column names unchanged. Restart/WAL replay keeps charset (additive `serde(default)` only)
- [ ] Unit/wire tests; `mysql-diff` suite `create_database_charset` (`SHOW CREATE DATABASE` may compare); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql report

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**` (rewrite/parse charset clauses)
- `crates/rusql-core/src/**` (optional `DatabaseMeta`; collation helpers only)
- `crates/rusql-storage/src/**` (catalog + additive WAL fields on `CreateDatabase`)
- `crates/rusql-executor/src/**` (CREATE DATABASE, SHOW CREATE DATABASE, SCHEMATA)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-r-issues.mjs`

Forbidden:
- Renaming/removing existing WAL fields (additive `serde(default)` only)
- `JSON_EXTRACT` / `UUID` / `GET_LOCK` / `LAST_INSERT_ID(expr)`
- New character sets beyond utf8mb4
- New crates
- Auth/TLS changes

## Negative Constraints

- Do not claim every MySQL charset/collation
- Do not change table-column collation semantics (M59/M62)
- Do not implement `ALTER DATABASE … CHARACTER SET` in this issue

## Test plan

```bash
cargo test -p rusql-sql create_database
cargo test -p rusql-storage create_database
cargo test -p rusql-executor create_database
cargo test -p rusql-server create_database
```
