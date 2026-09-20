## Goal

Fill `COLUMN_DEFAULT`, `EXTRA`, and `COLUMN_KEY` on `information_schema.COLUMNS` for ORM migrators.

## Category

Phase T — Schema completeness (M156). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M12/M27 COLUMNS exists with a subset. ORMs read EXTRA (`auto_increment`), COLUMN_KEY (`PRI`/`UNI`/`MUL`), COLUMN_DEFAULT. Do not invent GENERATED extra unless M146 is on main.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] PK INT AUTO_INCREMENT: `COLUMN_KEY=PRI`, `EXTRA` contains `auto_increment`, `COLUMN_DEFAULT` empty or documented
- [ ] Non-key VARCHAR with DEFAULT 'x': COLUMN_DEFAULT comparable to MySQL (`x` or `'x'` — pin tests)
- [ ] Unknown schema still not errno 1146 for COLUMNS (existing behavior)
- [ ] Unit/wire tests; `mysql-diff` if column order already matches; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/info_schema.rs`
- `crates/rusql-core/src/**` (read-only meta)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- New information_schema tables (INNODB_* is M194)
- Changing COLUMNS column count incompatibly without tests
- WAL writes

## Negative Constraints

- Do not fake `GENERATED ALWAYS AS` in EXTRA unless generated columns exist
- Do not add STATISTICS table in this slice

## Test plan

```bash
cargo test -p rusql-executor information_schema_columns
cargo test -p rusql-server columns_extra
```
