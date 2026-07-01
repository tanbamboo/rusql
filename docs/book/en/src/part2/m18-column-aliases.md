# M18 — SELECT column aliases

**Issue #41**

## Problem

APIs and ORMs expose result columns by name. `SELECT id FROM users` forces clients to know physical column names; production queries use `AS` for stable DTO field names (`user_id`, `display_name`). Wire protocol column metadata must reflect aliases or drivers mis-map rows.

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| Alias in `resolve_projection` only | Reuses M14 pipeline | `ORDER BY` must resolve alias vs base name |
| Separate alias map post-project | Clear separation | Duplicate column resolution |
| Ignore aliases | — | Breaks MySQL compat |

## Decision

- M14 already parsed `SelectItem::ExprWithAlias`; M18 **documents and tests** the contract.
- Output header = alias when present, else base column name.
- `ORDER BY` still resolves against **output** column names (M17).

## Internals

```rust
names.push(alias.unwrap_or(col));
```

Projection indices still point at underlying table columns; only metadata changes.

## Trade-offs

`SELECT expr AS alias` for non-identifier expressions not supported yet. Implicit alias (`SELECT id user_id` without `AS`) depends on sqlparser MySQL dialect — not required for M18 acceptance.

## Further reading

- MySQL 8.0 Reference: [SELECT aliases](https://dev.mysql.com/doc/refman/8.0/en/select.html)
- ODBC/JDBC result-set column label semantics

## Harness lesson

> Alias coverage is one JSON step in `basic_dml` — asserts protocol column names, not just cell values.
