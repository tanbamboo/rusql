# M19 — SELECT LIMIT OFFSET

**Issue #42**

## Problem

Pagination APIs use `LIMIT` + `OFFSET` together (`page 2` = skip 10, take 10). M16 `LIMIT` alone cannot express cursor/page semantics clients send to MySQL.

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| `skip` then `take` after sort | Correct MySQL order | Materializes full sorted set |
| Keyset pagination | Efficient at scale | Different SQL surface |
| OFFSET in storage scan | Early termination | Complex with heap MVP |

## Decision

- Read `Query.offset` from sqlparser.
- Pipeline: filter → project → **ORDER BY** → **OFFSET skip** → **LIMIT take**.
- Integer literal offset only.

## Internals

```rust
apply_pagination(rows, offset, limit) // skip then take
```

Works with `information_schema` scans via shared `finish_rows_query`.

## Trade-offs

Large offsets are O(n) — acceptable for MVP. No `OFFSET` without `ORDER BY` guarantee (MySQL allows it; insertion order is undefined).

## Further reading

- MySQL 8.0: [LIMIT clause](https://dev.mysql.com/doc/refman/8.0/en/select.html)
- *Use The Index, Luke* — offset pagination vs keyset

## Harness lesson

> Combine `ORDER BY id LIMIT 1 OFFSET 1` in compat JSON — catches ordering/pagination interaction bugs.
