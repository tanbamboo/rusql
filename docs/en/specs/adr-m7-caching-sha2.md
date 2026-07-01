# ADR: M7 caching_sha2_password

**Status**: Accepted (via [#7](https://github.com/tanbamboo/rusql/issues/7))  
**Date**: 2026-06-30  
**Supersedes in part**: [adr-auth-mvp.md](adr-auth-mvp.md) (native-only default)

## Decision

1. Handshake **defaults** to `caching_sha2_password` (MySQL 8.0 behavior).
2. Implement **fast-auth** verify (32-byte SHA256 scramble).
3. Keep **`mysql_native_password`** via plugin fallback when client requests it.
4. **Defer** RSA full-auth and AuthMoreData `0x04` path (no TLS RSA yet).

## Consequences

- MySQL 8 clients can connect without `--default-auth=mysql_native_password`.
- `--auth-password` verifies both plugins.

## Human input welcome

Comment on #7 if you need RSA full-auth without SSL for legacy clients.
