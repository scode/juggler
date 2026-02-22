# Contributing

## Release Process

1. Edit `Cargo.toml` and bump `package.version` to the release version.
2. Run `cargo update --workspace` to refresh `Cargo.lock` for the new workspace version.
3. Run `cargo metadata --format-version 1 --locked > /dev/null` to confirm lockfile consistency without running tests.
4. Submit a PR with these changes and merge it.
5. Tag the merge commit as `vX.Y.Z` and push the tag.
6. `cargo dist` handles the rest of the release workflow.

See [dist documentation](https://github.com/axodotdev/cargo-dist) for details.
