# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-27 |
| Branch | `issue-336-m184-require-ssl` (M184) |
| Next step | After this PR merges: label M185 ([#337](https://github.com/tanbamboo/rusql/issues/337)) `agent-ready` and remove `needs-human` from #336. Do not mass-label later phases. Do not jump to #342. Ultimate MySQL 8.0 goal is **not** complete (Phases X–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence on `main`.

## Status vs goal (2026-10-27)

| Layer | Status |
|-------|--------|
| CI on `main` | Green through M183 (PR #437, `ff763d0`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q–W | **Complete** on `main` |
| Phase X M181 | **Complete** on `main` via PR #435. `--ssl-cert` / `--ssl-key` present the certificate. Plaintext default remains. Errno **1043** / **3159**. Not `REQUIRE SSL` |
| Phase X M182 | **Complete** on `main` via PR #436. Role graph in `mysql.user.json`. Privileges apply only while the role is active. Errnos **1396** / **3523** / **3530** / **3876** / **1227** / **1142**. Not mandatory roles |
| Phase X M183 | **Complete** on `main` via PR #437. `PASSWORD EXPIRE` still logs in; `COM_QUERY` is errno **1820** until the password changes. Reuse errno **3638**. Built-in `validate_password` off until `SET GLOBAL` (errno **1819**). Not a plugin `.so` |
| Phase X M184 | **This branch.** `REQUIRE SSL` on the account. Plaintext login errno **1045**. TLS login uses the M181 listener. Not `REQUIRE X509` |
| Phase X M185–M188 | Open. Next after merge is M185 ([#337](https://github.com/tanbamboo/rusql/issues/337)), still `needs-human` until its own ADR |
| Phase Y–Z | Open. [#342](https://github.com/tanbamboo/rusql/issues/342) was already `agent-ready` and is not next |

| Estimated surface | ~45–70% client-visible; remaining work is Phase X through Z (including M209/M210) |

## Recent Progress

- **M184** — `CREATE USER` / `ALTER USER … REQUIRE SSL` stores `require_ssl` on the account in `mysql.user.json`. Plaintext login and plaintext `COM_CHANGE_USER` are errno **1045**. The same account completes the M181 TLS handshake and can query. Accounts without the flag stay plaintext. `REQUIRE NONE` clears it. `SHOW CREATE USER` appends ` REQUIRE SSL`. `REQUIRE X509` / `ISSUER` / `SUBJECT` / `CIPHER` are errno **1064**. Errno **3159** stays `--require-secure-transport`. ADR: [adr-m184-require-ssl.md](docs/en/architecture/adr-m184-require-ssl.md) (#336)
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
cargo test -p rusql-core require_ssl
cargo test -p rusql-executor show_create_user
cargo test -p rusql-server require_ssl
cargo test
node scripts/harness-validate.mjs
node scripts/check-changelog.mjs
node scripts/mysql-diff.mjs
```

## Notes

- Expired accounts can still authenticate. `COM_QUERY` is errno **1820** until that account changes its password. `validate_password` stays `OFF` unless `SET GLOBAL validate_password.policy` is `LOW`, `MEDIUM`, or `STRONG`. History lives on the account row in `mysql.user.json` (cap 64). Not `PASSWORD REQUIRE CURRENT`.
- Roles are not login accounts. The same `user`@`host` cannot be both. Only `CREATE ROLE` names can be granted as roles (errno **3523** otherwise). `SET ROLE DEFAULT` activates nothing in this slice. `SELECT` with an empty `FROM`, including `CURRENT_ROLE()`, does not require a table privilege.
- A `GRANT role` mixed into one COM_QUERY with other SQL is errno **1064** (sqlparser requires `ON`). Send role grants as their own statements. `CREATE` / `DROP` / `SET ROLE` inside a parsed batch are handled.
- mysql-diff `roles` compares statement success and failure, including `CURRENT_ROLE()` = `NONE` and `SET ROLE` **3530** for an ungranted role. The allow/deny path is `roles_grant_set_role_allows_select` (each mysql-diff step is a new connection).
- M181 stack is rustls 0.23 (`ring`) + tokio-rustls, TLS 1.2 and 1.3. `mysql-diff` keeps `--ssl-mode=DISABLED`. Account `REQUIRE SSL` is enforced after that upgrade: plaintext is errno **1045**, not **3159**.
- `REQUIRE X509` / issuer / subject / cipher are errno **1064**. Do not commit certificates.
- Do not mass-label Phase X–Z. Next sequenced issue after M184 merges is M185 ([#337](https://github.com/tanbamboo/rusql/issues/337)). [#342](https://github.com/tanbamboo/rusql/issues/342) (M190) was already `agent-ready` and is not next.
- Overall MySQL 8.0 goal is **not** complete.
