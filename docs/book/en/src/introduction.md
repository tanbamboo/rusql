# Introduction

This book records how **rusql** — a MySQL 8.0–compatible database in Rust — was built milestone by milestone with **AI agents** and **Harness Engineering**.

It is not a Rust tutorial or a MySQL internals manual. It answers:

1. **What problem** each milestone solved on the path to compatibility
2. **Which design** we chose and **what we gave up**
3. **How the harness** (issues, sensors, docs, fixtures) made autonomous delivery safe

## Who this is for

- Engineers evaluating Harness Engineering for production codebases
- Contributors who want the *story* behind `main`, not only the API
- Readers learning how to scope database MVPs without boiling the ocean

## How to read

- **Part I** — cross-cutting harness ideas (read first if you are new to the Martin Fowler harness article)
- **Part II** — one chapter per merged milestone (M0–M13); chapters are independent after Part I
- **Appendix** — quantitative snapshot from our first retrospective

## What we deliberately omit

Long code listings live in the repository and in [specs](../../en/specs/). The [user guide](../../en/user-guide.md) tells you how to run tests today. This book explains *why* those features exist.

## Status

The book is a **living document**: when milestone M14+ lands on `main`, its chapter should be added or updated in the same spirit — problem, choice, trade-off, harness lesson.
