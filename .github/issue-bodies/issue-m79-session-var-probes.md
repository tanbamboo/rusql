## Goal

Extend the M77 `@@` stub set with the remaining high-frequency connector probes (`auto_increment_increment`, `time_zone` / `system_time_zone`, `transaction_isolation` / `tx_isolation`, `max_allowed_packet`, `license`) so JDBC/ORM handshake `SELECT @@session.…` batches succeed instead of errno 1193.

## Background

Phase Q after M78. M77 covers version, autocommit, charset, collation, and sql_mode. MySQL Connector/J still sends a multi-column `SELECT @@session.auto_increment_increment, @@time_zone, @@transaction_isolation, …` on connect; any unknown name fails the whole query. Full `SHOW VARIABLES` and `SET @@` remain later.

## Acceptance Criteria

- [ ] `SELECT @@auto_increment_increment` and `@@session.auto_increment_increment` return `1`
- [ ] `SELECT @@time_zone` and `@@system_time_zone` return documented stubs (`SYSTEM` and `UTC` respectively, or equivalent — document which)
- [ ] `SELECT @@transaction_isolation` and `@@tx_isolation` return `REPEATABLE-READ` (rusql snapshot isolation)
- [ ] `SELECT @@max_allowed_packet` returns a positive integer stub (document value, e.g. `67108864`)
- [ ] `SELECT @@license` returns a non-empty stub (e.g. `GPL`)
- [ ] Unknown `@@` names still return errno 1193 via existing i18n
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` where Docker MySQL/CLI defaults differ; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (extend M77 stub table)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (only if new errors)
- `crates/rusql-core/src/**` (session fields only if SET persistence is already present)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Full `SHOW VARIABLES` catalog
- `SET @@` / user-defined variables `@foo` (unless already present as a one-line wire-up)
- `SET NAMES` persistence
- New crates

## Negative Constraints

- Do not enumerate every MySQL 8.0 sysvar; only the listed connector-probe stubs plus the existing M77 set
- Do not implement `SET SESSION` persistence in this issue
- Do not implement `SHOW VARIABLES`

## Test plan

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-server session_var
```
