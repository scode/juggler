# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

- `cargo build` - Build the project
- `cargo test` - Run all tests  
- `cargo fmt` - Format code (include changes in PRs)
- `cargo clippy` - Run linter
- `cargo run` - Run the TODO juggler TUI application

## Code Architecture

This is a Rust terminal user interface (TUI) application built with Ratatui that displays TODO items from YAML files. The main components:

- **main.rs**: Single-file application containing the complete TUI implementation
- **App struct**: Main application state managing list selection and TODO items
- **Todo struct**: Represents individual TODO items with expandable comments
- **Data loading**: Reads from `TODOs.yaml` (hardcoded filename in `load_todos()`)

Key architectural patterns:
- Event-driven TUI using crossterm for input handling
- State management through ListState for navigation
- Toggle-based expand/collapse for TODO items with comments
- Visual indicators (📋/📖) for expandable/expanded items

## Data Format

TODO items are stored in YAML format with structure:
```yaml
- title: "Item title"
  comment: "Optional comment (can be multiline)"
```

The application expects `TODOs.yaml` in the project root and loads it on startup.