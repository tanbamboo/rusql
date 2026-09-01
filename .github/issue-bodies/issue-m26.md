## Goal

Full `caching_sha2_password` RSA key exchange (issue #7 follow-up).

## Depends on

- M7 caching_sha2 fast path

## Acceptance Criteria

- [ ] Clients requiring RSA auth succeed
- [ ] Document negative constraints removed

## File Boundaries

- crates/rusql-protocol/**
