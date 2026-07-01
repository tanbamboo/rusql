# Introduction

This book documents **rusql** — a MySQL 8.0–compatible relational database implemented in Rust — built through **Harness Engineering** and autonomous AI agents. It is written for **professional software engineers**: you may not have designed a storage engine before, but you should leave with a clear mental model of protocol, catalog, execution, durability, and how to ship such a system incrementally.

## What this book is

A **design narrative** tied to real merges on `main`. Each milestone chapter answers:

1. **Problem** — What client or compatibility gap forced this slice?
2. **Design space** — What alternatives existed?
3. **Decision** — What we built, with trade-offs explicit.
4. **Internals (light)** — Enough implementation detail to reason about bugs, not a line-by-line walkthrough.
5. **Harness lesson** — How sensors, issues, and fixtures made the slice safe to merge.

## What this book is not

- A Rust tutorial (see the Rust Book).
- A replacement for Oracle’s MySQL Reference Manual.
- A dump of source code (see `crates/` and `docs/en/specs/`).

Operational verification lives in the [user guide](../../en/user-guide.md). This book explains **why** those features exist.

## Audience

| Reader | Takeaway |
|--------|----------|
| Backend engineer | How MySQL-shaped systems layer protocol, SQL, and storage |
| Harness / agent advocate | Feedforward + feedback patterns that survived 16+ milestones |
| Contributor | Context behind ADRs, compat JSON, and crate boundaries |

## Reading order

1. **[MySQL compatibility landscape](part0/mysql-landscape.md)** — vocabulary and layers
2. **Part I — Harness Engineering** — process that makes agent delivery work
3. **Part II — Milestones** — M0–M16+ in merge order (chapters cross-link but stand alone)
4. **[Bibliography](appendix/bibliography.md)** — papers and manuals cited throughout

## Depth standard (2026 revision)

Early drafts were intentionally short. After reader feedback ([#28](https://github.com/tanbamboo/rusql/issues/28)), chapters were expanded with:

- Richer problem statements (who breaks, how we noticed)
- Design alternatives we rejected
- Pointers to classic literature (ARIES, B+Trees, isolation)
- Honest gaps vs production MySQL

## Living document

New milestones add chapters in English and Simplified Chinese. The [compat roadmap](../../en/specs/mysql-compat-roadmap.md) lists planned work M17–M35.
