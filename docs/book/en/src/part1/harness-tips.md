# Tips for agent-native projects

Practical patterns that worked on rusql (M0–M13).

## 1. Milestone = PR = issue

One vertical slice per merge. `main` always runs; users always have an up-to-date guide.

## 2. Decision tables in issues

When humans might care (auth mode, parser choice), put options in the issue **before** coding. Async override via issue comment.

## 3. ADR before dependent milestones

Parser (#4) and auth (#3) ADRs unblocked M2–M7 without re-debate.

## 4. Compat fixtures are your best feedback dollar

JSON driving real wire tests caught regressions that unit tests missed. Add a fixture step when SQL becomes user-visible.

## 5. File boundaries in every P0 issue

Stops agents from “helpfully” refactoring unrelated crates.

## 6. Eliminate continue? prompts

Autonomy rule: pick next `agent-ready` issue or create one from roadmap. Human time is for `needs-human` only.

## 7. Changelog + release notes every PR

Developers read CHANGELOG; users read release-notes. Sensor enforces structure (#23).

## 8. Accept CI rustfmt as tax

~12.5% branch fix rate was formatting — run `cargo fmt` before push.

## 9. Bilingual parity sensors

`doc-parity.mjs` and `check-book.mjs` prevent en/zh drift.

## 10. Retrospective metrics

Periodic harness reports (see [appendix](../appendix/metrics.md)) guide **feedback** investments, not more process for its own sake.

## Harness lesson

> Optimize for **time-to-green CI** and **time-to-user-verifiable docs**, not line count per session.
