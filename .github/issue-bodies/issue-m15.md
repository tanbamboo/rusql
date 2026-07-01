## Goal

`USE rusql` (and `USE DATABASE rusql`) sets session default database; unknown DB errors.

## Acceptance Criteria

- [ ] `USE rusql` returns OK
- [ ] Unknown database name returns error
- [ ] Wire/executor tests + CHANGELOG + release notes + book chapter

## File Boundaries

- crates/rusql-core/** (session field)
- crates/rusql-executor/**
- docs/**
