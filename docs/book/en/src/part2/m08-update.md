# M8 — UPDATE

**Merged**: PR #20

## Problem

CRUD without **UPDATE** blocks realistic apps and ORM smoke tests.

## Design choices

- `UPDATE tbl SET col = literal [WHERE col = literal]`
- Reuse delete-filter plumbing for row selection
- WAL record for durable updates

## Trade-offs

Literal assignments only — no expressions, no multi-column SET in one statement beyond parser limits.

## Harness lesson

> After M5, **every DML milestone adds compat steps** — UPDATE was a small JSON diff + green CI.

## See also

- compat `drop_and_delete` suite (UPDATE step)
