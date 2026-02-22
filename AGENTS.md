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
- For Google Tasks sync, treat the notes ownership marker (`JUGGLER_META_OWNED_V1`) as the deletion authority.
- Keep logout behavior idempotent: deleting a missing refresh token should be treated as success.

## Commit message intent

- All commits created by agents must follow Conventional Commits as expected by `git-cliff` when `conventional_commits` parsing is enabled.
- Required header format: `<type>[optional scope][!]: <description>`
- `type` must be lowercase. `feat` is required for new features and `fix` is required for bug fixes. Other allowed types are `docs`/`doc`, `perf`, `refactor`, `style`, `test`, `chore`, `ci`, and `revert`.
- `description` is required, must immediately follow `: `, and should be a concise summary of the change.
- `scope` is optional and must be a short noun in parentheses, for example `fix(parser):`.
- `!` is optional and only valid immediately before `:` to signal a breaking change.
- An optional body may be added only after one blank line following the header.
- Optional footer lines may be added only after one blank line following the body (or following the header when there is no body).
- Footer lines must use git trailer style (`Token: value`), with one footer per line.
- Breaking changes must include a line starting with `BREAKING CHANGE: ` in the body or footer, even when `!` is used in the header.
- Commit messages that do not follow these rules are not allowed.

## Release intent

- Releases are managed with `cargo dist`.
- Do not modify `.github/workflows/release.yml` directly.
- If `dist-workspace.toml` changes, run `dist init` and commit the generated updates.
