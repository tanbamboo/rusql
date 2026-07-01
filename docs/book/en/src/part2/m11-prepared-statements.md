# M11 — Prepared statements

**Merged**: PR #27 · Issue #26 · [spec](../../../en/specs/m11-stmt-prepare.md)

## Problem

Drivers use **`COM_STMT_PREPARE` / `EXECUTE` / `CLOSE`** with `?` placeholders — `COM_QUERY` only is insufficient.

## Design choices

- Binary stmt OK packets in `rusql-protocol`
- Per-connection prepared statement store
- Bind `?` → literal substitution before parse (MVP simplicity)

## Trade-offs

No binary resultset, no `COM_STMT_FETCH`, no long-data — documented in spec boundaries.

## Harness lesson

> Protocol milestones need **packet-level unit tests** *and* wire tests — stmt id started at 1 after bug found in review.

## See also

- [m11-stmt-prepare.md](../../../en/specs/m11-stmt-prepare.md)
