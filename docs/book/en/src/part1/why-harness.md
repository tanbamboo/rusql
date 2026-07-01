# Why Harness Engineering?

[Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) treats the **environment around the coder** — human or AI — as the product. The goal is not smarter prompts; it is **reliable feedforward and feedback** so that small units of work merge quickly without breaking `main`.

## Feedforward vs feedback

| Direction | Question | rusql examples |
|-----------|----------|-----------------|
| **Feedforward** | What should be built next, and under what constraints? | `agent-ready` issues, ADRs, file boundaries, HANDOFF |
| **Feedback** | Did the change actually work? | `cargo test`, clippy, compat JSON fixtures, CI |

A serious project fails when feedforward is vague (“make it like MySQL”) or feedback is slow (manual QA only). rusql optimizes both: **one milestone → one issue → one PR**, each gated by sensors.

## Why not “just use Copilot”?

Ad-hoc AI coding without harness tends to:

- Expand scope across layers in a single diff
- Skip user-testable documentation
- Re-litigate decisions (auth plugin, SQL parser) every session

Harness Engineering makes **decisions durable** (ADRs), **scope bounded** (issue bodies), and **quality measurable** (first-pass CI rate, compat fixtures).

## Why a database is a good harness demo

Databases are unforgiving: wire bytes, SQL semantics, and persistence must align. Users can connect with a real `mysql` client. That makes rusql a **credible** harness case study — not a todo app — while still shipping vertical slices.

## Harness lesson

> Pick a domain where **external clients** provide free feedback (MySQL wire protocol), then invest in **executable specs** (compat JSON) early.
