## Goal

Control flow in stored procedures: `IF`/`THEN`/`ELSE`, `WHILE`, `LOOP`, `LEAVE`.

## Category

Phase V — Stored programs (M168). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

After M167 local vars. mysql-test sp-control. Keep REPEAT/CASE-statement for a follow-up if needed; IF+WHILE+LOOP+LEAVE is this slice.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] IF n>0 THEN SELECT 1; ELSE SELECT 0; END IF inside CALL matches branch
- [ ] WHILE n>0 DO SET n=n-1; END WHILE terminates; LEAVE exits a labeled LOOP
- [ ] Infinite loop guard: document max iterations or timeout (must not hang CI)
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
- Cursors (M169)
- Triggers control-flow expansion beyond reuse
- New crates

## Negative Constraints

- Do not implement GOTO
- Do not add CASE statement if IF covers tests — optional extra

## Test plan

```bash
cargo test -p rusql-executor proc_if
cargo test -p rusql-executor proc_while
```
