## Goal

`COM_STMT_PREPARE` / `COM_STMT_EXECUTE` / `COM_STMT_CLOSE` for parameterless and `?` placeholder SQL (text params).

## Acceptance Criteria

- [ ] Prepare returns statement id + param/column counts
- [ ] Execute runs prepared SELECT/INSERT with optional bound params
- [ ] Close releases statement
- [ ] Wire integration test passes
- [ ] CHANGELOG + release notes + user guide updated

## File Boundaries

- crates/rusql-protocol/**
- crates/rusql-server/**
- docs/**

## Negative Constraints

- No COM_STMT_FETCH, no binary resultset
- No long-data (0x18) in this milestone
