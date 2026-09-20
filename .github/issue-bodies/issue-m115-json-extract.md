## Goal

Evaluate `JSON_EXTRACT(json, path)` for the probe path `$.a` so JSON columns in apps are not `unsupported function`.

## Background

Phase R after M40 JSON storage and M113 builtins. Probe: `fn_json_extract` → unsupported `JSON_EXTRACT`. Leave `JSON_SET` / `->` / `->>` for M133/M134.

## Acceptance Criteria

- [ ] `SELECT JSON_EXTRACT('{"a":1}', '$.a')` returns a cell comparable to MySQL (`1` or `"1"` — pin the actual MySQL text in tests)
- [ ] Missing path returns SQL NULL (empty cell), not an error
- [ ] Invalid JSON is an error with an i18n message (document errno if mapped)
- [ ] Unknown other functions still unsupported. M113 builtins unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`expr.rs` and tests)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- New crates / serde_json as a new workspace dep unless already present
- `JSON_OBJECT` / `JSON_SET` / `->` operators
- WAL format changes

## Negative Constraints

- Do not implement the full JSONPath spec; `$.key` and optional `$.a.b` nested objects are enough if tests pin them
- Do not claim `JSON_TABLE` / `JSON_SEARCH`

## Test plan

```bash
cargo test -p rusql-executor json_extract
cargo test -p rusql-server json_extract
```
