# ADR: M186 `mysql.user` table shape

**Status**: Accepted (M186 / [#338](https://github.com/tanbamboo/rusql/issues/338))  
**Date**: 2026-10-27

## Context

Connectors probe account metadata with `SELECT` on `mysql.user` (host, plugin, and `ssl_type`). Today that name is not a table, so the statement is errno **1146**. Accounts already live in `{data_dir}/mysql.user.json` (M55, plus the M182 role graph, M183 password fields, and M184 `require_ssl`). Handing clients that JSON file would expose password hashes and would not be a SQL table.

This ADR is the queryable column subset those probes use. It does not change `CONSTITUTION.md`. `FLUSH PRIVILEGES` stays on M187. Enterprise plugins stay on M188.

## Decision

1. **Virtual table, not a file dump.** `SELECT` from `mysql.user` (any letter case, including `USE mysql` then `SELECT … FROM user`) reads login accounts from the privilege catalog. It is not a base table and not the contents of `mysql.user.json`. `INSERT`, `UPDATE`, and `DELETE` still see no base table (errno **1146**). Other `mysql.*` names, such as `mysql.db`, stay errno **1146**.

2. **Columns and MySQL 8.0 types.** Only the probe subset. `SELECT *` returns them in this order:

   | Column | MySQL 8.0 type | Value |
   |--------|----------------|-------|
   | `Host` | `char(255)` | account host |
   | `User` | `char(32)` | account name |
   | `plugin` | `char(64)` | `auth_plugin` (`caching_sha2_password` or `mysql_native_password`) |
   | `ssl_type` | `enum('','ANY','X509','SPECIFIED')` | `ANY` when the account has M184 `require_ssl`, otherwise an empty string |

   Rows are login accounts, ordered by `Host`, then `User`. An empty catalog returns those columns and zero rows, not errno 1146. Roles from M182 are not rows.

   The executor cell for "no SSL requirement" is an empty string. rusql’s text protocol encodes every empty string as SQL NULL, so a client sees NULL for that cell and the string `ANY` when `require_ssl` is set. `''` and `ANY` are the only values this slice produces.

3. **No password column.** `authentication_string` is not a column. `SELECT authentication_string FROM mysql.user` is errno **1054** (`ER_BAD_FIELD_ERROR`). The same errno applies to any other name outside the subset (`Select_priv`, `account_locked`, and the rest of the 8.0 table). Hashes stay in `mysql.user.json` and are not copied into the result. `X509` and `SPECIFIED` are legal enum members in the type and are not produced: M184 rejects those `REQUIRE` forms with errno **1064**.

4. **Privilege.** `root` can `SELECT`. Any other account needs `SELECT` on `mysql.user` (or a broader grant). Without it the statement is errno **1142**. A grant does not add the password hash to the row.

## Consequences

- Clients that `SELECT Host, User, plugin` or `ssl_type` see catalog accounts. Clients that expect the full 8.0 column list, including `authentication_string`, get errno **1054** on the missing name.
- `mysql-diff` compares `Host`, `User`, and `plugin` for an account without `REQUIRE SSL`, and includes `ssl_type` when the value is `ANY`. It does not select `authentication_string`, so the output has no hash. The empty `ssl_type` cell is not compared, because the wire sends it as NULL.
- Changing `mysql.user.json` on disk is not visible until the process reloads the catalog. That reload is M187 (`FLUSH PRIVILEGES`), not this slice.

## Out of scope

- Full MySQL 8.0 `mysql.user` column list and writable grant tables
- `mysql.db`, `mysql.tables_priv`, and the other privilege tables
- Returning or redacting `authentication_string` (the column is absent)
- Roles as `mysql.user` rows
- `FLUSH PRIVILEGES` (M187)

## How to verify

```bash
cargo test -p rusql-executor mysql_user_table
```

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH caching_sha2_password BY 'secret';
CREATE USER 'appssl'@'%' IDENTIFIED WITH caching_sha2_password BY 'secret' REQUIRE SSL;
SELECT Host, User, plugin, ssl_type FROM mysql.user WHERE User = 'appssl';
```

The `appssl` row has `plugin = caching_sha2_password` and `ssl_type = ANY`. `SELECT authentication_string FROM mysql.user` is errno **1054**. `SELECT Host FROM mysql.db` is errno **1146**.
