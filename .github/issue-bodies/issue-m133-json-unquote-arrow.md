## Goal

Accept `JSON_UNQUOTE` and the `->` / `->>` operators so ORM JSON column access matches portable MySQL after M115 `JSON_EXTRACT`.

## Category

Phase S — JSON, set SQL, and remaining query forms (M133). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Phase S after M115 (`JSON_EXTRACT($.key)`). MySQL `col->'$.a'` is `JSON_EXTRACT`; `col->>'$.a'` is `JSON_UNQUOTE(JSON_EXTRACT(...))`. Leave `JSON_OBJECT` / `JSON_SET` for M134.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT JSON_UNQUOTE(JSON_EXTRACT('{"a":"x"}', '$.a'))` matches Docker MySQL text (unquoted `x`)
- [ ] `SELECT col->'$.a'` and `SELECT col->>'$.a'` on a JSON/text column match `JSON_EXTRACT` / `JSON_UNQUOTE(JSON_EXTRACT)` for the same path
- [ ] Missing path still SQL NULL. Invalid JSON still errors (same class as M115)
- [ ] M115 `JSON_EXTRACT` behavior unchanged. `JSON_SET` / `JSON_OBJECT` stay later
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**` (`->` / `->>` rewrite if sqlparser needs it)
- `crates/rusql-executor/src/expr.rs` and tests
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- `JSON_OBJECT` / `JSON_ARRAY` / `JSON_SET` (M134)
- `JSON_TABLE` / `JSON_SEARCH`
- CREATE DATABASE charset (M114) / WAL format changes
- New crates

## Negative Constraints

- Do not implement the full JSONPath spec; pin `$.key` and optional nested `$.a.b`
- Do not change M40 JSON storage format

## Test plan

```bash
cargo test -p rusql-executor json_unquote
cargo test -p rusql-server json_unquote
```
