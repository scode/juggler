### Maintenance rule for agents

Always keep this file updated when you change the codebase:
- Fix anything incorrect or outdated here as part of your edit.
- For major changes, add or expand sections to capture the new architecture and behavior.
- Ensure commands, invariants, data model, and workflows remain accurate.
- Always run `cargo fmt` after every edit before you run tests or open a PR.

## Agent Guide: juggler

A Rust terminal UI (TUI) for managing TODOs stored in YAML with optional one-way sync to Google Tasks. Dual-mode: interactive TUI (default) and CLI (login/sync). Async stack with Tokio; modular architecture.

### Project at a glance
- **Language/runtime**: Rust 2024 edition, Tokio async
- **UI**: `ratatui` with `crossterm`
- **Storage**: YAML via `serde_yaml`; atomic writes and timestamped archives
- **Sync**: Google Tasks, one-way from local YAML → Google
- **Logging**: `env_logger` via `RUST_LOG`

### Repository layout
- `src/main.rs`: Entry point, Clap CLI, routing between TUI and commands
- `src/ui/mod.rs`: TUI app orchestration and inline side-effect handling
- `src/ui/keymap.rs`: Canonical normal-mode keymap metadata used by both input dispatch and help footer rendering
- `src/ui/event.rs`: Pure input mapping (`read_action`/`map_key`) from key events to update actions
- `src/ui/model.rs`: Plain UI state (`AppModel`, `Section`, `TodoItems`, `UiState`, prompt mode state)
- `src/ui/update.rs`: Reducer logic (`Action` -> state changes) and optional side effects (`Option<SideEffect>`)
- `src/ui/view.rs`: Pure rendering helpers
- `src/ui/editor.rs`: External editor integration (`$VISUAL`/`$EDITOR`)
- `src/ui/widgets.rs`: Rendering-only widgets (currently `PromptWidget`)
- `src/store.rs`: TODO model, YAML IO, archiving
- `src/google_tasks.rs`: REST client, mapping, create/update/delete, dry-run; uses mockable `Clock` for OAuth token expiry
- `src/oauth.rs`: PKCE browser login, local HTTP callback server, and OAuth `state` validation on callback
- `src/config.rs`: Constants (e.g., list name, OAuth client id), paths
- `README.md`: User-facing how-to, commands, examples
- `CLAUDE.md`: Expanded architectural notes and test guidance
- `TODOs.yaml`: Example data (real data lives in `~/.juggler/TODOs.yaml`)

### Build, run, test
- Build: `cargo build --release`
- Run TUI: `cargo run` (or `./target/release/juggler`)
- Login (browser OAuth): `cargo run -- login` (optional `--port 8080`)
- Sync (recommended): `cargo run -- sync google-tasks`
- Dry-run: append `--dry-run`
- Logging: prefix with `RUST_LOG=info|debug`
- Lint/format: `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt`
- Tests: `cargo test`

### TUI key bindings (core)
- `j/k`: navigate
- `o`: expand/collapse item
- `x`: select/deselect for bulk ops
- `e`: toggle done
- `E`: edit in external editor (`$VISUAL`/`$EDITOR`, default "emacs"; supports args)
- `c`: create a new item
- `s` / `S`: snooze by +1 day / -1 day
- `p` / `P`: snooze by +7 days / -7 days
- `t`: set a custom relative delay (for example `5d`, `-2h`)
- `q`: quit and save
- `Q`: quit, save locally, then sync to Google Tasks

### Data model and storage
- File: `~/.juggler/TODOs.yaml`
- Permissions: user-only on Unix; created automatically
- Atomic updates with temp files; archives previous file to `TODOs_YYYY-MM-DDTHHhMMmSSs.yaml`
- YAML schema (serde):
  - `title: string`
  - `comment: string | null` (multiline supported)
  - `done: bool` (defaults false)
  - `due_date: RFC3339 string | null`
  - `google_task_id: string` (set after sync)

### Google Tasks integration
- List name: `juggler` (see `config.rs`)
- One-way sync: local YAML is authoritative; remote is overwritten
- Auth: PKCE browser login stores a refresh token in the OS keychain via the `keyring` crate; sync reads it automatically
- Usage: run `juggler login` once, then `juggler sync google-tasks` (no flags)
- Logout support: `cargo run -- logout` deletes the stored refresh token from the system keychain
- Local callback server default port: 8080 (configurable via `--port`)
- Dry-run shows intended operations without side effects (no Google API writes and no local TODO file/archive writes)

### Architecture summary (for contributors/agents)
- `main.rs`: Defines Clap CLI (default TUI, `login`, `sync google-tasks`). Starts Tokio runtime. Dispatches to UI or command handlers.
- `ui/mod.rs`: Owns runtime dependencies (`TodoEditor`, `SharedClock`) and orchestrates draw -> read action -> update -> optional inline side-effect handling.
- `ui/model.rs`: Plain state only (`AppModel`, `TodoItems`, `UiState`, prompt mode state), with no runtime dependencies.
- `ui/keymap.rs`: Defines normal-mode key bindings and generated help footer text from one metadata table.
- `ui/event.rs`: Maps `KeyEvent` input to update actions; no direct mutation or editor calls.
- `ui/update.rs`: Reducer logic for navigation/toggles/due-date operations/prompt handling and `Option<SideEffect>` requests for edit/create.
- `ui/view.rs`: Rendering-only drawing helpers over immutable model state.
- `ui/widgets.rs`: Prompt widget rendering.
- `ui/editor.rs`: Launches the external editor and maps edited YAML back into `Todo`.
- `store.rs`: Defines `TodoItem` and list container, YAML serialization/deserialization, load/save, and archival. Uses tempfiles + atomic rename.
- `google_tasks.rs`: Maps between `TodoItem` and Google Task. Implements create/update/delete and list reconciliation, ID tracking (`google_task_id`), and dry-run behavior. Uses `reqwest`, structured logging, and a mockable `Clock` for token expiry.
- `oauth.rs`: Implements public-client PKCE OAuth, validates callback `state`, spawns local HTTP server for redirect, opens browser (`open` crate).
- `credential_storage.rs`: `CredentialStore` trait with two implementations:
  - `KeyringCredentialStore` (real; OS keychain via `keyring`)
  - `InMemoryCredentialStore` (mock; used in tests). Provides store/get/delete refresh token.
- `config.rs`: Path helpers, constants like `GOOGLE_TASKS_LIST_NAME` and OAuth client id.

### Common agent workflows
- Add a CLI flag:
  - Extend Clap in `main.rs`, thread the flag into the relevant module, add tests.
- Change a key binding or UI behavior:
  - Update `src/ui/keymap.rs` first (source of truth), then adjust reducer behavior in `src/ui/update.rs` and input mapping in `src/ui/event.rs` as needed; update README shortcuts if user-visible.
- Extend the TODO schema (new field):
  - Update `TodoItem` in `store.rs` with `serde` attributes; ensure load/save round-trips; consider defaulting for backward compat; update sync mapping if relevant.
- Modify sync mapping or add a new provider:
  - Update `google_tasks.rs` mapping, invariants, and tests; for a new provider, mirror the structure and gate behind a new subcommand.
- Adjust storage behavior:
  - Edit `store.rs` save/load, atomicity, and archiving; preserve invariants below.

### Invariants and pitfalls
- Local YAML is the single source of truth; sync is strictly one-way to Google.
- On TUI exit with sync (`Q`), local TODOs are always saved first, then sync runs; on successful sync, TODOs are saved again to persist `google_task_id` updates.
- TUI side effects are single-step and inline (`Option<SideEffect>`): reducer requests edit/create, `App` executes editor I/O, then feeds one follow-up apply action back into the reducer.
- `google_task_id` must remain stable per item; deleting it forces re-creation on next sync.
- Always keep atomic writes + archival semantics intact (tempfile, rename, timestamped backup).
- Do not log secrets or full tokens; prefer `--dry-run` for previews.
- Preserve cross-platform behavior; Unix-only permissions are guarded appropriately.
- OAuth client id is public; for native desktop clients, the client secret is not confidential and is embedded (see `GOOGLE_OAUTH_CLIENT_SECRET`). The app always includes it in token requests.

### Contribution checklist
- Build, test, format, lint:
  - `cargo build`
  - `cargo test`
  - `cargo fmt`
  - `cargo clippy --all-targets --all-features -- -D warnings`
- Update `README.md` if user-visible behavior or flags change.
- Add/adjust tests for new behavior (UI, store round-trips, OAuth, sync; use wiremock where applicable).
- Keep logging helpful and behind `RUST_LOG` levels.

### Quick commands (copy/paste)
- TUI: `cargo run`
- Login: `cargo run -- login`
- Logout: `cargo run -- logout`
- Sync: `RUST_LOG=info cargo run -- sync google-tasks`
- Dry-run: append `--dry-run`
- Clean build + lint: `cargo clean && cargo build && cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`

### Coding guidelines
- Do not add comments that just state "what" something is doing, unless for some reason it is exceedingly unclear.
- Do not add any comments that speak to the reader as if they are reviewing a diff. The comment should address
  the reader as if they are reading a snapshot of the source code without history. For example, comments like
  "moved this line here" make no sense because it references some action taken in a diff.

### Releases

This project uses "dist" (cargo dist) - https://axodotdev.github.io/cargo-dist/ - for release management.

.github/workflows/release.yml should never be directly modified - it is managed using dist.

If dist-workspace.toml is modified, `dist init` must be run to apply resulting changes.
