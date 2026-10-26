# ADR: M181 TLS listener (`--ssl-cert` / `--ssl-key`)

**Status**: Accepted (M181 / [#333](https://github.com/tanbamboo/rusql/issues/333))  
**Date**: 2026-10-26

## Context

MySQL 8.0 clients encrypt the login and command phase with TLS. The official client flag `--ssl-mode=REQUIRED` connects only when the server greeting advertises `CLIENT_SSL` (`0x0800`) and the following TLS handshake succeeds. `REQUIRED` encrypts the session. It does not verify a CA. `VERIFY_CA` and `VERIFY_IDENTITY` do.

rusql's greeting omitted `CLIENT_SSL`. A flag that was parsed and ignored would not satisfy `REQUIRED`. This ADR is the TLS listener subset for M181. Account `REQUIRE SSL` / `REQUIRE X509` is M184. Replica SSL options stay ignored (M179).

Trust: this document is the accepted TLS decision for M181. It does not change `CONSTITUTION.md`. Later Phase X issues (roles, password expiry, audit, `mysql.user` shape) keep their own `needs-human` gates.

## Decision

1. **Stack.** [rustls](https://github.com/rustls/rustls) 0.23 with the `ring` provider and `tokio-rustls`. TLS 1.2 and TLS 1.3. No new cipher, no OpenSSL, no custom certificate verifier. The server does not request a client certificate.

2. **CLI.** `--ssl-cert` is a PEM certificate chain. `--ssl-key` is a PEM private key (PKCS#8, PKCS#1, or SEC1). Both are required together. Neither means the listener stays plaintext. One without the other, an unreadable path, or PEM that rustls rejects is a startup error (i18n). The process does not listen. Key bytes are not logged and are not committed.

3. **Greeting.** `CLIENT_SSL` is set only when a certificate loaded. The default server, including `mysql-diff` (`--ssl-mode=DISABLED`), does not advertise it.

4. **Upgrade.** After the greeting (sequence 0), a 32-byte SSL request with `CLIENT_SSL` and `CLIENT_PROTOCOL_41` (sequence 1) starts the rustls handshake on that TCP connection. The server presents the configured chain. The MySQL login packet is sequence 2. Authentication packets start at sequence 3. A longer first packet is a normal login, not TLS.

5. **Plaintext still works** when `--require-secure-transport` is off, including on a TLS-enabled listener. Clients that never send an SSL request complete the existing handshake. Default local tests stay plaintext.

6. **No certificate, client requires TLS.** The greeting's `CLIENT_SSL` bit is clear. `mysql --ssl-mode=REQUIRED` fails in the client with `CR_SSL_CONNECTION_ERROR` (2026) and does not log in. rusql does not invent a server packet for that client-side refusal. A raw client that sends an SSL request anyway receives errno **1043** (`ER_HANDSHAKE_ERROR`, i18n "Bad handshake") at sequence 2. That packet is not treated as a username.

7. **`--require-secure-transport`.** Off by default, matching MySQL 8.0. When on, a plaintext login packet is rejected with errno **3159** (`ER_SECURE_TRANSPORT_REQUIRED`) before authentication. The message is the MySQL text (i18n). A TLS login on the same listener succeeds. This is the server-wide flag. It is not `CREATE USER … REQUIRE SSL` (M184) and not `SET GLOBAL` in this slice. rusql has no Unix socket listener, so there is no socket exemption.

8. **Broken TLS.** If the bytes after a valid SSL request are not a successful TLS handshake, the connection closes. The client observes 2026. The server log is the i18n TLS failure string plus the rustls error. PEM contents are not included.

9. **Auth stays on.** Password verification, `caching_sha2_password`, and `mysql_native_password` are unchanged. TLS does not skip authentication. The existing `CLIENT_SSL` branch that accepts a full-auth password only after the client set that capability still applies, and only once the socket is actually TLS.

## Consequences

- Operators generate certificates outside the repo (`openssl` or another CA). A self-signed certificate is enough for `--ssl-mode=REQUIRED`.
- `mysql-diff` keeps `--ssl-mode=DISABLED` and does not need certificates.
- Cloning a data directory does not copy TLS material. Certificates are process arguments, not catalog files.

## Out of scope

- Per-account `REQUIRE SSL`, `REQUIRE X509`, issuer/subject checks, `--ssl-ca` client authentication (M184)
- Reloading a certificate without restart
- Replica client TLS (`MASTER_SSL` remains ignored, M179)
- Enterprise audit / keyring plugins (M185, M188)

## How to verify

```bash
cargo test -p rusql-server tls_listen
cargo test -p rusql-protocol ssl_request
```

Manual client (certificate files must not be committed):

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data \
  --ssl-cert ./server.crt --ssl-key ./server.key
mysql --protocol=TCP -h 127.0.0.1 -P 3307 -u root --ssl-mode=REQUIRED -e "SELECT 1"
```

The same `mysql` command against a server started without `--ssl-cert` / `--ssl-key` must fail. `--ssl-mode=DISABLED` must succeed against both servers unless `--require-secure-transport` is set, in which case plaintext login is errno 3159.
