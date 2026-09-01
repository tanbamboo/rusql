## Question

Confirm SQL parser approach for rusql MVP.

## Options Considered

### A: `sqlparser` crate with MySQL dialect (current scaffold)
- Pros: fast to integrate, good for standard SQL
- Cons: not 100% MySQL-compatible; gaps in MySQL-specific syntax

### B: Fork/extend sqlparser for MySQL gaps
- Pros: incremental compatibility improvements
- Cons: maintenance burden

### C: Build custom MySQL parser from scratch
- Pros: full control
- Cons: very high effort, not recommended for MVP

## Agent Recommendation

**A** now, with **B** as targeted extensions when mysql-test-runner reveals gaps.

Please confirm or suggest an alternative.
