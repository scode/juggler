# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

- `cargo build` - Build the project
- `cargo test` - Run all tests  
- `cargo fmt` - Format code (include changes in PRs)
- `cargo clippy` - Run linter
- `cargo run` - Run the TODO juggler TUI application

## Code Architecture

This is a Rust terminal user interface (TUI) application built with Ratatui that displays and manages TODO items from YAML files. The application is now modularized into three main components:

- **main.rs**: Entry point that initializes the terminal, loads todos, runs the app, and saves on exit
- **store.rs**: Data persistence layer handling YAML serialization/deserialization and external editor integration
- **ui.rs**: TUI implementation with App struct, event handling, and rendering logic

Key architectural patterns:
- Modular design with separation of concerns (UI, storage, main coordination)
- Event-driven TUI using crossterm for input handling
- State management through ListState for navigation
- External editor integration via trait abstraction (TodoEditor)
- Auto-save functionality on application exit
- Due date support with automatic sorting (overdue items highlighted)

## Data Format

TODO items are stored in YAML format with enhanced structure:
```yaml
- title: "Item title"
  comment: "Optional comment (can be multiline)"
  done: false  # Optional, defaults to false
  due_date: "2025-01-07T09:00:00Z"  # Optional ISO 8601 timestamp
```

The application reads from `TODOs.yaml` in the project root on startup and automatically saves changes on exit. Items are sorted by due date with overdue items visually highlighted.