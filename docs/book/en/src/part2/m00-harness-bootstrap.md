# M0 — Harness bootstrap

**Merged**: bootstrap commit, issue #1

## Problem

Start a MySQL-compatible database in Rust with **AI-native workflow** from day zero — not bolt harness on later.

## Design choices

| Topic | Choice | Alternative rejected |
|-------|--------|----------------------|
| Repo layout | Cargo workspace + layered crates | Monolith binary |
| Governance | CONSTITUTION, AGENTS, issue-loop rule | Ad-hoc README only |
| Profile | `profiles/rust/sensors.yaml` | Copy-paste CI commands |
| i18n | `rusql-i18n` crate early | English-only strings |

## Trade-offs

Up-front harness files feel heavy for an empty repo — but they define **how** every later milestone ships.

## What we deferred

Any SQL beyond hello-world; storage beyond in-memory sketches.

## Harness lesson

> Bootstrap **process artifacts** (sensors, issue templates, HANDOFF) in M0 so agents never ask “where do tests live?”

## See also

- [Architecture overview](../../../en/architecture/overview.md)
- [spec-to-ship workflow](../../../en/workflows/spec-to-ship.md)
