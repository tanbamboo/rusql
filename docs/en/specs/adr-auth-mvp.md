# ADR: MVP Authentication Plugin

**Status**: Accepted (via [#3](https://github.com/tanbamboo/rusql/issues/3#issuecomment-4840854335))  
**Date**: 2026-06-30

## Decision

**Phase M1**: Support `mysql_native_password` only (Option A).

**Follow-up**: Add `caching_sha2_password` in a dedicated issue after handshake is stable.

## Rationale

- Simpler MVP; handshake already advertises `mysql_native_password`
- MySQL 8.0 default auth can be added without blocking M1/M2
- Current implementation stubs password verification; real native_password hash check is a separate task

## Consequences

- Clients defaulting to `caching_sha2_password` may fail until follow-up issue is done
- Document connection flag: `--default-auth=mysql_native_password` where needed
