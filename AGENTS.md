# AGENTS.md

This file should capture project intent and policy for automated contributors.
Keep it opinionated and durable; do not use it as an architecture inventory.

## Source of truth

- Treat `SPEC.md` as a mandatory behavioral specification.
- Treat the codebase and tests as the executable implementation of that specification.
- If this file conflicts with observed behavior or policy, update this file.

## Coding intent

- Place Rust doc comments (`///`, `//!`) before attribute directives (`#[derive(...)]`, `#[cfg(...)]`, and serde attributes).
- Prefer comments that explain intent, tradeoffs, and context over comments that restate obvious behavior.
- Do not write comments that depend on diff/history context.

## Workflow intent

- Keep this file focused on non-discoverable intent and policy; remove factual drift when you encounter it.
- Run `cargo fmt` after edits, before tests, and before opening or updating a PR.
- Add or update tests for behavior changes unless explicitly directed otherwise.
- Update user-facing docs when behavior, flags, or workflows change.
- When behavior changes are intentional and clear, update `SPEC.md` in the same change so the specification remains authoritative.
- During code review, verify implementation and tests for `SPEC.md` compliance; call out any mismatches explicitly.

## Release intent

- Releases are managed with `cargo dist`.
- Do not modify `.github/workflows/release.yml` directly.
- If `dist-workspace.toml` changes, run `dist init` and commit the generated updates.
