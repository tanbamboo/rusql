# Bibliography and further reading

Curated references cited across this book. URLs are stable entry points; prefer the paper PDF when available.

## Relational model & query processing

- Codd, E. F. (1970). *A Relational Model of Data for Large Shared Data Banks.* Communications of the ACM.
- Garcia-Molina, H., Ullman, J., Widom, J. (2008). *Database Systems: The Complete Book.* — query optimization, transactions (textbook).
- CMU 15-445/645 lecture notes (Ramakrishnan / Mozafari) — [https://15445.courses.cs.cmu.edu/](https://15445.courses.cs.cmu.edu/)

## Recovery & logging

- Mohan, C., Haderle, D., Lindsay, B., Pirahesh, H., Schwarz, P. (1992). *ARIES: A Transaction Recovery Method Supporting Fine-Granularity Locking and Partial Rollbacks Using Write-Ahead Logging.* ACM TODS. — WAL design, redo/undo (rusql M3 skeleton, M31 target).
- Gray, J., Reuter, A. (1993). *Transaction Processing: Concepts and Techniques.*

## Indexing & storage

- Comer, D. (1979). *The Ubiquitous B-Tree.* ACM Computing Surveys. — M4 secondary indexes.
- Lehman, P., Yao, F. (1981). *Efficient Locking for Concurrent Operations on B-Trees.* — future concurrency on index pages.

## Transaction isolation

- Berenson, H. et al. (1995). *A Critique of ANSI SQL Isolation Levels.* — M9 connection overlay vs M32 MVCC goals.
- Adya, A. (1999). *Weak Consistency: A Generalized Theory and Optimistic Implementations.* MIT PhD thesis.

## MySQL-specific

- Oracle MySQL 8.0 Reference Manual — [https://dev.mysql.com/doc/refman/8.0/en/](https://dev.mysql.com/doc/refman/8.0/en/)
- Dubois, P. (2005). *MySQL* (Developer's Library) — protocol and admin perspective.
- Socolofsky, T., Kale, C. (1992). *RFC 1180: TCP/IP tutorial* — background for wire debugging.

## Harness & software engineering

- Fowler, M. (2026). *Harness Engineering.* [martinfowler.com/articles/harness-engineering.html](https://martinfowler.com/articles/harness-engineering.html)
- Brooks, F. (1975). *The Mythical Man-Month* — incremental delivery metaphor for milestones.

## Rust systems

- Rust Book — ownership for safe storage code.
- Tokio documentation — async TCP for `rusql-server`.

When a chapter cites “see bibliography”, start with the section above for that milestone’s topic.
