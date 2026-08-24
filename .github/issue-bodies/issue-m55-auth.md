## Goal

Support multiple user accounts and `mysql_native_password` authentication plugin alongside caching_sha2.

## Category

Phase M — Security & privileges.

## Depends on

- M7 caching_sha2, M54 GRANT/REVOKE

## Acceptance Criteria

- [ ] `CREATE USER 'app'@'%' IDENTIFIED BY 'secret'` persisted
- [ ] Login as non-root user with correct password succeeds
- [ ] `mysql_native_password` handshake path for older clients
- [ ] `DROP USER` removes account
- [ ] Document auth plugin selection in user-guide

## File Boundaries

- `crates/rusql-protocol/**`, `crates/rusql-server/**`, `crates/rusql-core/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No LDAP/Kerberos plugins
- No `caching_sha2_password` fast path changes that break existing tests

## Note

Milestone id `M55-auth` — distinct from closed M32 MVCC issue numbering context; tracked as **M55-auth** in roadmap to avoid confusion with storage M55 label in docs (issue title uses M55-auth).
