## Goal

Accept `CONVERT(s USING utf8mb4)` and charset `CAST` so charset conversion probes are not unsupported.

## Category

Phase S — JSON, set SQL, and remaining query forms (M139). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M46 `CAST` exists for types. `CONVERT(expr USING charset)` is a client/ORM probe. Only utf8mb4 (and utf8 synonym if already aliased) for this slice; identity conversion is enough if bytes are already utf8mb4.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT CONVERT('abc' USING utf8mb4)` returns `abc` (comparable to MySQL)
- [ ] Unknown charset → errno 1115 (i18n), same family as M114 unknown charset
- [ ] Existing `CAST(x AS CHAR)` / numeric CAST unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/expr.rs`
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/collation.rs` (lookup only; no new collations)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- New character sets beyond utf8mb4
- CREATE DATABASE charset catalog (M114 implementation)
- String transcoding from latin1 unless tests pin a documented subset

## Negative Constraints

- Do not claim `CONVERT(x, DATETIME)` style CONVERT(expr, type) unless already parsed — this issue is USING charset
- Do not change SET NAMES (M82)

## Test plan

```bash
cargo test -p rusql-executor convert_charset
cargo test -p rusql-server convert_charset
```
