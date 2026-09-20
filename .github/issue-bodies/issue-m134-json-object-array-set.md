## Goal

Evaluate `JSON_OBJECT`, `JSON_ARRAY`, and `JSON_SET` so apps can construct and patch JSON values.

## Category

Phase S — JSON, set SQL, and remaining query forms (M134). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Depends on M115 extract. Probe pack did not list constructors; ORMs and `mysql-test` json* cases need them. Unknown paths for `JSON_SET` follow MySQL (create intermediate objects for `$.a` on `{}`).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT JSON_OBJECT('a', 1, 'b', 'x')` is a JSON object comparable to MySQL
- [ ] `SELECT JSON_ARRAY(1, 'x')` is a JSON array comparable to MySQL
- [ ] `SELECT JSON_SET('{"a":1}', '$.b', 2)` adds `b`; `JSON_SET` on an existing key replaces it
- [ ] Odd-length `JSON_OBJECT` args error (MySQL 1582 / documented). Invalid JSON document errors as M115
- [ ] Unit/wire tests; `mysql-diff`; docs as usual. `->` operators remain M133

## File Boundaries

Allowed:
- `crates/rusql-executor/src/expr.rs` and tests
- `crates/rusql-sql/src/**` if function parse needs help
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- `JSON_TABLE` / `JSON_SEARCH` / `JSON_MERGE_PATCH`
- WAL format / M114 charset
- New crates

## Negative Constraints

- Do not implement `JSON_REPLACE` / `JSON_REMOVE` unless they are one-line aliases of `JSON_SET` with tests
- Do not claim binary JSON type beyond existing M40 text/JSON cells

## Test plan

```bash
cargo test -p rusql-executor json_object
cargo test -p rusql-server json_set
```
