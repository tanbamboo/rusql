# rusql Book

**Building a MySQL-like Database with AI and Harness Engineering**

Narrative documentation: *why* we built each milestone, what we traded off, and how Harness Engineering kept `main` deliverable. For hands-on testing see [user-guide](../en/user-guide.md).

| Edition | Build | Source |
|---------|-------|--------|
| English | `mdbook build` in `docs/book/en` | [en/src/SUMMARY.md](en/src/SUMMARY.md) |
| 简体中文 | `mdbook build` in `docs/book/zh-CN` | [zh-CN/src/SUMMARY.md](zh-CN/src/SUMMARY.md) |

## Prerequisites

Install [mdBook](https://rust-lang.github.io/mdBook/):

```bash
cargo install mdbook
```

## Build both editions

```bash
node scripts/build-book.mjs
```

Output: `book-output/en/` and `book-output/zh-CN/` (gitignored).

## Validate structure (CI)

```bash
node scripts/check-book.mjs
```

Checks chapter file existence and en/zh parity without requiring mdbook.

## Philosophy

- **One chapter per milestone** — aligned with merged PRs on `main`
- **Design over code** — snippets only when they clarify protocol or storage choices
- **Harness thread** — each chapter ends with a concrete lesson for agent-native development
