## Problem

How do we **demonstrate Harness Engineering on a serious software project** — as a **professional** book for engineers building or evaluating MySQL-compatible systems?

## Reader feedback (2026-07-01)

> 1. Every chapter is TOO simple — need more problem/design detail, some internals OK, academic papers welcome.
> 2. Write like a professional book for professional engineers, even if not deep in database internals.

## Depth standard (revised acceptance)

### Per chapter (en + zh-CN)

- [ ] **Problem** — who breaks, how we discovered the gap (≥2 paragraphs)
- [ ] **Design space** — table of alternatives considered
- [ ] **Decision** — what shipped + explicit trade-offs
- [ ] **Internals** — conceptual (no large code dumps); optional diagram
- [ ] **Further reading** — 2+ citations (papers, MySQL manual, bibliography)
- [ ] **Harness lesson** — one actionable sensor/process takeaway

### Book-wide

- [x] Part 0: MySQL compatibility landscape
- [x] Appendix: Bibliography
- [x] Expanded exemplar chapters (M3 WAL, M4 indexes, introduction)
- [ ] Roll depth pass to remaining M0–M16 chapters (incremental PRs OK)
- [x] Roadmap doc linking milestones to MySQL 8 goal

### Tooling

- [x] `node scripts/check-book.mjs` on CI
- [x] `mdbook build` when mdbook installed
- [x] Linked from README

## Out of scope

- GitBook.com hosting
- Auto-generated chapters from git log only

## File boundaries

- `docs/book/**`
- `docs/en/specs/mysql-compat-roadmap.md`
- `scripts/check-book.mjs`, `scripts/create-roadmap-issues.mjs`

## Living process

Code milestones M17+ tracked in roadmap issues #40–#58. Book chapters updated when design story changes or on dedicated #28 depth passes.
