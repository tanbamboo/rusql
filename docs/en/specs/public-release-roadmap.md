# Public release roadmap

Constitution-aligned path from the current `main` tip to a **MySQL 8.0 community drop-in** public GA. This document does not redefine the ultimate goal; it sequences packaging and Phase Z exits already defined in [mysql-full-parity-roadmap.md](mysql-full-parity-roadmap.md).

**中文**: [public-release-roadmap.md](../../zh-CN/specs/public-release-roadmap.md)

## Release classes

| Class | When allowed | Messaging |
|-------|--------------|-----------|
| **Developer preview** | `main` is CI-green | Experimental binary; **not** a production MySQL drop-in |
| **Public GA (drop-in claim)** | M209 + M210 evidence on `main` | May claim MySQL 8.0 community functional equivalence with linked evidence |
| **Enterprise plugin parity** | Never required | Explicit skip [M188](https://github.com/tanbamboo/rusql/issues/340) |

Public GA means the first GitHub Release that may claim MySQL 8.0 community drop-in use. That gate is **M210**. Earlier tags are developer preview only.

## Binding rules

Every stage follows the project contract ([CONSTITUTION.md](../../../CONSTITUTION.md), [AGENTS.md](../../../AGENTS.md)):

- Spec-first; stay inside issue file boundaries
- Sensors before merge (`cargo fmt` / `clippy -D warnings` / `test` / `harness-validate`; CI mysql-diff / protocol as required)
- User-visible strings via `rusql-i18n` (en-US + zh-CN)
- Bilingual docs; one issue per PR; label only the next sequenced issue `agent-ready`
- No `CONSTITUTION.md` / `.github/CODEOWNERS` edits; no force-push; no `--no-verify`
- HANDOFF must not say the ultimate goal is achieved until M209 and M210 evidence exists

## Current status

- Phases Q–Y complete on `main`; Phase Z M196–M198 complete (GIS subset, FULLTEXT boolean, WAL crash recovery)
- Open Phase Z: M199–M210 ([#352](https://github.com/tanbamboo/rusql/issues/352)–[#363](https://github.com/tanbamboo/rusql/issues/363))
- Client-visible surface remains incomplete; many features are documented MVPs or stubs
- Workspace version stays `0.1.0` until a GA cut after M210

## Stages

```mermaid
flowchart LR
  s0["Stage0 developer preview"]
  s1["Stage1 M199-M206"]
  s2["Stage2 M207-M208"]
  s3["Stage3 M209-M210"]
  s4["Stage4 GA publish"]
  s0 --> s1 --> s2 --> s3 --> s4
```

### Stage 0 — Developer preview (parallel; no drop-in claim)

- README / user-guide keep **not a production drop-in**
- Tag only when CI is green; CHANGELOG / bilingual release-notes describe a preview
- Purpose: downloadable binary for contributors and early testers
- Does **not** complete the ultimate MySQL 8.0 goal

### Stage 1 — Phase Z product surface (M199–M206)

| ID | Issue | Focus |
|----|-------|--------|
| M199 | [#352](https://github.com/tanbamboo/rusql/issues/352) | Group Replication / InnoDB Cluster — ADR then subset or documented blocker (no silent skip) |
| M200 | [#353](https://github.com/tanbamboo/rusql/issues/353) | Clone plugin — ADR; implement or dated substitute |
| M201 | [#354](https://github.com/tanbamboo/rusql/issues/354) | UDF `.so` — prefer explicit security skip (M188 pattern) |
| M202 | [#355](https://github.com/tanbamboo/rusql/issues/355) | Component / plugin loader — ADR; documented reject or subset |
| M203 | [#356](https://github.com/tanbamboo/rusql/issues/356) | Window named frames + `RANGE BETWEEN` |
| M204 | [#357](https://github.com/tanbamboo/rusql/issues/357) | Histogram / optimizer stats persist |
| M205 | [#358](https://github.com/tanbamboo/rusql/issues/358) | Hash join / BNL cost |
| M206 | [#359](https://github.com/tanbamboo/rusql/issues/359) | Temp tables `MEMORY` / InnoDB engine clause |

Exit: each closed or explicitly **Out of scope** with ADR + SKIPS / compatibility report rows.

### Stage 2 — Evidence floors (M207–M208)

| ID | Issue | Focus |
|----|-------|--------|
| M207 | [#360](https://github.com/tanbamboo/rusql/issues/360) | Portable mysql-test expansion 100 → 500 |
| M208 | [#361](https://github.com/tanbamboo/rusql/issues/361) | Gap probe becomes CI floor (0 unexpected gaps) |

Exit: harness sensors prove a broader MySQL surface than the curated mysql-diff suite alone.

### Stage 3 — Public GA gate (M209–M210)

| ID | Issue | Focus |
|----|-------|--------|
| M209 | [#362](https://github.com/tanbamboo/rusql/issues/362) | Requirement-by-requirement matrix in `rusql-vs-mysql.md` (en + zh-CN); no in-scope **Missing** |
| M210 | [#363](https://github.com/tanbamboo/rusql/issues/363) | Dump/restore + listed ORM suite + replication + locking green vs Docker `mysql:8.0`; M198 ADR present |

Exit: HANDOFF may say the ultimate goal is achieved; README may drop “not a drop-in” only with evidence links.

### Stage 4 — Publish the GA artifact

- Cut a GA version only if M210 is green (do not ship `1.0.0` on stubs)
- GitHub Release + CHANGELOG + en/zh-CN release-notes
- crates.io / package registries only with human approval for publish secrets and naming

## Operating loop

For each issue: [spec-to-ship](../workflows/spec-to-ship.md) → implementer within file boundaries → sensors → bilingual docs → PR → merge when green → HANDOFF → label only the next sequenced issue `agent-ready`.

Do not mass-label Phase Z. Do not mark the ultimate goal complete until M209 and M210 evidence exists on `main`.

## Immediate next product issue

[#352 M199](https://github.com/tanbamboo/rusql/issues/352) Group Replication: accept bilingual ADR, then implement a membership subset or leave an explicit open blocker that M210 tracks.
