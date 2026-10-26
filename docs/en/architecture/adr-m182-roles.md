# ADR: M182 roles (`CREATE ROLE` / `GRANT role` / `SET ROLE`)

**Status**: Accepted (M182 / [#334](https://github.com/tanbamboo/rusql/issues/334))  
**Date**: 2026-10-26

## Context

MySQL 8.0 treats a role as a named bundle of privileges. `CREATE ROLE` defines the name. `GRANT SELECT ON db.* TO role` attaches privileges to that name. `GRANT role TO user` records an edge. The privileges do not apply until the session runs `SET ROLE`. `CURRENT_ROLE()` reports the active names. `REVOKE role FROM user` removes the edge, and the next statement no longer has those privileges.

rusql already stores direct `GRANT` / `REVOKE` rows in `{data_dir}/mysql.user.json` (M54) and login accounts in the same file (M55). This ADR is the role graph for M182. It does not change `CONSTITUTION.md`. Mandatory roles, password expiry, audit, and the `mysql.user` table shape stay on their own issues.

## Decision

1. **Catalog.** Roles are rows in `mysql.user.json`, separate from login accounts. A role cannot authenticate. Omitted host is `%`. `CREATE ROLE` of an existing role or login account is errno **1396**. `DROP ROLE` of an unknown role is errno **1396**. `IF NOT EXISTS` / `IF EXISTS` suppress that error. Dropping a role deletes its privilege rows and every edge that mentions it.

2. **Edges.** `GRANT role [, role] TO user_or_role [, user_or_role]` and `REVOKE role FROM user_or_role` persist edges in the same file. The role must exist (errno **3523** if it does not). The grantee must be an existing account, an existing role, or `root` (errno **3523** otherwise). An empty user name is errno **3876**. Granting a role that is already granted succeeds. Revoking a role that is not granted is errno **3530**.

3. **Cycles.** MySQL allows a cycle (`GRANT r1 TO r2` and `GRANT r2 TO r1`, including a self-grant). rusql stores the edge and **detects** the cycle. Privilege lookup walks the graph with a visited set and stops when a node repeats, so a cycle cannot loop. The cycle adds no privilege the acyclic closure does not already have.

4. **Activation.** `SET ROLE` is session state. It is not written to `mysql.user.json`. It clears on `COM_RESET_CONNECTION` / `COM_CHANGE_USER`. Forms: one or more role names, `NONE`, `ALL`, `ALL EXCEPT role`, and `DEFAULT`. A named role must be granted **directly** to the current account or the statement is errno **3530** and the active set is unchanged. `ALL` activates every direct grant. `DEFAULT` activates nothing: this slice does not store default roles and does not implement mandatory roles or `activate_all_roles_on_login`.

5. **Enforcement.** Statement checks use direct grants plus the privileges of each still-granted active role, including roles granted to those roles. Before `SET ROLE`, a role grant does not allow the statement (errno **1142**). After `REVOKE` or `DROP ROLE`, the next statement drops the role from the active set and denies the statement. `root` remains a superuser for privilege checks. `SET ROLE` does not bypass the "must be granted" rule for `root`. `SELECT` with no `FROM` (including `CURRENT_ROLE()`) does not require a table privilege.

6. **`CURRENT_ROLE()`.** No active roles returns the MySQL token `NONE`. Otherwise the value is `` `name`@`host` `` values joined by commas, in activation order. `SET ROLE ALL` sorts by name then host.

7. **`SHOW GRANTS`.** Privilege lines stay in the existing M54 shape. Each direct role edge adds `` GRANT `role`@`host` TO 'user'@'host' ``. Lines are sorted. This is the documented format. It is not `SHOW GRANTS ... USING` expansion and not the backtick quoting MySQL uses on the account.

8. **Who may change the catalog.** `CREATE ROLE` / `DROP ROLE` use the same superuser gate as `CREATE USER` (errno **1227**, i18n text names `CREATE ROLE`). `GRANT` / `REVOKE` of a role use the existing superuser gate for privilege grants (errno **1142**). There is no separate `CREATE ROLE` privilege bit and no `WITH ADMIN OPTION`.

9. **Old files.** `roles` and `role_edges` default to empty when absent, so a data directory written before M182 still loads.

## Consequences

- Cloning a data directory copies the role graph with the other privilege rows.
- A login account and a role cannot share the same `user`@`host`.
- Granting a login account as if it were a role is rejected with errno **3523**. Only `CREATE ROLE` names are roles.

## Out of scope

- Mandatory roles and `activate_all_roles_on_login`
- `SET DEFAULT ROLE` (so `SET ROLE DEFAULT` is empty)
- `WITH ADMIN OPTION` on a role grant
- `SHOW GRANTS ... USING`
- Enterprise audit (M185)

## How to verify

```bash
cargo test -p rusql-sql create_role
cargo test -p rusql-executor roles
cargo test -p rusql-server roles_grant_set_role_allows_select
```

Manual:

```sql
CREATE ROLE r;
GRANT SELECT ON rusql.* TO r;
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
GRANT r TO 'app'@'%';
-- connect as app
SELECT CURRENT_ROLE();          -- NONE
SET ROLE r;
SELECT CURRENT_ROLE();          -- `r`@`%`
```
