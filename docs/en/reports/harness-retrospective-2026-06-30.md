# Harness Engineering Retrospective — rusql (2026-06-30)

**Scope**: M0 harness bootstrap through M8 UPDATE (MySQL 8.0 compatibility track)  
**Period**: ~2026-06-30 07:06 UTC → 2026-07-01 01:11 UTC (~18 hours calendar, agent-driven)  
**Repository**: [tanbamboo/rusql](https://github.com/tanbamboo/rusql)

---

## 1. Executive summary

rusql was bootstrapped with Harness Engineering (spec-first, sensor-gated, issue-loop) and delivered **nine milestones (M0–M8)** on a single deliverable `main` branch. The harness **feedforward** layer (issues, ADRs, sensors, HANDOFF, user guide) enabled autonomous multi-milestone progress with minimal human blocking. The **feedback** layer (local sensors + CI + wire-protocol tests + JSON compat fixtures) kept **first-pass CI success at ~87.5%** per PR, with failures concentrated in formatting and flaky integration tests rather than logic regressions on `main`.

**Verdict**: Harness design is **fit for purpose** at MVP velocity. Main gaps are differential MySQL compat feedback, post-merge user bug telemetry, and occasional HANDOFF/doc drift.

---

## 2. Harness model (feedforward vs feedback)

### 2.1 Feedforward (information → work)

| Mechanism | Role | Maturity |
|-----------|------|----------|
| GitHub Issues (`agent-ready`, `priority:P0`) | Work queue, scope contract | Strong |
| Issue body template (goal, AC, boundaries) | Spec before code | Good |
| ADRs (`adr-auth-mvp`, `adr-sql-parser`, M6/M7) | Record decisions, reduce re-litigation | Good |
| `HANDOFF.md` | Cross-session state | Moderate (sometimes stale) |
| `docs/en/user-guide.md` + zh-CN mirror | User-testable contract | Good (from M3+) |
| `README` milestone table | Roadmap visibility | Strong |
| `.cursor/rules/issue-loop.mdc` | Agent autonomy + doc gate on merge | Strong |
| `profiles/rust/sensors.yaml` | Expected quality bar | Strong |
| `scripts/check-issue-replies.mjs` | Unblock `needs-human` | Deployed, lightly used |
| Compat JSON fixtures (`compat/basic.json`) | Executable spec for SQL subset | Strong (M5+) |

**Feedforward strength**: One milestone → one issue → one PR is consistently enforced. Decisions (#3 auth, #4 parser) were captured as ADRs before dependent milestones shipped.

**Feedforward gaps**:

- Formal `docs/specs/<feature>.md` per milestone is inconsistent (issue body + ADR often substitute).
- zh-CN user-guide formatting drifted (extra blank lines) — i18n mirror not sensor-gated.
- Issue #16 included an explicit decision table (good); earlier issues were thinner on negative constraints.

### 2.2 Feedback (work → correction)

| Mechanism | Role | Maturity |
|-----------|------|----------|
| `cargo fmt --check` | Format gate | Strong (caught cross-OS drift) |
| `cargo clippy -D warnings` | Static analysis | Strong |
| `cargo test` | Unit + integration | Strong (~40+ tests) |
| `harness-validate.mjs` | Repo structure | Strong |
| GitHub Actions CI | Remote sensor loop | Strong |
| Wire integration tests (`rusql-server`) | Protocol + SQL E2E | Strong |
| `compat_suite` JSON runner | Regression on SQL subset | Strong |
| User manual testing | Real MySQL client | Ad hoc (user-driven) |
| Production/staging telemetry | — | Not present |

**Feedback strength**: Every merge to `main` passed CI. M5 compat fixtures created a **closed-loop** between documented features and automated wire tests.

**Feedback gaps**:

- No automated **rusql vs MySQL** differential runner (only rusql self-tests).
- No Bugbot/security gate on every PR in recorded workflow.
- Post-merge defects rely on user filing Issues (none filed yet in this window).

---

## 3. Delivery metrics

### 3.1 Throughput

| Metric | Value |
|--------|-------|
| Calendar span | ~18 hours |
| Milestones completed | M0–M8 (9) |
| Merged PRs | 8 (#6, #9, #11, #13, #15, #17, #18, #20) |
| Commits on `main` (squash) | 12 |
| Closed issues | 11 |
| Open issues (deferred) | 1 (#5 replication ADR) |
| Rust source lines (crates) | ~3,094 |
| Rust test functions | ~40+ |

### 3.2 Task decomposition granularity

| Dimension | Observation | Score |
|-----------|-------------|-------|
| Issue scope | 1 milestone ≈ 1 vertical slice (protocol/storage/sql) | Excellent |
| PR size (median) | ~497 net LOC added per PR | Good |
| PR size (max) | M2 ~589 net LOC | Acceptable |
| Branch lifetime | Minutes to ~1 hour | Excellent |
| Long-lived branches | None observed | Excellent |

**Assessment**: Decomposition matches Harness “small PRs, fast merge” guidance. M6 (auth + DROP + DELETE) was the broadest slice but still single-reviewable.

### 3.3 Spec quality (rubric 1–5)

| Criterion | Score | Notes |
|-----------|-------|-------|
| Testable acceptance criteria | 4 | Present on P0 issues; early M1 thinner |
| File boundaries | 3 | Template exists; not always in issue body |
| Decision documentation | 4 | ADRs + issue #16 decision table |
| User-facing test path | 4 | User guide + compat fixtures from M5 |
| Negative constraints | 3 | Improved over time (M7 defers RSA full-auth) |

**Overall spec quality**: **3.6 / 5** — sufficient for agent autonomy; improve standalone spec files for M9+.

### 3.4 First-pass rate

| Layer | First-pass | Denominator | Rate |
|-------|------------|-------------|------|
| PR CI (rust job) | 7 green on first push | 8 PRs | **87.5%** |
| Local `cargo test` before PR | ~2 fix cycles (M3 flake, M4 serde) | 8 milestones | **~75%** |
| PR merge without code-change commit on branch | 7 / 8 | 8 PRs | **87.5%** |

**Known first-pass failures**:

- PR #13 (M4): `rustfmt` Linux vs Windows match-arm formatting → fix commit on branch.
- Early `main` push: 2 CI failures during bootstrap (pre-M1).

### 3.5 Bug fix & rework rate

| Category | Count | % of commits (pre-squash history) |
|----------|-------|-------------------------------------|
| Feature commits | ~10 | ~71% |
| Style/rework (`rustfmt`) | 2 | ~14% |
| Docs/harness only | 2 | ~14% |
| Post-merge production bugs | 0 filed | 0% |

**Rework rate** (branch-level fix commits / total branch commits): **~12.5%** (1 fix per 8 feature PRs).

**Bug fix rate**: Not measurable yet — no user-reported Issues after merge. Internal defects caught pre-merge:

- M3: parallel test shared temp dir → flaky `persistence_across_connections`
- M3: `ColumnDef` missing `Serialize` for WAL
- M6/M7: clippy `dead_code` / unused imports in test harness

### 3.6 Wait / cycle time

| Wait type | Typical duration | Frequency |
|-----------|------------------|-----------|
| CI `rust` job | ~3m 35s – 3m 56s | Every PR |
| PR open → merge | ~4–5 minutes | 8 PRs |
| `needs-human` (#3, #4) | ~25 min to resolution | 2 (bootstrap) |
| User “continue?” prompts | Eliminated after autonomy rule | 0 in latter half |
| Human PR review | De facto auto-merge on green | Low friction |

**Agent wall-clock on CI**: ~32 minutes cumulative (8 × ~4 min) — acceptable overhead.

**Issue → merge lead time** (examples):

| Issue | Created → Closed | Notes |
|-------|------------------|-------|
| #10 M3 | ~15 min | Includes PR #11 |
| #14 M5 | ~7 min | Fastest slice |
| #7 M7 | ~28 hours | P2 → agent-ready later; actual work ~5 min PR |

---

## 4. Milestone timeline

```
M0 Harness     ─┬─ issue #1, bootstrap commit
M1 Handshake   ─┼─ PR #6
M2 COM_QUERY   ─┼─ PR #9, issues #3/#4 ADRs
M3 WAL         ─┼─ PR #11
M4 Index       ─┼─ PR #13 (+1 rustfmt fix)
M5 Compat      ─┼─ PR #15
M6 Auth+DML    ─┼─ PR #17, issue #16 decision doc
M7 caching_sha2─┼─ PR #18, closes #7
M8 UPDATE      ─┴─ PR #20
```

---

## 5. Sensor effectiveness

| Sensor | Catches observed | False negatives |
|--------|------------------|-----------------|
| rustfmt | Cross-platform formatting | None significant |
| clippy | unused/dead_code | — |
| cargo test | Logic, protocol, compat | M3 flake (fixed) |
| harness-validate | Missing harness dirs | — |
| compat_suite | SQL wire regressions | Only covered SQL in JSON |

**Recommendation**: Add `cargo fmt --check` to **pre-commit local habit** (already in CI) — eliminates 100% of observed CI failures.

---

## 6. What worked well

1. **Issue-loop + agent-ready** — Clear queue after M0; agent did not stall on “continue?”
2. **Milestone = PR** — `main` always deliverable; user guide tracks reality
3. **ADR for forks** — Auth and parser choices unblocked M2–M7 without re-debate
4. **M5 compat fixtures** — Best feedback investment; encodes user-testable SQL
5. **Shared `test_support` wire client** — Reduced duplication; accelerated M5–M8 tests
6. **Autonomous decision tables in issues** (#16) — Human can override async via Issue comment

---

## 7. What to improve (harness evolution)

| Priority | Item | Feedforward / Feedback |
|----------|------|------------------------|
| P0 | Differential MySQL compat runner (optional Docker) | Feedback |
| P0 | Stale HANDOFF sensor or merge checklist | Feedforward |
| P1 | Per-milestone `docs/specs/mN-*.md` template enforcement | Feedforward |
| P1 | zh-CN doc parity lint (line count / section headers) | Feedback |
| P1 | Record CI first-pass metric in PR template checkbox | Feedback |
| P2 | Close #5 replication ADR before M10+ | Feedforward |
| P2 | RSA full-auth for caching_sha2 (issue #7 follow-up) | Spec gap |

---

## 8. Progress toward MySQL 8.0 goal

| Area | Status | Harness coverage |
|------|--------|------------------|
| Wire protocol handshake | Done (sha2 + native) | Integration tests |
| COM_QUERY subset | Partial | compat JSON |
| Persistence | WAL skeleton | Unit + integration |
| Indexes | Secondary B+Tree | Unit + compat |
| Auth | Fast-path sha2/native | Auth tests |
| DML | CRUD subset | compat |
| Prepared statements | Not started | — |
| Transactions | Not started | — |
| information_schema | Not started | — |
| mysql-test subset | Not started | — |
| Replication | ADR only (#5) | — |

**Estimated compatibility depth**: ~5–8% of full MySQL 8.0 surface — but **harness velocity is high** (~0.5 milestones/hour in burst mode).

---

## 9. Conclusion

The rusql Harness Engineering setup successfully **fed forward** intent (roadmap → issues → ADRs → user docs) and **fed back** quality (sensors → CI → compat tests) through M8. Metrics indicate **healthy decomposition**, **high merge cadence**, and **low rework**, with CI formatting as the main repeatable failure mode.

Pausing implementation for this retrospective is appropriate before M9+: the next harness investments should emphasize **differential compat feedback** and **spec file rigor**, not more process overhead.

---

*Generated from repository state, git log, GitHub Issues/PRs, and CI run history on 2026-06-30.*
