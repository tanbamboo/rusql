# AGENTS.md — Project Contract

> Entry contract for AI coding agents. Follows the [agents.md](https://agents.md) open standard.
> Load details progressively via links; do not inline an encyclopedia here.

**简体中文**: [docs/zh-CN/AGENTS.md](docs/zh-CN/AGENTS.md)

## Purpose

**rusql** is a Rust implementation of a MySQL 8.0-compatible database, built with Harness Engineering (Agent = Model + Harness). Long-term goal: full MySQL 8.0 compatibility; MVP starts with wire protocol and basic SQL.

## Repository Map

Full map: [.agents/context-index.md](.agents/context-index.md).

| Path | Purpose |
|------|---------|
| `crates/` | Rust workspace crates (protocol, sql, storage, server, …) |
| `profiles/rust/` | Stack-specific guides and sensors |
| `.agents/` | Portable agent layer (cross-tool) |
| `docs/en/` | English documentation (canonical) |
| `docs/zh-CN/` | Simplified Chinese mirrors |
| `.cursor/` | Cursor thin wrapper (rules, skills, hooks) |
| `locales/` | Shared locale files (referenced by `rusql-i18n`) |

## Active Profile

**rust** — see [profiles/rust/](profiles/rust/).

## Common Commands

| Command | Description |
|---------|-------------|
| `cargo fmt --all` | Format Rust code |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint |
| `cargo test` | Run tests |
| `cargo build --release` | Release build |
| `node scripts/harness-validate.mjs` | Validate harness structure |

## Session Protocol

1. **Start**: Read [HANDOFF.md](HANDOFF.md) and `profiles/rust/guides.md`; poll `agent-ready` GitHub issues
2. **Plan**: Complex tasks use Plan mode; follow [docs/en/workflows/spec-to-ship.md](docs/en/workflows/spec-to-ship.md)
3. **Implement**: Run profile sensors before declaring done
4. **End**: Update HANDOFF.md; log repeat failures in [HARNESS_CHANGELOG.md](HARNESS_CHANGELOG.md)

## Trust Boundaries

See [docs/en/agent-governance/trust.md](docs/en/agent-governance/trust.md).

**Agents may autonomously:**
- Implement features in `crates/` within issue file boundaries
- Run lint/test/build
- Update related documentation
- Open PRs linked to issues

**Agents must not autonomously:**
- Modify `CONSTITUTION.md`, `.github/CODEOWNERS`, or production secrets
- Force-push or rewrite git history
- Skip CI sensors or commit with `--no-verify`
- Hardcode user-visible strings in `crates/` (use `rusql-i18n` keys)

## Core Principles

1. **Spec-first**: Spec is the program — see [CONSTITUTION.md](CONSTITUTION.md)
2. **Deterministic over prose**: Important rules need matching sensors
3. **Hashimoto loop**: Every repeated agent failure → harness fix
4. **Local gates = CI gates**
5. **i18n**: Default locale `en-US`; also support `zh-CN`

## Further Reading

- Governance: [docs/en/agent-governance/](docs/en/agent-governance/)
- Architecture: [docs/en/architecture/](docs/en/architecture/)
- Workflows: [docs/en/workflows/](docs/en/workflows/)
