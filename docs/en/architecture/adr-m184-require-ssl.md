# ADR: M184 account `REQUIRE SSL`

**Status**: Accepted (M184 / [#336](https://github.com/tanbamboo/rusql/issues/336))  
**Date**: 2026-10-27

## Context

MySQL 8.0 can require an encrypted connection for one account (`CREATE USER` / `ALTER USER … REQUIRE SSL`) without turning on the server-wide `require_secure_transport` flag. A plaintext login for that account fails. A TLS login with the correct password succeeds. Accounts that do not set the clause still connect in plaintext, including to a listener that also offers TLS.

rusql already presents a certificate when `--ssl-cert` and `--ssl-key` are set (M181, [adr-m181-tls.md](adr-m181-tls.md)). That listener still accepts plaintext. This ADR is the per-account flag. It does not change `CONSTITUTION.md`. `REQUIRE X509`, issuer, subject, and cipher lists stay out of this slice.

## Decision

1. **Flag on the account row.** `CREATE USER … REQUIRE SSL` and `ALTER USER … REQUIRE SSL` set `require_ssl` on the account in `{data_dir}/mysql.user.json`. `REQUIRE NONE` clears it. The clause may appear before or after `IDENTIFIED` and the M183 `PASSWORD` clauses. Old catalogs load with the flag false. The clause is an admin change (errno **1227** for a non-superuser), including when it is combined with `IDENTIFIED BY`.

2. **Plaintext login is errno 1045.** If the matched account has `require_ssl` and the socket is not TLS, the server sends errno **1045** (`ER_ACCESS_DENIED_ERROR`) and the existing access-denied text (i18n). It does not finish authentication. This is the MySQL client errno for an account SSL restriction. Errno **3159** (`ER_SECURE_TRANSPORT_REQUIRED`) stays the M181 `--require-secure-transport` rejection and is not reused here. `COM_CHANGE_USER` on a plaintext session to a `REQUIRE SSL` account is the same errno **1045**; the previous session user stays.

3. **TLS login succeeds.** On the M181 rustls listener, the same account completes the existing password handshake after the TLS upgrade and may `COM_QUERY`. The server still does not request a client certificate. A listener without `--ssl-cert` / `--ssl-key` cannot satisfy the flag, so that account cannot log in until TLS is configured.

4. **Other accounts stay plaintext.** An account without the flag still logs in without TLS, including when the listener has a certificate and when a sibling account requires SSL. `--require-secure-transport` is unchanged.

5. **`SHOW CREATE USER`.** When the flag is set, the reconstructed statement ends with ` REQUIRE SSL`. The password hash stays out of the cell. When the flag is clear, the cell does not add `REQUIRE SSL` (this stub still omits MySQL's other default clauses).

6. **Skipped forms are a syntax error.** `REQUIRE X509`, `REQUIRE ISSUER`, `REQUIRE SUBJECT`, and `REQUIRE CIPHER` do not change the account. The statement is errno **1064**. They are not stored and not treated as `REQUIRE SSL`.

## Consequences

- Operators who need `REQUIRE SSL` must start the server with `--ssl-cert` and `--ssl-key`. Plaintext clients for that account see errno 1045.
- `mysql-diff` stays `--ssl-mode=DISABLED`. It checks that the DDL succeeds. The plaintext-versus-TLS login check is `cargo test -p rusql-server require_ssl`, which generates an ephemeral certificate and does not commit it.
- rusql has no Unix socket listener, so a socket is not treated as a secure transport.

## Out of scope

- `REQUIRE X509`, `REQUIRE ISSUER`, `REQUIRE SUBJECT`, `REQUIRE CIPHER`
- Client-certificate request, `--ssl-ca`, or subject matching
- Reloading the server certificate without a restart (M181)

## How to verify

```bash
cargo test -p rusql-server require_ssl
cargo test -p rusql-executor show_create_user
cargo test -p rusql-core require_ssl
```

Manual (do not commit the key):

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data \
  --ssl-cert ./server.crt --ssl-key ./server.key
```

```sql
CREATE USER 'app'@'%' IDENTIFIED BY 'secret' REQUIRE SSL;
SHOW CREATE USER 'app'@'%';
```

`mysql --ssl-mode=DISABLED -u app -psecret` is errno **1045**. `mysql --ssl-mode=REQUIRED -u app -psecret` connects. A user created without `REQUIRE SSL` still connects with `--ssl-mode=DISABLED`.
