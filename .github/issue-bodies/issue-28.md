## Problem

How do we **demonstrate Harness Engineering on a serious software project** — not as a blog post, but as a durable narrative that tracks real merges, real trade-offs, and real sensors?

## Feasibility (brainstorm)

| Idea | Verdict | Notes |
|------|---------|-------|
| One chapter per milestone/PR | **Yes** | M0–M13 map cleanly to vertical slices; keeps chapters short and shippable |
| GitBook-style navigation | **Yes** | Use **[mdBook](https://rust-lang.github.io/mdBook/)** (SUMMARY.md, static HTML). Same workflow as GitBook; optional publish to GitBook.com later |
| English + zh-CN | **Yes** | Parallel trees under `docs/book/en` and `docs/book/zh-CN`; `check-book.mjs` enforces chapter parity |
| Database design focus | **Yes** | Chapters lead with problem → choice → trade-off; link to ADRs/specs instead of pasting code |
| Harness tips | **Yes** | Dedicated Part I + per-chapter “Harness lesson” callouts |
| Minimal code | **Yes** | At most one small snippet per chapter when it clarifies a protocol or storage decision |

**Risks & mitigations**

- *Doc drift* — Book updates are part of milestone ship checklist when the milestone is user-visible or changes harness story.
- *Duplicating user-guide* — Book = *why*; user-guide = *how to test today*.
- *CI without mdbook* — `build-book.mjs` skips gracefully; `check-book.mjs` validates structure without the binary.

## Optimized scope (MVP)

### In scope

- [x] mdBook project (en + zh-CN) under `docs/book/`
- [x] Introduction + Harness Engineering part (3 chapters)
- [x] Milestone chapters M0–M13 (problem, design, trade-off, harness lesson)
- [x] Appendix: metrics snapshot from harness retrospective
- [x] `scripts/check-book.mjs` + `scripts/build-book.mjs`
- [x] README link to book

### Out of scope (follow-ups)

- GitBook.com SaaS hosting / custom domain
- Auto-generate chapters from git log
- Book PDF release pipeline

## Acceptance criteria

- [ ] `node scripts/check-book.mjs` passes on CI
- [ ] `mdbook build` succeeds for en and zh-CN when mdbook is installed
- [ ] README links to `docs/book/README.md`
- [ ] CHANGELOG documents the book

## File boundaries

- `docs/book/**`
- `scripts/check-book.mjs`, `scripts/build-book.mjs`
- `README.md`, `CHANGELOG.md`, `profiles/rust/sensors.yaml` (optional sensor)

## Living process

After each milestone merge, add or refresh that milestone’s chapter (both locales) in the same PR or a fast-follow docs PR.
