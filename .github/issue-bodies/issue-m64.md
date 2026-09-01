## Goal

Fire AFTER UPDATE and AFTER DELETE row triggers with OLD/NEW references (completes M48 beyond BEFORE INSERT MVP).

## Category

Phase J — Stored programs (post-M61).

## Depends on

- M48 trigger MVP (BEFORE INSERT)

## Acceptance Criteria

- [ ] `CREATE TRIGGER … AFTER UPDATE ON t FOR EACH ROW …` executes on matching UPDATE rows
- [ ] `CREATE TRIGGER … AFTER DELETE ON t FOR EACH ROW …` executes on matching DELETE rows
- [ ] Trigger body supports side-effect DML with `OLD.col` / `NEW.col` substitution
- [ ] `information_schema.TRIGGERS` reflects timing/event
- [ ] Unit tests for AFTER UPDATE audit insert and AFTER DELETE audit insert

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-core/**`

## Negative Constraints

- No cascading trigger chains beyond depth 1
- No BEFORE UPDATE in this issue
