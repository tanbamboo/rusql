# M15: USE database

## Goal

Session default database via `USE rusql` for MySQL client compatibility.

## Acceptance criteria

- [x] `USE rusql` returns OK
- [x] Unknown database errors
- [x] `session.database` drives `information_schema.TABLE_SCHEMA`
- [x] Tests + docs

## Boundaries

- Single logical database (`rusql`) only
- No `USE ROLE` / warehouse variants

## Decisions

| Topic | Choice |
|-------|--------|
| Storage | `Session.database` string field |
| Multi-db | Reject unknown names |
