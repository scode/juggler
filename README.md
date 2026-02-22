# juggler

A TODO juggler TUI application built with [Ratatui] that displays and manages TODO items from TOML files. Features a terminal user interface for managing tasks with due dates, comments, and Google Tasks synchronization.

[Ratatui]: https://ratatui.rs

## Features

- **Terminal User Interface**: Navigate and manage TODOs with keyboard shortcuts
- **TOML Storage**: TODOs stored in human-readable TOML format with metadata, stable IDs, comments, and due dates
- **Due Date Support**: Automatic sorting with overdue items highlighted
- **External Editor Integration**: Edit TODOs in your preferred editor (via `$VISUAL`/`$EDITOR`)
- **Google Tasks Sync (Bare Bones)**: Manual setup flow; see [`docs/google-tasks-sync.md`](docs/google-tasks-sync.md)
- **Completion Tracking**: Mark items as done/undone
- **Snooze/Prepone**: Quickly adjust due dates by ±1 day or ±7 days, plus custom delays

## Installation

### macOS and Linux (Homebrew)

```bash
brew install scode/dist-tap/juggler
```

### Other platforms

1. Install Rust using [rustup](https://rustup.rs/).
2. From the repository root, run:

```bash
cargo install --path .
```

## Quick Start

1. **Run the TUI:**
   ```bash
   juggler
   ```

2. Optional: **Google Tasks sync is currently very bare bones** and requires manual setup (including changing OAuth client values in code). See [`docs/google-tasks-sync.md`](docs/google-tasks-sync.md).

## Basic Usage

### Terminal UI Mode

Launch the interactive TUI (default behavior):

```bash
cargo run
# or
./target/release/juggler
```

**Keyboard Shortcuts:**
- `j/k` - Move cursor down/up
- `o` - Toggle expand/collapse on the cursored item
- `x` - Select/deselect the cursored item
- `e` - Toggle done on selected items; if none selected, acts on the cursored item
- `E` - Edit the cursored item in external editor (`$VISUAL`/`$EDITOR`, supports args)
- `s` - Snooze selected items by 1 day; if none selected, snooze the cursored item
- `S` - Unsnooze (minus 1 day) for selected items; if none selected, unsnooze the cursored item
- `p` - Snooze by 7 days for selected items; if none selected, snooze the cursored item
- `P` - Prepone by 7 days for selected items; if none selected, prepone the cursored item
- `t` - Custom delay prompt (e.g., 5d, -2h)
- `q` - Quit and save
- `Q` - Quit, save, and sync to Google Tasks (sync is skipped if the initial local save fails)

Note: Actions operate on all selected items. If no items are selected, they apply to the item under the cursor.

### Command Line Mode

View available commands:

```bash
juggler --help
juggler login --help
juggler logout --help
juggler sync google-tasks --help
```

**Available commands:**
- `juggler` - Launch interactive TUI mode
- `juggler login` - Browser-based OAuth authentication
- `juggler sync google-tasks` - Sync TODOs with Google Tasks
- `juggler logout` - Remove the stored refresh token (idempotent if no token is stored)

**Login options:**
- `--port <PORT>`: Local callback port (default: 8080)

**Sync options:**
- `--dry-run`: Log actions without executing them (safe testing mode)

## Google Tasks Sync

Synchronization to Google Tasks is currently very bare bones and requires manual setup, including changing the OAuth client values in source code before building. See [`docs/google-tasks-sync.md`](docs/google-tasks-sync.md).

## Data Format

By default, TODOs are stored at `~/.juggler/TODOs.toml`. Each save creates a timestamped backup of the previous file in the same directory (e.g., `TODOs_2025-01-07T09-00-00.toml`).

```toml
[metadata]
format_version = 1
juggler_edition = 1

[todos.T1]
title = "Buy groceries"
comment = """- Milk
- Bread
- Eggs"""
done = false
due_date = "2025-01-07T09:00:00Z"  # ISO 8601 format
google_task_id = "task_abc123"     # Set after sync

[todos.T2]
title = "Completed task"
done = true
```

## License

Copyright (c) Peter Schuller <peter.schuller@infidyne.com>

This project is licensed under the MIT license ([LICENSE] or <http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
