# ADR: M187 `FLUSH PRIVILEGES`

**Status**: Accepted (M187 / [#339](https://github.com/tanbamboo/rusql/issues/339))  
**Date**: 2026-10-27

## Context

`CREATE USER`, `GRANT`, `REVOKE`, role changes, and password changes already update the in-memory privilege catalog and write `{data_dir}/mysql.user.json` before they return OK. A direct edit of that file is invisible until the process reads it again. MySQL's `FLUSH PRIVILEGES` is that read: it reloads the grant tables from durable storage. Returning OK without reading the file is wrong, including when a `GRANT` is already live in memory.

Reload runs on the same catalog authentication reads. The open question was whether that races a login. This ADR decides the lock order. It does not change `CONSTITUTION.md`. Enterprise plugins stay on M188.

## Decision

1. **`FLUSH PRIVILEGES` reloads the file.** `FLUSH PRIVILEGES`, `FLUSH LOCAL PRIVILEGES`, and `FLUSH NO_WRITE_TO_BINLOG PRIVILEGES` (and a comma list that is only `PRIVILEGES`) take the privilege catalog's write lock, read `mysql.user.json`, and replace the in-memory catalog with that snapshot. The statement then returns OK. It does not merge with memory and it does not write the file. A `GRANT` or `CREATE USER` that already saved the file stays in effect, because the reload reads that same file. If the file was replaced afterward, the file wins and the next connection sees the replaced accounts and grants.

2. **No torn catalog.** Authentication copies the account password out while holding the read lock, then releases it. Reload holds the write lock for the whole read-and-replace. A login therefore sees either the previous catalog or the fully parsed replacement. A parse failure (errno **1105**) leaves the previous catalog in place. A missing file reloads an empty catalog, the same result as process start when the file is absent. In-process account DDL writes the file before OK, so a following `FLUSH PRIVILEGES` does not wipe those accounts unless the file itself was removed or replaced.

3. **Who may run it.** rusql has no separate `RELOAD` privilege bit. `root` is the stand-in, matching the other account-admin gates. Any other account gets errno **1227** (`ER_SPECIFIC_ACCESS_DENIED_ERROR`) with the existing rusql wording for that errno, naming `RELOAD`. The check applies to every recognized `FLUSH` form, before the reload. An expired password is still errno **1820** and is not a flush.

4. **Other `FLUSH` forms are not executed.** `FLUSH TABLES` (including a table list and `WITH READ LOCK`), `FLUSH HOSTS`, `FLUSH STATUS`, `FLUSH LOGS`, `FLUSH BINARY LOGS`, `FLUSH RELAY LOGS`, `FLUSH USER_RESOURCES`, `FLUSH OPTIMIZER_COSTS`, and the log variants are errno **1235** (`ER_NOT_SUPPORTED_YET`) for `root`. They do not close tables, take a read lock, drop databases, flush the host cache, or reload TLS certificates. A list that mixes `PRIVILEGES` with another option is **1235** and does not reload. Unrecognized `FLUSH` syntax is errno **1064** for every account. `FLUSH HOSTS` is not implemented.

5. **Same catalog `SELECT mysql.user` reads.** After a successful reload, `SELECT mysql.user` (M186) and later logins use the replaced snapshot. Password hashes stay in the file and are still not a result column.

## Consequences

- Editing `mysql.user.json` on disk has no effect until `FLUSH PRIVILEGES` or a process restart. SQL account statements do not need `FLUSH` for the current process, and `FLUSH` still re-reads the file.
- `FLUSH TABLES` and `FLUSH TABLES WITH READ LOCK` fail with errno **1235** on rusql and succeed on MySQL 8.0. `mysql-diff` covers `FLUSH PRIVILEGES` (including `LOCAL` / `NO_WRITE_TO_BINLOG`) and does not compare those other forms.
- A non-superuser `FLUSH` is errno **1227**, including forms that `root` would get as **1235**.
- This slice does not consult `read_only` / `super_read_only`.

## Out of scope

- `FLUSH TABLES`, `FLUSH TABLES WITH READ LOCK`, and closing or dropping databases
- `FLUSH HOSTS`, `FLUSH STATUS`, and the log / optimizer / user-resource forms
- A grantable `RELOAD` privilege distinct from `root`
- Reloading TLS certificates (M181)

## How to verify

```bash
cargo test -p rusql-executor flush_privileges
cargo test -p rusql-server flush_privileges
```

```sql
FLUSH PRIVILEGES;
```

Replace `{data_dir}/mysql.user.json` with a catalog that contains a new account, run `FLUSH PRIVILEGES` as `root`, and connect as that account. `FLUSH TABLES` as `root` is errno **1235**. The same statement as another user is errno **1227**.
