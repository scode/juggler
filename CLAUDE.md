# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

- `cargo build` - Build the project
- `cargo test` - Run all tests  
- `cargo fmt` - Format code (include changes in PRs)
- `cargo clippy` - Run linter
- `cargo run` - Run the TODO juggler TUI application

**CLI Commands:**
- `cargo run -- sync google-tasks --token <TOKEN>` - Sync with Google Tasks
- `cargo run -- sync google-tasks --token <TOKEN> --dry-run` - Test sync without changes
- `RUST_LOG=info cargo run -- sync google-tasks --token <TOKEN>` - Sync with logging

**IMPORTANT**: ALWAYS run `cargo fmt` and `cargo clippy` after making changes and tests pass.

## Code Architecture

This is a Rust terminal user interface (TUI) application built with Ratatui that manages TODO items from YAML files and syncs with Google Tasks. The application has a dual-mode architecture:

### Core Modules

- **main.rs**: Entry point with Clap CLI handling, async runtime setup, and mode routing (TUI vs CLI)
- **store.rs**: Data persistence layer with YAML serialization, external editor integration, and Google Tasks API sync
- **ui.rs**: TUI implementation with App struct, event handling, rendering logic, and keyboard shortcuts

### Architectural Patterns

- **Dual-mode operation**: TUI mode (default) vs CLI mode (sync commands)
- **Async runtime**: Uses Tokio for Google Tasks API operations
- **Modular design**: Clear separation between UI, storage, and main coordination
- **Event-driven TUI**: Crossterm for input handling with ListState navigation
- **External integrations**: Editor integration via trait abstraction and Google Tasks API sync
- **Logging**: env_logger for sync operation visibility

### Google Tasks Integration

- **One-way sync**: Local YAML is authoritative, changes push to Google Tasks
- **API operations**: Create, update, delete tasks via Google Tasks REST API
- **Dry-run mode**: Preview changes without execution
- **Task mapping**: Local TODOs map to Google Tasks in "juggler" task list
- **ID tracking**: `google_task_id` field links local items to remote tasks

## Data Format

TODO items are stored in YAML format with enhanced structure:
```yaml
- title: "Item title"
  comment: "Optional comment (can be multiline)"
  done: false  # Optional, defaults to false
  due_date: "2025-01-07T09:00:00Z"  # Optional ISO 8601 timestamp
  google_task_id: "task_abc123"     # Set after Google Tasks sync
```

The application reads from `TODOs.yaml` in the project root on startup and automatically saves changes on exit. Items are sorted by due date with overdue items visually highlighted.

## Key Constants and Configuration

- `GOOGLE_TASKS_LIST_NAME`: Set to "juggler" - the Google Tasks list name for sync operations
- TUI keyboard shortcuts defined in `ui.rs` constants (j/k navigation, o expand, x select, etc.)
- External editor uses `$EDITOR` environment variable, defaults to "emacs"

## Testing Considerations

- Tests use isolated temporary files to avoid interference
- Store tests validate YAML roundtrip serialization and loading
- UI tests verify keyboard interactions and display formatting
- Sync operations are not directly tested (require valid Google API tokens)