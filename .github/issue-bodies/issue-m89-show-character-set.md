## Goal

Implement `SHOW CHARACTER SET` / `SHOW CHARSET` so clients and GUIs that probe the charset list get MySQL-shaped rows instead of an unsupported-statement error.

## Background

Phase Q after M88. sqlparser 0.53 has no `Statement::ShowCharset`; `SHOW CHARACTER SET` typically lands as `ShowVariable` or a parse rewrite (same pattern as `SHOW ENGINES` / `SHOW TABLE STATUS`). phpMyAdmin, the official CLI, and several GUIs send `SHOW CHARACTER SET` or `SHOW CHARSET` (often with `LIKE`) after engine/table-status probes. This slice is a documented stub catalog aligned with rusql's utf8mb4 collations (M59/M62). `SHOW ENGINE INNODB STATUS` and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CHARACTER SET` returns MySQL-shaped columns `Charset`, `Description`, `Default collation`, `Maxlen`
- [ ] The documented stub set includes at least `utf8mb4` with `Maxlen` = `4` and `Default collation` matching rusql's documented utf8mb4 default (`utf8mb4_unicode_ci`, same as M87 `SHOW TABLE STATUS` `Collation`)
- [ ] Other documented stub charsets (if any) are constants, not a live MySQL charset plugin list
- [ ] `SHOW CHARSET` is equivalent to `SHOW CHARACTER SET` for this slice
- [ ] `SHOW CHARACTER SET LIKE 'utf8%'` (MySQL `LIKE` on `Charset`) filters that set; a non-matching pattern returns zero rows, not an error
- [ ] `SHOW COLLATION` from M59 and `SHOW ENGINES` from M88 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (charset list/descriptions differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CHARACTER SET` / `SHOW CHARSET` `[LIKE …]` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Wire charset conversion beyond existing utf8mb4
- Full MySQL 8.0 charset catalog
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CHARACTER SET` parity; document the stub subset
- Do not implement latin1 (or other) wire encoding; this is a metadata stub
- Do not change `SET CHARACTER SET` / `SET CHARSET` behavior from M83
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)

## Test plan

```bash
cargo test -p rusql-sql character_set
cargo test -p rusql-executor character_set
cargo test -p rusql-server character_set
```
