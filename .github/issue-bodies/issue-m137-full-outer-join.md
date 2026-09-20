## Goal

Evaluate `FULL OUTER JOIN` / `FULL JOIN` with NULL-padding on both unmatched sides.

## Category

Phase S — JSON, set SQL, and remaining query forms (M137). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M41 LEFT/RIGHT OUTER JOIN exist. MySQL 8.0.31+ supports FULL OUTER JOIN. Implement as LEFT UNION unmatched RIGHT (or equivalent) matching Docker MySQL.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Two-table `FULL OUTER JOIN` on a key matches MySQL row set (both unmatched sides present with NULLs)
- [ ] `FULL JOIN` synonym accepted. INNER/LEFT/RIGHT unchanged
- [ ] ON predicate required (same as existing outer join). USING optional only if already supported for LEFT
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**` (join)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Natural join
- Multi-table FULL JOIN of 3+ tables unless tests are cheap
- WAL

## Negative Constraints

- Do not change LEFT JOIN null-padding
- Do not claim hash-join performance (M205)

## Test plan

```bash
cargo test -p rusql-executor full_outer_join
cargo test -p rusql-server full_outer_join
```
