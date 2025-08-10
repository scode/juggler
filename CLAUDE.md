# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

- `cargo build` - Build the project
- `cargo test` - Run all tests  
- `cargo fmt` - Format code (include changes in PRs)
- `cargo clippy` - Run linter
- `cargo run` - Run the TODO juggler TUI application

**CLI Commands:**

*OAuth Refresh Token Authentication (Recommended):*
- `cargo run -- sync google-tasks --refresh-token <REFRESH_TOKEN>` - Sync with Google Tasks using OAuth
- `cargo run -- sync google-tasks --refresh-token <REFRESH_TOKEN> --dry-run` - Test sync without changes
- `RUST_LOG=info cargo run -- sync google-tasks --refresh-token <REFRESH_TOKEN>` - Sync with logging

*Browser OAuth Login (Recommended for first-time setup):*
- `cargo run -- login` - Interactive browser-based OAuth authentication
- `cargo run -- login --port 8080` - OAuth login with custom callback port

*Legacy Bearer Token Authentication (Deprecated):*
- `cargo run -- sync google-tasks --token <TOKEN>` - Sync with Google Tasks using bearer token (deprecated)
- `cargo run -- sync google-tasks --token <TOKEN> --dry-run` - Test sync without changes (deprecated)
- `RUST_LOG=info cargo run -- sync google-tasks --token <TOKEN>` - Sync with logging (deprecated)

**IMPORTANT**: ALWAYS run `cargo fmt` and `cargo clippy` after making changes and tests pass.

## Code Architecture

This is a Rust terminal user interface (TUI) application built with Ratatui that manages TODO items from YAML files and syncs with Google Tasks. The application has a dual-mode architecture:

### Core Modules

- **main.rs**: Entry point with Clap CLI handling, async runtime setup, and mode routing (TUI vs CLI)
- **store.rs**: Data persistence layer with YAML serialization, external editor integration, TODO data structures, and archiving functionality
- **ui.rs**: TUI implementation with App struct, event handling, rendering logic, and keyboard shortcuts
- **google_tasks.rs**: Google Tasks API integration with OAuth client, sync operations, and task mapping
- **oauth.rs**: OAuth 2.0 PKCE flow implementation with local HTTP server for browser authentication
- **config.rs**: Application constants including API URLs, OAuth client ID, and file path management functions

### Architectural Patterns

- **Dual-mode operation**: TUI mode (default) vs CLI mode (sync/login commands)
- **Async runtime**: Uses Tokio for Google Tasks API operations and OAuth flows
- **Modular design**: Clear separation between UI, storage, API integration, and main coordination
- **Event-driven TUI**: Crossterm for input handling with ListState navigation and dual-section layout (pending/done)
- **External integrations**: Editor integration via trait abstraction, Google Tasks API sync, and OAuth browser flow
- **Logging**: env_logger for sync operation visibility with configurable levels
- **Trait-based architecture**: TodoEditor trait allows for different editing backends (external editor, potential future UI editor)

### Google Tasks Integration

- **One-way sync**: Local YAML is authoritative, changes push to Google Tasks
- **Authentication**: OAuth refresh token (recommended) or bearer token (deprecated)
- **OAuth flow**: PKCE-based browser authentication with local HTTP server callback
- **Token management**: Automatic access token refresh with 5-minute buffer and caching
- **API operations**: Create, update, delete tasks via Google Tasks REST API
- **Dry-run mode**: Preview changes without execution
- **Task mapping**: Local TODOs map to Google Tasks in "juggler" task list with "j:" prefix
- **ID tracking**: `google_task_id` field links local items to remote tasks
- **Built-in OAuth client**: Uses hardcoded public client ID for seamless authentication
- **Environment variables**: No client secret required; PKCE public client flow only

## Data Format and Storage

TODO items are stored in YAML format with enhanced structure:
```yaml
- title: "Item title"
  comment: "Optional comment (can be multiline)"
  done: false  # Optional, defaults to false
  due_date: "2025-01-07T09:00:00Z"  # Optional ISO 8601 timestamp
  google_task_id: "task_abc123"     # Set after Google Tasks sync
```

### File Storage Architecture

- **Primary file**: `~/.juggler/TODOs.yaml` - main TODO storage
- **Directory creation**: `~/.juggler` directory created automatically with secure permissions (owner-only on Unix)
- **Archiving system**: Before each update, existing `TODOs.yaml` is copied to `TODOs_YYYY-MM-DDTHH-MM-SS.yaml`
- **Atomic updates**: Uses temporary files and atomic rename to prevent data corruption
- **Platform compatibility**: Permission setting is conditional (Unix only, skipped on Windows)

The application reads from `~/.juggler/TODOs.yaml` on startup and automatically saves changes on exit. Items are sorted by due date with overdue items visually highlighted.

## Key Constants and Configuration

- `GOOGLE_TASKS_LIST_NAME`: Set to "juggler" - the Google Tasks list name for sync operations
- `GOOGLE_OAUTH_CLIENT_ID`: Hardcoded public OAuth client ID for browser authentication
- TUI keyboard shortcuts defined in `ui.rs` constants:
  - `j/k` - Navigate up/down between items
  - `o` - Toggle expand/collapse item (show/hide comments)
  - `x` - Toggle select/deselect item for bulk operations
  - `e` - Toggle done status for selected items or current item
  - `E` - Edit current item in external editor
  - `s/S` - Snooze items by 1 day/1 week
  - `q` - Quit and save
- External editor uses `$EDITOR` environment variable, defaults to "emacs"
- TODO storage: `~/.juggler/TODOs.yaml` with automatic archiving to timestamped backups

## Testing Considerations

- Tests use isolated temporary files to avoid interference
- Store tests validate YAML roundtrip serialization, loading, and archiving functionality
- UI tests verify keyboard interactions, display formatting, and state management
- Google Tasks sync operations use wiremock for HTTP mocking without requiring real API tokens
- OAuth client tests verify token refresh, caching, and error handling
- Comprehensive test coverage for both OAuth and legacy bearer token authentication paths
- Tests validate dry-run mode functionality and logging behavior
- Archive tests verify timestamped backup creation and content preservation

## Key Dependencies

- **ratatui**: Modern terminal UI framework for the TUI interface
- **crossterm**: Cross-platform terminal manipulation for input handling
- **reqwest**: HTTP client for Google Tasks API interactions
- **tokio**: Async runtime for API operations and OAuth flows
- **serde/serde_yaml**: Serialization for YAML data persistence
- **chrono**: Date/time handling for due dates and relative time display
- **clap**: Command-line argument parsing
- **hyper**: HTTP server for OAuth callback handling
- **wiremock**: HTTP mocking for comprehensive API testing
- **dirs**: Cross-platform home directory detection
- **tempfile**: Secure temporary file handling for atomic operations