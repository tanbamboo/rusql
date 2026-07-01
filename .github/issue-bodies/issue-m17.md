## Goal

`ORDER BY col [ASC|DESC]` on table `SELECT`.

## Depends on

- M14 SELECT column projection (merged #35)

## Acceptance Criteria

- [ ] `SELECT * FROM t ORDER BY id`
- [ ] `SELECT name FROM t ORDER BY name DESC`
- [ ] Compat + executor tests + docs + book chapter

## File Boundaries

- crates/rusql-executor/**
- crates/rusql-server/compat/basic.json
- docs/**
