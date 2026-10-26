# ADR: M183 password policy (`PASSWORD EXPIRE` / validate / reuse)

**Status**: Accepted (M183 / [#335](https://github.com/tanbamboo/rusql/issues/335))  
**Date**: 2026-10-26

## Context

MySQL 8.0 can mark a `caching_sha2_password` account so the next session must change the password. Login still succeeds. Any later statement except a password change fails with errno **1820** (`ER_MUST_CHANGE_PASSWORD`) until `ALTER USER` or `SET PASSWORD` sets a new password. Accounts can also keep a reuse limit (`PASSWORD HISTORY`, `PASSWORD REUSE INTERVAL`) and a lifetime (`PASSWORD EXPIRE INTERVAL` / `default_password_lifetime`). The `validate_password` component, when installed, rejects weak passwords with errno **1819**.

rusql already verifies `caching_sha2_password` and `mysql_native_password` (M7 / M55) and stores accounts in `{data_dir}/mysql.user.json`. This ADR is the M183 subset. It does not change `CONSTITUTION.md`. Per-account `REQUIRE SSL` stays on M184.

## Decision

1. **Expiration is must-change.** `ALTER USER user PASSWORD EXPIRE` and `CREATE USER … PASSWORD EXPIRE` set `password_expired` on the account row. A correct password still completes the handshake. `COM_QUERY` (and `COM_INIT_DB`, field list, prepared statements, process info) then returns errno **1820** until that account sets a new password. `COM_PING` still works. Changing the password clears the flag unless the same statement also says `PASSWORD EXPIRE`.

2. **Who may change a password.** A superuser (`root`) may `CREATE` / `DROP` / `ALTER` accounts and `SET PASSWORD FOR` anyone. Any account may change its own password with `SET PASSWORD`, `SET PASSWORD FOR USER()`, or `ALTER USER USER() IDENTIFIED BY` (or its own `user`@`host`) when that statement does not also change policy. Policy clauses (`PASSWORD EXPIRE`, `PASSWORD HISTORY`, `PASSWORD REUSE INTERVAL`) stay on the superuser gate (errno **1227**). A missing account is errno **1396**.

3. **Lifetimes.** `PASSWORD EXPIRE NEVER`, `PASSWORD EXPIRE DEFAULT`, and `PASSWORD EXPIRE INTERVAL n DAY` are stored on the account. `DEFAULT` uses the global `default_password_lifetime` (default **0**, which means time-based expiry is off, matching MySQL 8.0). `NEVER` does not expire by time and does not clear an already-expired flag. A non-zero lifetime expires the account when `now >= password_changed_unix + days * 86400`. A change time of `0` (catalogs written before M183) does not expire by time. `SET GLOBAL default_password_lifetime = n` persists in `mysql.user.json`.

4. **Reuse uses the account row.** There is no `mysql.password_history` table. Previous passwords are kept on the same account record, newest first, in the same representation M55 already uses for the current password (this does not change hashing). Global `password_history` and `password_reuse_interval` default to **0** (checks off). `ALTER USER … PASSWORD HISTORY n` and `PASSWORD REUSE INTERVAL n DAY` override that account; `DEFAULT` follows the global value. When the effective count is N > 0, the new password must not match the current password or the newest retained values that still fall inside that count (errno **3638**). The English text is `Cannot use these credentials for 'user@host' …`. mysql-diff sets `PASSWORD HISTORY` on `CREATE USER` because MySQL 8.0.46 rejects that reuse only when the history policy was already active for the retained password. Turning history on later does not make MySQL reject the password that was current at that moment; rusql still rejects the plaintext. When the reuse interval is D > 0, it also must not match a retained password whose change time is less than D days ago. Both limits apply together. History is capped at 64 entries.

5. **`validate_password` is built in, off by default.** There is no plugin `.so` and no `INSTALL COMPONENT` (M201). `validate_password.policy` defaults to `OFF`, so `CREATE USER` / `ALTER USER` / `SET PASSWORD` accept the same passwords as today, including the `'secret'` fixtures. `SET GLOBAL validate_password.policy = LOW|MEDIUM|STRONG` (or `0|1|2`) turns checks on. `LOW` requires `validate_password.length` characters (default 8; `0` disables the length check). `MEDIUM` also requires one ASCII lower, one upper, one digit, and one non-alphanumeric. `STRONG` is `MEDIUM` plus a rejection when the password contains the account name and that name is at least 3 characters. There is no dictionary file. Failure is errno **1819**. `SET` without `GLOBAL` / `@@GLOBAL` is errno **1229**. A bad value is errno **1231**.

6. **Reads.** `SELECT @@validate_password.length`, `@@validate_password.policy`, `@@password_history`, `@@password_reuse_interval`, and `@@default_password_lifetime` (optional `GLOBAL.` / `SESSION.` prefix) return the global value. `SHOW VARIABLES` does not list them (the stub catalog is unchanged).

7. **Non-expired login is unchanged.** Accounts that are not expired still authenticate with `caching_sha2_password` or `mysql_native_password` and may `COM_QUERY`. Hash functions are not modified. Old `mysql.user.json` files load with the new fields defaulted.

## Consequences

- An expired account can connect and then only change its own password until that succeeds.
- Turning on `LOW` with length 8 rejects short passwords that existing tests use. The default stays `OFF` so those statements keep working.
- Cloning a data directory copies expiration, history, and the global policy with the account file. History entries are the same password strings M55 stores. Do not log them.

## Out of scope

- `PASSWORD REQUIRE CURRENT` / `REPLACE` (errno 3891–3893)
- A dictionary file for `STRONG`
- `validate_password` as a loadable `.so` (M201)
- Dual-password / retention-days beyond the in-row history cap
- Per-account `REQUIRE SSL` (M184)

## How to verify

```bash
cargo test -p rusql-protocol password_expire
cargo test -p rusql-server password_expire
```

Manual:

```sql
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
ALTER USER 'app'@'%' PASSWORD EXPIRE;
-- connect as app; SELECT 1 is errno 1820
ALTER USER USER() IDENTIFIED BY 'secret2';
SELECT 1;  -- ok
```

`SET GLOBAL validate_password.policy = LOW` then `CREATE USER … IDENTIFIED BY 'short'` is errno **1819**.
