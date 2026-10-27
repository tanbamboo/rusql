# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-27 |
| Branch | `issue-340-m188-enterprise-plugins` (M188 docs) |
| Next step | After this PR merges, M189 ([#341](https://github.com/tanbamboo/rusql/issues/341)) is the next sequenced issue. Label only that issue `agent-ready`. Do not mass-label Phase Y. Do not jump to #342. Remove `needs-human` from #340 when the ADR is on `main`. Ultimate MySQL 8.0 goal is **not** complete (Phases Y–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-27)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M187 (PR #441). M188 is this branch |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–W | **Complete** on `main` |
| Phase X M181 | **Complete** on `main` via PR #435. `--ssl-cert` / `--ssl-key` present the certificate. Plaintext default remains. Errno **1043** / **3159**. Not `REQUIRE SSL` |
| Phase X M182 | **Complete** on `main` via PR #436. Role graph in `mysql.user.json`. Privileges apply only while the role is active. Errnos **1396** / **3523** / **3530** / **3876** / **1227** / **1142**. Not mandatory roles |
| Phase X M183 | **Complete** on `main` via PR #437. `PASSWORD EXPIRE` still logs in; `COM_QUERY` is errno **1820** until the password changes. Reuse errno **3638**. Built-in `validate_password` off until `SET GLOBAL` (errno **1819**). Not a plugin `.so` |
| Phase X M184 | **Complete** on `main` via PR #438. `REQUIRE SSL` rejects plaintext login with errno **1045** and accepts TLS. Not `REQUIRE X509` |
| Phase X M185 | **Complete** on `main` via PR #439. Optional `--audit-log` JSON lines for connect, `COM_QUERY`, and disconnect. Not the Oracle audit plugin |
| Phase X M186 | **Complete** on `main` via PR #440. `SELECT mysql.user` returns `Host`, `User`, `plugin`, `ssl_type`. No `authentication_string` (errno **1054**). Unauthorized SELECT is errno **1142**. Other `mysql.*` tables stay errno **1146**. Not the full 8.0 grant table |
| Phase X M187 | **Complete** on `main` via PR #441. `FLUSH PRIVILEGES` reloads `mysql.user.json`. Errnos **1227** / **1105** / **1235**. Not `FLUSH TABLES` |
| Phase X M188 | **Done in this branch** ([#340](https://github.com/tanbamboo/rusql/issues/340)). Oracle Enterprise plugins are **out of scope** and are not loaded. Not a plugin loader |
| Phase Y–Z | Open. Next sequenced issue is M189 ([#341](https://github.com/tanbamboo/rusql/issues/341)). [#342](https://github.com/tanbamboo/rusql/issues/342) was already `agent-ready` and is not next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase Y through Z (including M209/M210) |

## Recent Progress

- **M188** — Oracle Enterprise plugins are an explicit **out of scope** skip: `audit_log` (not M185 `--audit-log`), `mysql_firewall`, `data_masking`, `component_masking`, `thread_pool`, `authentication_pam`, `authentication_ldap_sasl`, `authentication_ldap_simple`, `authentication_windows`, `keyring_okv`, `keyring_aws`, `keyring_hashicorp`, `keyring_oci`, `enterprise_encryption`, `version_tokens`. Same names in the en/zh-CN compatibility report and `tests/mysql-test/SKIPS.md`. Not loaded. Community `performance_schema`, logs, `validate_password`, semisync, `keyring_file`, `group_replication`, and `clone` stay in scope. Not M202. Ultimate goal is **not** complete. ADR: [adr-m188-enterprise-plugins.md](docs/en/architecture/adr-m188-enterprise-plugins.md) (#340)
- **M187 merged** — `FLUSH PRIVILEGES` (and `LOCAL` / `NO_WRITE_TO_BINLOG`) reloads `{data_dir}/mysql.user.json` under the privilege write lock and returns OK. A replaced file is what the next connection uses, including when `CREATE USER` / `GRANT` already updated memory. `root` only; other accounts are errno **1227**. Unreadable JSON is errno **1105** and keeps the previous catalog. A missing file reloads an empty catalog. `FLUSH TABLES` (including `WITH READ LOCK`), `FLUSH HOSTS`, `FLUSH STATUS`, and other recognized options are errno **1235** and do not run. Unrecognized `FLUSH` is errno **1064**. Not TLS reload. ADR: [adr-m187-flush-privileges.md](docs/en/architecture/adr-m187-flush-privileges.md) (#339 / [PR #441](https://github.com/tanbamboo/rusql/pull/441))
- **M186 merged** — `SELECT Host, User, plugin, ssl_type FROM mysql.user` reads login accounts from the privilege catalog (virtual table, not a dump of `mysql.user.json`). `ssl_type` is `ANY` when `require_ssl` is set, otherwise empty. `authentication_string` is not a column (errno **1054**), so password hashes are not returned. Empty catalog is zero rows. Non-superuser without `SELECT` is errno **1142**. `mysql.db` and other missing `mysql.*` names stay errno **1146**. Roles are not rows. Not writable and not the full 8.0 column list. ADR: [adr-m186-mysql-user.md](docs/en/architecture/adr-m186-mysql-user.md) (#338 / PR #440)
- **M185 merged** — `--audit-log=PATH` appends JSON lines (`timestamp`, `user`, `command`, `status`) for `Connect`, each finished `COM_QUERY`, and `Quit`. Off by default: no file. Password literals become `'<redacted>'`. Result rows and auth packets are not written. Not the Oracle Enterprise Audit plugin. ADR: [adr-m185-audit-log.md](docs/en/architecture/adr-m185-audit-log.md) (#337 / PR #439)
- **M184 merged** — `CREATE USER` / `ALTER USER … REQUIRE SSL` stores `require_ssl` on the account in `mysql.user.json`. Plaintext login and plaintext `COM_CHANGE_USER` are errno **1045**. The same account completes the M181 TLS handshake and can query. Accounts without the flag stay plaintext. `REQUIRE NONE` clears it. `SHOW CREATE USER` appends ` REQUIRE SSL`. `REQUIRE X509` / `ISSUER` / `SUBJECT` / `CIPHER` are errno **1064**. Errno **3159** stays `--require-secure-transport`. ADR: [adr-m184-require-ssl.md](docs/en/architecture/adr-m184-require-ssl.md) (#336 / PR #438)
- **M183 merged** — `ALTER USER … PASSWORD EXPIRE` / `CREATE USER … PASSWORD EXPIRE`. Login still succeeds. `COM_QUERY` is errno **1820** until `ALTER USER` / `SET PASSWORD` sets a new password. `PASSWORD HISTORY` / `PASSWORD REUSE INTERVAL` are errno **3638** (history on the account row, not a separate table). `validate_password.policy` defaults to `OFF`; `LOW` / `MEDIUM` / `STRONG` are errno **1819**. Lifetime: `NEVER` / `DEFAULT` / `INTERVAL n DAY` and `default_password_lifetime` (default 0). Missing account errno **1396**. Hashing unchanged. Not `PASSWORD REQUIRE CURRENT` or a plugin `.so`. ADR: [adr-m183-password-policy.md](docs/en/architecture/adr-m183-password-policy.md) (#335 / PR #437)
- **M182 merged** — `CREATE ROLE` / `DROP ROLE`, `GRANT role TO` / `REVOKE role FROM`, `SET ROLE` (`NONE` / `ALL` / `ALL EXCEPT` / `DEFAULT` / named), `CURRENT_ROLE()`. Role graph in `{data_dir}/mysql.user.json` (`roles`, `role_edges`). Privileges on a role allow statements only after `SET ROLE`. `REVOKE` removes access on the next statement. `SHOW GRANTS` adds `` GRANT `role`@`host` TO 'user'@'host' ``. Cycles are stored and detected; the privilege walk stops on a repeat. Errnos **1396**, **3523**, **3530**, **3876**, **1227**, **1142**. Not mandatory roles, `SET DEFAULT ROLE`, or `WITH ADMIN OPTION`. ADR: [adr-m182-roles.md](docs/en/architecture/adr-m182-roles.md) (#334 / PR #436)
- **M181 merged** — TLS listener. `--ssl-cert` and `--ssl-key` (PEM, both required) load rustls and advertise `CLIENT_SSL`. The server presents that certificate. `mysql --ssl-mode=REQUIRED` connects. Without the flags the greeting omits `CLIENT_SSL`, so REQUIRED fails in the client (2026) and a raw SSL request is errno **1043**. Plaintext login still works, including on a TLS listener. `--require-secure-transport` rejects plaintext with errno **3159** before authentication. Not per-account `REQUIRE SSL` / `REQUIRE X509` (M184). Private keys are not logged or committed. ADR: [adr-m181-tls.md](docs/en/architecture/adr-m181-tls.md) (#333 / PR #435)
- **M176 merged** — `@@gtid_executed` / `Executed_Gtid_Set` from applied GTIDs in `{data_dir}/gtid.json`. Empty string on a fresh server. `COMMIT` records produced GTIDs. `START SLAVE` records applied GTIDs. Restart reloads the set. `@@session.gtid_executed` errno **1238**. SET errno **1238**. Not `gtid_mode` UI or Group Replication (#328 / PR #429, `cd67acc`)
- **M180 merged** — `WRITE_ROWS` / `UPDATE_ROWS` / `DELETE_ROWS` v2 with extra-data length. `binlog_row_image` MINIMAL/NOBLOB/FULL. INSERT still logs the full row. Invalid image errno **1231**. Corrupt v2 header errno **1610** (#332 / PR #433)
- **M179 merged** — `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` (and `START REPLICA` synonyms). Replica connects, dumps, and applies row events. `SHOW SLAVE STATUS` IO/SQL flags. Errnos 1200, 1198, 1231, 2003 (#331 / PR #432)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
node scripts/doc-parity.mjs
node scripts/mysql-diff.mjs
```

## Notes

- `--audit-log` is off unless the path is set. The file is JSON lines, not the Oracle `audit_log` plugin. `status` 0 is a response without an ERR packet. Failed handshakes are not logged. `COM_STMT_EXECUTE` is not a query line. No rotation.
- Expired accounts can still authenticate. `COM_QUERY` is errno **1820** until that account changes its password. `validate_password` stays `OFF` unless `SET GLOBAL validate_password.policy` is `LOW`, `MEDIUM`, or `STRONG`. History lives on the account row in `mysql.user.json` (cap 64). Not `PASSWORD REQUIRE CURRENT`.
- Roles are not login accounts. The same `user`@`host` cannot be both. Only `CREATE ROLE` names can be granted as roles (errno **3523** otherwise). `SET ROLE DEFAULT` activates nothing in this slice. `SELECT` with an empty `FROM`, including `CURRENT_ROLE()`, does not require a table privilege.
- A `GRANT role` mixed into one COM_QUERY with other SQL is errno **1064** (sqlparser requires `ON`). Send role grants as their own statements. `CREATE` / `DROP` / `SET ROLE` inside a parsed batch are handled.
- mysql-diff `roles` compares statement success and failure, including `CURRENT_ROLE()` = `NONE` and `SET ROLE` **3530** for an ungranted role. The allow/deny path is `roles_grant_set_role_allows_select` (each mysql-diff step is a new connection).
- M181 stack is rustls 0.23 (`ring`) + tokio-rustls, TLS 1.2 and 1.3. `mysql-diff` keeps `--ssl-mode=DISABLED`. Account `REQUIRE SSL` is enforced after that upgrade: plaintext is errno **1045**, not **3159**.
- `REQUIRE X509` / issuer / subject / cipher are errno **1064**. Do not commit certificates.
- `mysql.user` is a virtual catalog table: `Host`, `User`, `plugin`, `ssl_type`. `ssl_type` is `ANY` or an empty string. The text protocol encodes that empty string as SQL NULL. `authentication_string` is errno **1054**. Do not put password hashes in mysql-diff output. Roles are not rows. Disk edits to `mysql.user.json` apply on `FLUSH PRIVILEGES` (or process restart). The reload replaces memory; it does not merge.
- `FLUSH TABLES` / `FLUSH TABLES WITH READ LOCK` are errno **1235** for `root` and do not lock or drop databases. Non-root `FLUSH` is errno **1227**. mysql-diff `flush_privileges` compares only the privilege form (MySQL succeeds on `FLUSH TABLES`).
- Do not mass-label Phase Y–Z. M188 lists Oracle Enterprise plugins as **out of scope** and does not load them. The next sequenced issue is M189 ([#341](https://github.com/tanbamboo/rusql/issues/341)). [#342](https://github.com/tanbamboo/rusql/issues/342) (M190) was already `agent-ready` and is not next.
- `--audit-log` is the M185 JSON file, not the Oracle `audit_log` plugin. `performance_schema`, `validate_password`, semisync, `group_replication`, and `clone` are not the M188 skip.
- Overall MySQL 8.0 goal is **not** complete.
