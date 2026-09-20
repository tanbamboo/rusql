## Goal

Support all six MySQL row trigger timings: BEFORE/AFTER × INSERT/UPDATE/DELETE.

## Category

Phase V — Stored programs (M171). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M48/M64 cover a subset. Inventory missing timings (often BEFORE UPDATE/DELETE or AFTER INSERT). OLD/NEW column refs for the timings that need them.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Each of the six timings can be created and fires on the matching DML (tests per timing)
- [ ] BEFORE can abort DML via SIGNAL (if M166 on main) or documented skip
- [ ] Existing trigger catalog rows still load (`serde(default)`)
- [ ] SHOW TRIGGERS lists Timing/Event correctly. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/stored_programs.rs`
- `crates/rusql-core/src/programs.rs`
- `crates/rusql-executor/src/programs.rs`
- `crates/rusql-executor/src/**` (DML hooks)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Statement-level FOR EACH STATEMENT
- Compound trigger bodies beyond existing BEGIN…END
- New crates

## Negative Constraints

- Do not implement INSTEAD OF (views)
- Do not add multiple triggers per timing unless MySQL 8 follow-up with FOLLOWS

## Test plan

```bash
cargo test -p rusql-executor trigger_before_update
cargo test -p rusql-executor trigger_timings
```
