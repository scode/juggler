# Contributing

## Commit and PR Title Conventions

- Commit messages must follow Conventional Commits: `<type>[optional scope][!]: <description>`.
- Use lowercase `type`. Use `feat` for new features and `fix` for bug fixes.
- Allowed types are `feat`, `fix`, `docs`, `doc`, `perf`, `refactor`, `style`, `test`, `chore`, `ci`, and `revert`.
- Breaking changes may use `!` in the header and must include a `BREAKING CHANGE: ` trailer in the body or footer.
- PR titles must follow the same Conventional Commit header format.
- CI enforces PR title format with `amannn/action-semantic-pull-request` in `.github/workflows/convention-commit-pr-title.yml`.

## Release Process

1. Edit `Cargo.toml` and bump `package.version` to the release version.
2. Run `cargo update --workspace` to refresh `Cargo.lock` for the new workspace version.
3. Run `cargo metadata --format-version 1 --locked > /dev/null` to confirm lockfile consistency without running tests.
4. Submit a PR with these changes and merge it.
5. Tag the merge commit as `vX.Y.Z` and push the tag.
6. `cargo dist` handles the rest of the release workflow.

See [dist documentation](https://github.com/axodotdev/cargo-dist) for details.
