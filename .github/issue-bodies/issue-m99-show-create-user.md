## Goal

Implement `SHOW CREATE USER` so clients and GUIs that probe account DDL get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M98. sqlparser 0.53 may not expose `SHOW CREATE USER`; it typically needs a parse rewrite (same pattern as `SHOW CREATE PROCEDURE`). phpMyAdmin, mysql CLI, and dump tools send `SHOW CREATE USER 'u'@'h'` after `CREATE USER`. M55 already stores accounts (`user`, `host`, auth plugin) in the privilege catalog. This slice reconstructs documented `CREATE USER … IDENTIFIED WITH …` DDL from that catalog. Password hashes stay out of the reconstructed cell (or a documented stub plugin clause without a live hash). `SHOW CREATE EVENT`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE USER 'u'@'h'` (and `user@host` / current-user form if a one-line parse is natural) returns MySQL-shaped columns including at least `CREATE USER for` / `Create User` (or the MySQL 8.0 pair of columns)
- [ ] The create-user cell is reconstructed from the M55 account catalog (`CREATE USER \`u\`@\`h\` IDENTIFIED WITH '{plugin}'`); not a live password-hash dump
- [ ] Unknown account is an error (same class as existing missing-user handling; MySQL-like errno 3162 `ER_NO_SUCH_USER` if a one-line `ExecError::Mysql` path is natural)
- [ ] `SHOW FUNCTION STATUS` from M98, `SHOW PROCEDURE STATUS` from M97, and `SHOW CREATE FUNCTION` from M96 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (plugin/hash extras differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE USER` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (account lookup only; no WAL format change)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `SHOW CREATE EVENT`
- Persisting or dumping password hashes in SHOW output
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE USER` parity; document reconstructed catalog DDL (plugin name, no live hash)
- Do not change `SHOW FUNCTION STATUS` output from M98
- Do not implement `SHOW CREATE EVENT`
- Do not invent TLS / resource-limit / DEFAULT ROLE clauses

## Test plan

```bash
cargo test -p rusql-sql show_create_user
cargo test -p rusql-executor show_create_user
cargo test -p rusql-server show_create_user
```
