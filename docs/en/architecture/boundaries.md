# Crate Boundaries

## Dependency Rules

```
rusql-server  → protocol, core, sql, executor, planner, storage, i18n
rusql-cli     → core, i18n
rusql-executor → core, storage, sql
rusql-planner → sql, core
rusql-protocol → i18n
rusql-sql     → (sqlparser only)
rusql-core    → i18n
rusql-storage → core
rusql-i18n    → (no internal deps)
```

## Forbidden Dependencies

- `rusql-storage` must not depend on `rusql-protocol` or `rusql-server`
- `rusql-sql` must not depend on `rusql-executor` or `rusql-storage`
- `rusql-i18n` must not depend on any other rusql crate

## Module Ownership (by `area:*` label)

| Label | Crates |
|-------|--------|
| `area:protocol` | rusql-protocol |
| `area:sql` | rusql-sql, rusql-planner, rusql-executor |
| `area:storage` | rusql-storage |
| `area:i18n` | rusql-i18n, locales |
| `area:harness` | AGENTS.md, profiles, .agents, .cursor |
