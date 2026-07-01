# M14 — SELECT column projection

**Merged**: (pending) · Issue #34

## Problem

ORMs and apps rarely need every column on every query. Returning full rows for `SELECT id FROM users` wastes wire bandwidth and hides projection bugs until production.

## Design choices

- Resolve `SELECT` list against catalog column order
- Project rows **after** scan / index lookup (same code paths as `SELECT *`)
- `Wildcard` (`*`) keeps pass-through behavior

## Trade-offs

Identifiers only — no expressions, `COUNT(*)`, or computed columns yet.

## Harness lesson

> Extend the **existing compat suite** (`basic_dml`) with one projected query — cheapest regression signal for executor changes.

## See also

- [m14-select-projection.md](../../../en/specs/m14-select-projection.md)
