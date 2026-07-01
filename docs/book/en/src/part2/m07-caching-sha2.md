# M7 — caching_sha2_password

**Merged**: PR #18 · Closes #7

## Problem

MySQL 8 defaults to **`caching_sha2_password`**. Clients without `--default-auth=mysql_native_password` failed against rusql.

## Design choices

- Advertise `caching_sha2_password` in handshake
- Implement **fast-path** verify (SHA256) aligned with common clients
- Keep `mysql_native_password` fallback ([adr-m7](../../../en/specs/adr-m7-caching-sha2.md))

## Trade-offs

**RSA full auth exchange deferred** — documented negative constraint; enough for local dev and CI clients.

## Harness lesson

> State **negative constraints** explicitly (“no RSA yet”) so agents do not gold-plate auth in one PR.

## See also

- [adr-m7-caching-sha2.md](../../../en/specs/adr-m7-caching-sha2.md)
