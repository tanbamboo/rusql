## Goal

Dump/restore + ORM suite + replication + locking all green vs Docker `mysql:8.0` — last gate for the ultimate goal.

## Category

Phase Z — Remaining MySQL 8.0 surface (M210). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

All prior phase exits green. Evidence: mysqldump load, ORM migrations, replica consistency, FOR UPDATE suite. Only then HANDOFF may say ultimate goal achieved.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Documented dump/restore of the simple + extended fixture vs MySQL 8.0
- [ ] ORM suite (at least one of Django/Rails/GORM/sqlx — listed) passes
- [ ] Replication + locking suites green
- [ ] M209 matrix has no in-scope Missing. Sensors green

## File Boundaries

Allowed:
- `scripts/**`
- `.github/workflows/**`
- `tests/**`
- `docs/en/**`, `docs/zh-CN/**`
- `HANDOFF.md`
- `CHANGELOG.md`
- `crates/**` only for harness hooks required by the gate

Forbidden:
- Declaring goal complete without evidence artifacts
- Force-push
- CONSTITUTION.md

## Negative Constraints

- Do not skip Docker comparison
- Do not treat Phase R probe-only as production drop-in

## Test plan

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-diff.mjs
node scripts/mysql-gap-probe.mjs
```
