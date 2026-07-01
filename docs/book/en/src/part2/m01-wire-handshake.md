# M1 — Wire protocol handshake

**Merged**: PR #6

## Problem

MySQL clients refuse to talk to a server that cannot complete **protocol version 10 handshake** and capability negotiation.

## Design choices

- **Tokio** TCP listener in `rusql-server`
- Initial handshake packet with server version string and charset
- OK/ERR packet framing in `rusql-protocol`
- Auth plugin name advertised early (evolved in M6/M7)

## Trade-offs

We implemented **enough** handshake for clients to connect and send `COM_QUERY`, not full SSL, compression, or connection attrs.

## What we deferred

Real password verification (M6), `caching_sha2_password` (M7), prepared statements (M11).

## Harness lesson

> Wire integration tests with a **minimal test client** in `test_support` paid off for every later milestone — invest once in M1.

## See also

- [adr-auth-mvp.md](../../../en/specs/adr-auth-mvp.md) (auth evolution)
