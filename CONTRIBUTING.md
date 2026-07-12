# Contributing to OmniMesh

## Development Loop

Every feature follows this pipeline:

1. **Research** — Understand the problem, read relevant RFCs
2. **Architecture** — Design the solution, document trade-offs
3. **RFC** — Write an ADR (Architecture Decision Record) in `docs/adr/`
4. **Implementation** — Write code in small, reviewable PRs
5. **Unit Tests** — Every module has tests, every edge case covered
6. **Integration Tests** — Cross-crate tests in `tests/`
7. **Benchmark** — Performance-sensitive code gets benchmarked
8. **Security Audit** — Review for panics, leaks, timing attacks
9. **Documentation** — Every public API documented
10. **Merge** — Nothing skips a stage

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` — zero warnings policy
- Every public function has a doc comment
- Keep files small (< 300 lines) — one responsibility per module
- Use `thiserror` for error types, `tracing` for logging
- Zeroize all key material on drop

## Architecture Decision Records (ADRs)

Major decisions are documented in `docs/adr/NNN-title.md`:

```markdown
# NNN — Decision Title

## Status: Accepted | Superseded | Deprecated

## Context
Why this decision was needed.

## Decision
What we decided and why.

## Consequences
What this means going forward.
```

## License

By contributing, you agree that your contributions will be licensed
under Apache-2.0 license.
