# Contributing

## Commit and PR Title Conventions

- Commit messages must follow Conventional Commits: `<type>[optional scope][!]: <description>`.
- Use lowercase `type`. Use `feat` for new features and `fix` for bug fixes.
- Allowed types are `feat`, `fix`, `docs`, `doc`, `perf`, `refactor`, `style`, `test`, `chore`, `ci`, and `revert`.
- Breaking changes may use `!` in the header and must include a `BREAKING CHANGE: ` trailer in the body or footer.
- PR titles must follow the same Conventional Commit header format.
- CI enforces PR title format with `amannn/action-semantic-pull-request` in `.github/workflows/convention-commit-pr-title.yml`.

## Changelog Setup

`cargo dist` populates GitHub release notes by parsing a root changelog file (for example `CHANGELOG.md`). This repository uses `git-cliff` for changelog generation and keeps the configuration in `cliff.toml`.

### Maintainer Setup (One-Time Per Machine)

Install `git-cliff`:

```bash
brew install git-cliff
# or
cargo install git-cliff
```

### Repository Bootstrap (One-Time In Repository History)

This setup has already been completed in this repository. It is not part of each release.

1. Initialize the Keep a Changelog template:
   ```bash
   git-cliff --init keepachangelog
   ```
2. Commit `cliff.toml` (and any intended template edits) so all release runs use the same format.

## Release Process

1. Set the release version in `Cargo.toml` (`X.Y.Z`).
2. Run `cargo update --workspace` to refresh `Cargo.lock` for the new workspace version.
3. Run `cargo metadata --format-version 1 --locked > /dev/null` to confirm lockfile consistency.
4. Generate the changelog entry for this release before tagging:
   ```bash
   VERSION=X.Y.Z
   git-cliff --tag "v$VERSION" -o CHANGELOG.md
   ```
5. Verify the release heading exists in `CHANGELOG.md`:
   ```bash
   rg -n "^## \\[$VERSION\\]" CHANGELOG.md
   ```
6. Submit and merge a PR that includes `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`.
7. Tag the merge commit as `vX.Y.Z` and push the tag.
8. `cargo dist` handles the rest of the release workflow and uses the matching changelog heading for the GitHub Release body.
9. The dist release plan runs tests on both Ubuntu and macOS before artifact builds. Regular `CI` (`.github/workflows/ci.yml`) keeps macOS tests disabled for non-release PR/push runs.

See [dist documentation](https://github.com/axodotdev/cargo-dist) for details.
