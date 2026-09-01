## Goal

Implement MySQL wire protocol initial handshake (server → client Initial Handshake, client → server Handshake Response, server → OK packet).

## Acceptance Criteria

- [ ] `rusql-protocol` encodes/decodes Initial Handshake packet
- [ ] Server accepts TCP connection and completes handshake sequence
- [ ] Integration test simulates client handshake without external `mysql` CLI
- [ ] Protocol errors use `rusql_i18n::messages::*`

## File Boundaries

`crates/rusql-protocol/**`, `crates/rusql-server/**`, `crates/rusql-i18n/locales/*`

## Negative Constraints

- No full SQL execution in this issue
- No `caching_sha2_password` until #3 is resolved
