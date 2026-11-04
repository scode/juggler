# Homebrew publishing migration

Keep the publisher-switch PR unmerged until the tap's juggler updater and candidate formula have been reviewed and tested. This change prepares the handoff; it does not activate a pull-managed formula in the tap.

## Publication ownership

`scode/homebrew-dist-tap` will own formula updates through its [pull workflow](https://github.com/scode/homebrew-dist-tap/blob/main/docs/pull-workflow.md). In this repository, `publish-jobs = []` disables only cargo-dist's tap-push job. cargo-dist 0.30.4 still builds archives, checksums, updater executables, and the release-side `juggler.rb` artifact, and publishes GitHub releases. The generated workflow's archive builds, release-plan tests, and GitHub release creation remain in place.

On 2026-11-03, [tap main at `1b2796b`](https://github.com/scode/homebrew-dist-tap/tree/1b2796b26fdbfa15151468b123e4ed786d2b0f39) had live pull entries for saltybox and treeward, but none for juggler. The existing juggler formula was still generated upstream. Review and test the tap's juggler updater and candidate before coordinating the live `pull.toml` entry with this repository's publisher switch. Keep one publisher for the formula during the handoff, and avoid cutting a release between the two activation changes.

Do not revoke `HOMEBREW_TAP_TOKEN` or any shared credentials as part of this change. Other upstream publishers may still need them. Removing the job's reference to the secret does not revoke the credential.

## Published archive contract

The inspected release is [v0.2.6](https://github.com/scode/juggler/releases/tag/v0.2.6), published on 2026-02-26. It predates the TLS fixes merged in [#261](https://github.com/scode/juggler/pull/261/changes) and the dependency update in [#262](https://github.com/scode/juggler/pull/262/changes). Use it as evidence of the archive layout, not as a patched migration candidate. Publish and verify a new release containing those changes before selecting the activation candidate; no new release is created by the publisher-switch PR.

All four archives were downloaded, listed, and checked against their published `.sha256` files on 2026-11-03. They are hosted under `https://github.com/scode/juggler/releases/download/v0.2.6/`:

| Platform | Asset | SHA-256 |
| --- | --- | --- |
| macOS ARM64 | `juggler-aarch64-apple-darwin.tar.xz` | `e6252cbe031f530400c3bd026e50e8516d382105bc44b62b1dcc244580d27000` |
| macOS x86-64 | `juggler-x86_64-apple-darwin.tar.xz` | `6bac328615c4c0ef12681c556a1143671b6dc2ad2c0130e6f82aa5f5505b0b90` |
| Linux ARM64, GNU libc | `juggler-aarch64-unknown-linux-gnu.tar.xz` | `a56fa02764c21d9362caab08fd34dfa69de2180013e72ac63068003dad7deb74` |
| Linux x86-64, GNU libc | `juggler-x86_64-unknown-linux-gnu.tar.xz` | `b02e40e4f3596b508d805cadca66b00af2b54a6c4dd07ccc6703a8070da38fd5` |

Each archive contains one top-level `juggler-<target>/` directory with exactly these files:

```text
juggler-<target>/
  juggler
  LICENSE
  README.md
  CHANGELOG.md
```

Install `juggler` into `bin`. There are no aliases or other binaries inside these archives. The MIT `LICENSE` is byte-identical across all four archives and this repository. Preserve `README.md` and `CHANGELOG.md` as documentation. The release also contains separate `juggler-<target>-update` executables; these are not part of the formula's installed binaries. The tap updater should consume archives and hashes, not execute upstream-generated Ruby.

## Runtime requirements and verification limits

Both Linux binaries list `libgcc_s.so.1`, `libm.so.6`, `libc.so.6`, and their architecture's ELF loader as dynamic dependencies. The loaders are `/lib64/ld-linux-x86-64.so.2` and `/lib/ld-linux-aarch64.so.1`. Both require GLIBC symbols through 2.34; the release manifest records builds on glibc 2.35. There is no dynamic OpenSSL or libdbus dependency in either binary.

Both macOS binaries load the system SystemConfiguration, Security, and CoreFoundation frameworks, `/usr/lib/libiconv.2.dylib`, and `/usr/lib/libSystem.B.dylib`. Their Mach-O load commands declare minimum macOS versions of 11.0 for ARM64 and 10.12 for x86-64. These are binary metadata, not evidence of execution on those oldest systems. No non-system dynamic libraries were found on either platform, so this inspection identifies no additional Homebrew library dependency.

Interactive editing needs an external editor (`VISUAL`, then `EDITOR`, otherwise `emacs`). Google Tasks authentication uses the OS credential store: macOS Keychain or a Linux D-Bus Secret Service. Browser authorization needs access to Google's login page; the printed URL can be opened manually if the browser launcher fails. These facilities are not needed for the noninteractive validation test below.

Only the Linux x86-64 release binary was executed here. ARM64 Linux and both macOS archives were inspected and hashed, not installed or run. The tap's candidate still needs its native installation and formula tests, with the platforms actually exercised recorded according to the tap's maintenance guide.

## Candidate formula test

This exercises the on-disk format contract and the sync command's credential precondition without a terminal, browser, network request, or keychain access. An unsupported store version must fail before authentication; a valid store must reach credential validation without being rewritten or archived. Explicit blank flags override any inherited OAuth credentials.

The two CLI outcomes and unchanged valid file were verified using the published Linux x86-64 binary. The Ruby block is a proposed formula test, not a claim that a candidate formula has passed `brew test`:

```ruby
test do
  state = testpath/"state"
  state.mkpath
  store = state/"TODOs.toml"
  store.write <<~TOML
    [metadata]
    format_version = 2
    juggler_edition = 1

    [todos.T1]
    title = "Homebrew fixture"
    done = false
  TOML

  command = "#{bin}/juggler --juggler-dir #{state} " \
            "--google-oauth-client-id ' ' --google-oauth-client-secret ' ' " \
            "sync google-tasks --dry-run 2>&1"
  assert_match "unsupported metadata.format_version=2 (expected 1)", shell_output(command, 1)

  File.write(store, store.read.sub("format_version = 2", "format_version = 1"))
  original = store.read
  assert_match "Missing Google OAuth client id", shell_output(command, 1)
  assert_equal original, store.read
  assert_equal ["TODOs.toml"], state.children.map { |path| path.basename.to_s }.sort
end
```

For activation, bind the tap's reviewed updater, config, and candidate formula to the new patched release and its four recorded hashes. Confirm regeneration matches the candidate, and require its native install and formula-test evidence before merging the publisher switch. Coordinate the dedicated tap activation change with this PR; do not infer readiness from the presence of the generic updater alone.
