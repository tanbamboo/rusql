# ADR: M6 Authentication and DML Extensions

**Status**: Accepted (via [#16](https://github.com/tanbamboo/rusql/issues/16))  
**Date**: 2026-06-30

## Decision

1. **`mysql_native_password` verification** when `--auth-password` is set (env `RUSQL_AUTH_PASSWORD`).
2. **Default remains open** (no password check) for zero-config dev/tests.
3. **SQL**: `DROP TABLE`, `DELETE FROM … WHERE col = literal`, `DELETE FROM table` (all rows).

## Deferred

- `caching_sha2_password` ([#7](https://github.com/tanbamboo/rusql/issues/7))
- Multi-user auth file / `mysql.user` table
- `UPDATE`, `TRUNCATE`, replication ([#5](https://github.com/tanbamboo/rusql/issues/5))

## Human feedback welcome

Reply on #16 if you want **require-auth by default** in a future release.
