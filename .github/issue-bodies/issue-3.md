## Question

Which authentication plugin should rusql support in the MVP protocol handshake?

## Options Considered

### A: `mysql_native_password` only (recommended for MVP)
- Pros: simpler, widely supported by older clients, easier stub implementation
- Cons: deprecated in MySQL 8.0 default

### B: `caching_sha2_password` only
- Pros: MySQL 8.0 default
- Cons: more complex crypto (RSA, SHA256)

### C: Both with negotiation
- Pros: best compatibility
- Cons: more implementation work upfront

## Agent Recommendation

Start with **A** for M1, add **B** in a follow-up issue after handshake works.

Please reply with A, B, or C on this issue.
