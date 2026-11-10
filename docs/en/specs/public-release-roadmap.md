# Public release roadmap

Constitution-aligned path from the current `main` tip to a **MySQL 8.0 community drop-in** public GA. This document does not redefine the ultimate goal; it sequences packaging and Phase Z exits already defined in [mysql-full-parity-roadmap.md](mysql-full-parity-roadmap.md).

**中文**: [public-release-roadmap.md](../../zh-CN/specs/public-release-roadmap.md)

**Snapshot date**: 2026-11-10

## Release classes

| Class | When allowed | Messaging |
|-------|--------------|-----------|
| **Developer preview** | `main` is CI-green | Experimental binary; **not** a production MySQL drop-in |
| **Public GA (drop-in claim)** | M209 + M210 evidence on `main` | May claim MySQL 8.0 community functional equivalence with linked evidence |
| **Enterprise plugin / UDF `.so` parity** | Never required | Explicit skips [M188](https://github.com/tanbamboo/rusql/issues/340), [M201](https://github.com/tanbamboo/rusql/issues/354) |

Public GA means the first GitHub Release that may claim MySQL 8.0 community drop-in use. That gate is **M210**. Earlier tags are developer preview only (`v0.1.0-preview` is Stage 0).

## Binding rules

Every stage follows the project contract ([CONSTITUTION.md](../../../CONSTITUTION.md), [AGENTS.md](../../../AGENTS.md)):

- Spec-first; stay inside issue file boundaries
- Sensors before merge (`cargo fmt` / `clippy -D warnings` / `test` / `harness-validate`; CI mysql-diff / protocol as required)
- User-visible strings via `rusql-i18n` (en-US + zh-CN)
- Bilingual docs; one issue per PR; label only the next sequenced issue `agent-ready`
- No `CONSTITUTION.md` / `.github/CODEOWNERS` edits; no force-push; no `--no-verify`
- HANDOFF must not say the ultimate goal is achieved until M209 and M210 evidence exists

## Current status (evidence)

```mermaid
flowchart LR
  doneThru["Done: Q-Y plus Z M196-M201"]
  stage1rem["Stage1 rem: M202-M206"]
  stage2["Stage2 M207-M208"]
  stage3["Stage3 M209-M210"]
  ga["Public GA claim"]
  doneThru --> stage1rem --> stage2 --> stage3 --> ga
```

| Layer | Status |
|-------|--------|
| CI / tip | Green on `main` through M201 ([PR #461](https://github.com/tanbamboo/rusql/pull/461)); tip includes M199–M201 |
| Stages Q–Y | Complete |
| Phase Z done | M196 GIS, M197 FULLTEXT, M198 crash recovery, M199 GR membership stub, M200 local CLONE, M201 UDF `.so` skip |
| Stage 0 | Done: [v0.1.0-preview](https://github.com/tanbamboo/rusql/releases/tag/v0.1.0-preview) prerelease; README/user-guide mark **developer preview** / **not a drop-in** |
| Open Phase Z | M202–M210 ([#355](https://github.com/tanbamboo/rusql/issues/355)–[#363](https://github.com/tanbamboo/rusql/issues/363)) — **9 issues** |
| Next `agent-ready` | [#355 M202](https://github.com/tanbamboo/rusql/issues/355) (also `needs-human`, ADR-gated) |
| Workspace version | `0.1.0` until a GA cut after M210 |
| Verdict today | Usable for prototypes and contributing — **not** a public MySQL replacement |

Many shipped features remain documented MVPs or stubs (GR membership without XCom, local clone not plugin-loaded, stub `performance_schema` / status / InnoDB catalogs, async replication MVP). M209 must classify each as Works / Stub / Out of scope — **Missing** is forbidden for in-scope community rows at close.

## Gap inventory to public GA

### A. Stage 1 remainder — product surface (M202–M206)

| ID | Issue | Pri | Gap |
|----|-------|-----|-----|
| M202 | [#355](https://github.com/tanbamboo/rusql/issues/355) | P3 | Component / plugin loader — ADR then documented `INSTALL PLUGIN` reject or rusql-native subset (no silent OK; no Oracle EE plugins) |
| M203 | [#356](https://github.com/tanbamboo/rusql/issues/356) | P2 | Named `WINDOW` + `RANGE BETWEEN` (after M129 `ROWS`) |
| M204 | [#357](https://github.com/tanbamboo/rusql/issues/357) | P2 | Histogram / optimizer stats persist |
| M205 | [#358](https://github.com/tanbamboo/rusql/issues/358) | P2 | Hash join / BNL cost in planner |
| M206 | [#359](https://github.com/tanbamboo/rusql/issues/359) | P2 | Temp tables honor `MEMORY` / InnoDB engine clause |

Exit: each closed or explicitly **Out of scope** with ADR + SKIPS / compatibility report rows. Prefer M202 as documented reject if AC allows (same discipline as M201).

### B. Stage 2 — harness floors (M207–M208) — required before M210 evidence

| ID | Issue | Pri | Gap |
|----|-------|-----|-----|
| M207 | [#360](https://github.com/tanbamboo/rusql/issues/360) | P1 | Portable mysql-test 100 → 500 |
| M208 | [#361](https://github.com/tanbamboo/rusql/issues/361) | P1 | Gap probe CI floor (0 unexpected gaps; allowfile for known) |

### C. Stage 3 — definition of done (M209–M210) — P0

| ID | Issue | Pri | Gap |
|----|-------|-----|-----|
| M209 | [#362](https://github.com/tanbamboo/rusql/issues/362) | P0 | Full requirement matrix in `rusql-vs-mysql.md` (en + zh-CN); no in-scope **Missing** |
| M210 | [#363](https://github.com/tanbamboo/rusql/issues/363) | P0 | Dump/restore + listed ORM suite + replication + locking green vs Docker `mysql:8.0`; sensors green; M198 ADR already on `main` |

### D. Cross-cutting release blockers

- Refresh `rusql-vs-mysql.md` (+ zh-CN) so the production scorecard matches post-M201 reality (finished under M209)
- Close or ADR leftover MVP gaps that would become **Missing** under M209 (continuous replica SQL thread depth, full gap-lock matrix, full `performance_schema`, full InnoDB Cluster, etc.) — no silent skip
- Packaging: semver bump only after M210; crates.io only with human approval for naming and publish secrets

## Staged plan (remaining)

### Stage 0 — Developer preview — **DONE**

Preview tag and honest messaging landed. Do not treat Stage 0 as GA.

### Stage 1 — Finish product surface (M202–M206)

1. **M202**: accept bilingual ADR (`needs-human`), then documented `INSTALL PLUGIN` reject or minimal rusql-native loader story — no fake OK.
2. **M203–M206**: feature PRs with mysql-diff/protocol coverage, i18n, bilingual docs, HANDOFF; label only the next issue `agent-ready`.

### Stage 2 — Evidence floors (M207–M208)

Raise portable mysql-test to 500; promote gap probe to failing CI on unexpected gaps.

### Stage 3 — Public GA gate (M209–M210)

Matrix with no in-scope Missing; production drop-in artifacts vs Docker MySQL 8.0. Only then may HANDOFF say the ultimate goal is achieved.

### Stage 4 — Publish the GA artifact

Cut GA version (e.g. `1.0.0`) only if M210 is green. GitHub Release + CHANGELOG + en/zh-CN release-notes. Optional crates.io with human approval.

## Operating loop

For each issue: [spec-to-ship](../workflows/spec-to-ship.md) → implementer within file boundaries → sensors → bilingual docs → PR → merge when green → HANDOFF → label only the next sequenced issue `agent-ready`.

Do not mass-label Phase Z. Do not mark the ultimate goal complete until M209 and M210 evidence exists on `main`.

## Immediate next product issue

[#355 M202](https://github.com/tanbamboo/rusql/issues/355) Component / plugin loader: accept bilingual ADR, then documented reject or subset — clear `needs-human` only when the ADR is accepted in-repo.
