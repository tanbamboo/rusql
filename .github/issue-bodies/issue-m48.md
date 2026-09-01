## Goal

Support row-level triggers on INSERT, UPDATE, DELETE.

## Category

Phase J — Stored programs.

## Depends on

- M47 stored procedures (shared execution context)

## Acceptance Criteria

- [ ] `CREATE TRIGGER tr BEFORE INSERT ON t FOR EACH ROW …`
- [ ] `AFTER UPDATE` / `AFTER DELETE` minimum
- [ ] `NEW` / `OLD` row references in trigger body (single-table)
- [ ] `DROP TRIGGER`
- [ ] `information_schema.TRIGGERS` stub
- [ ] Portable negative/positive cases in compat harness

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-core/**`, `crates/rusql-storage/**`

## Negative Constraints

- No `BEFORE UPDATE` cascading trigger chains beyond depth 1
- No trigger on views
