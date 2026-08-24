## Goal

Handle connection identity switch and reset without reconnect (MySQL 8.0 client compatibility).

## Category

Phase L — Wire protocol.

## Depends on

- M7 auth, M15 USE database

## Acceptance Criteria

- [ ] `COM_CHANGE_USER` re-authenticates and switches default schema
- [ ] `COM_RESET_CONNECTION` clears session state (prepared stmts, temp vars stub)
- [ ] Official `mysql` client reconnect flows pass smoke test
- [ ] Unit tests in `rusql-protocol` for packet encode/decode

## File Boundaries

- `crates/rusql-protocol/**`, `crates/rusql-server/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No SSL/TLS renegotiation in this milestone
