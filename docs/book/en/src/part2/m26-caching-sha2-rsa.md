# M26 — caching_sha2 RSA full auth

**Issue #49**

## Problem

M7 implemented `caching_sha2_password` **fast-path** only (32-byte SHA256 scramble). Clients on non-TLS connections that cannot use fast auth need the **RSA public-key exchange** defined in MySQL 8 protocol.

## Decision

- On fast-auth success: `AuthMoreData(0x01, 0x03)` then OK (MySQL-compatible).
- On empty/non-fast initial response: `AuthMoreData(0x01, 0x04)` → client `0x02` → server PEM → RSA-OAEP(SHA1) encrypted XOR-scrambled password.
- Generate 2048-bit RSA key pair when `--auth-password` is enabled.
- Keep `mysql_native_password` single-round path unchanged.

## Harness lesson

> `accepts_caching_sha2_rsa_when_auth_enabled` simulates the full wire exchange without fast-auth scramble — catches seq-number and padding bugs.

## References

- [MySQL caching_sha2 blog (RSA steps)](https://dev.mysql.com/blog-archive/preparing-your-community-connector-for-mysql-8-part-2-sha256/)
- [adr-m7-caching-sha2.md](../../../en/specs/adr-m7-caching-sha2.md) (RSA deferral removed)
