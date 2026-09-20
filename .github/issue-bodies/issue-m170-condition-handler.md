## Goal

`DECLARE CONTINUE|EXIT HANDLER FOR` SQLEXCEPTION / NOT FOUND / mysql_errno.

## Category

Phase V — Stored programs (M170). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

After M166/M169. CONTINUE resumes; EXIT leaves the BEGIN block. Keep SQLEXCEPTION + NOT FOUND + one errno.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] CONTINUE HANDLER FOR NOT FOUND SET done=1 on FETCH past end; loop terminates via done flag
- [ ] EXIT HANDLER FOR SQLEXCEPTION leaves the block without aborting the whole CALL unless unhandled
- [ ] Unhandled SIGNAL still reaches the client
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/stored_programs.rs`
- `crates/rusql-executor/src/programs.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- UNDO handlers
- GET DIAGNOSTICS
- Stacked diagnostics area

## Negative Constraints

- Do not implement handler precedence beyond MySQL documented order for the three classes above
- Do not add schema-level handlers

## Test plan

```bash
cargo test -p rusql-executor handler_continue
cargo test -p rusql-executor handler_exit
```
