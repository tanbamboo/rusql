# Further reading

## In this repository

- [User guide](../../../en/user-guide.md) — verify features on `main`
- [Release notes](../../../en/release-notes.md) — per-milestone summaries
- [CHANGELOG](../../../../CHANGELOG.md)
- [Harness retrospective report](../../../en/reports/harness-retrospective-2026-06-30.md)
- [Replication ADR (future)](../../../en/specs/adr-replication.md)

## External

- [Harness Engineering — Martin Fowler](https://martinfowler.com/articles/harness-engineering.html)
- [MySQL 8.0 Reference Manual](https://dev.mysql.com/doc/refman/8.0/en/)
- [mdBook documentation](https://rust-lang.github.io/mdBook/)
- [sqlparser-rs](https://github.com/sqlparser-rs/sqlparser-rs)

## Contributing a chapter

When milestone M14+ merges:

1. Add a row to both `SUMMARY.md` files
2. Write en + zh-CN chapters (problem → choice → trade-off → harness lesson)
3. Run `node scripts/check-book.mjs`
4. Link from release-notes if the milestone is user-visible
