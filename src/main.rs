use std::{fs, io};

use chrono::{DateTime, Utc};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Borders, List, ListState, Paragraph},
};

const HELP_TEXT: &str =
    "o - open, j - select next, k - select previous, x - toggle select, e - toggle done, q - quit";

const KEY_QUIT: KeyCode = KeyCode::Char('q');
const KEY_TOGGLE_EXPAND: KeyCode = KeyCode::Char('o');
const KEY_NEXT_ITEM: KeyCode = KeyCode::Char('j');
const KEY_PREVIOUS_ITEM: KeyCode = KeyCode::Char('k');
const KEY_TOGGLE_DONE: KeyCode = KeyCode::Char('e');
const KEY_TOGGLE_SELECT: KeyCode = KeyCode::Char('x');

#[derive(Debug, serde::Deserialize)]
struct TodoConfig {
    title: String,
    comment: Option<String>,
    #[serde(default)]
    done: bool,
    due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct Todo {
    title: String,
    comment: Option<String>,
    expanded: bool,
    done: bool,
    selected: bool,
    due_date: Option<DateTime<Utc>>,
}

impl Todo {
    fn collapsed_summary(&self) -> String {
        let mut text = String::new();

        // Add relative time if due date exists
        if let Some(relative_time) = self.format_relative_time() {
            text.push_str(&format!("{} ", relative_time));
        }

        text.push_str(&self.title);
        if self.has_comment() {
            if self.expanded {
                text.push_str(" >>>"); // Expanded indicator
            } else {
                text.push_str(" >"); // Expandable indicator
            }
        }
        text
    }

    fn has_comment(&self) -> bool {
        self.comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
    }

    fn expanded_text(&self) -> String {
        let mut text = String::new();

        // Add relative time if due date exists
        if let Some(relative_time) = self.format_relative_time() {
            text.push_str(&format!("{} ", relative_time));
        }

        text.push_str(&self.title);
        if self.has_comment() {
            text.push_str(" >>>"); // Expanded indicator
        }
        if let Some(comment) = &self.comment {
            text.push('\n');
            let indented = comment
                .lines()
                .map(|line| format!("         {}", line))
                .collect::<Vec<_>>()
                .join("\n");
            text.push_str(&indented);
        }
        text
    }

    fn format_relative_time(&self) -> Option<String> {
        self.due_date.map(|due| {
            let now = Utc::now();
            let duration = due.signed_duration_since(now);

            let total_seconds = duration.num_seconds();
            let abs_seconds = total_seconds.abs();

            let (value, unit) = if abs_seconds < 60 {
                (abs_seconds, "s")
            } else if abs_seconds < 3600 {
                (abs_seconds / 60, "m")
            } else if abs_seconds < 86400 {
                (abs_seconds / 3600, "h")
            } else {
                (abs_seconds / 86400, "d")
            };

            if total_seconds < 0 {
                format!("-{}{}", value, unit)
            } else {
                format!("{}{}", value, unit)
            }
        })
    }
}

fn load_todos() -> io::Result<Vec<Todo>> {
    let content = fs::read_to_string("TODOs.yaml")?;
    let configs: Vec<TodoConfig> = serde_yaml::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(configs
        .into_iter()
        .map(|c| Todo {
            title: c.title,
            comment: c.comment,
            expanded: false,
            done: c.done,
            selected: false,
            due_date: c.due_date,
        })
        .collect())
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new()?;
    let app_result = app.run(&mut terminal);
    ratatui::restore();
    app_result
}

#[derive(Debug)]
pub struct App {
    exit: bool,
    state: ListState,
    items: Vec<Todo>,
    pending_count: usize,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let mut state = ListState::default();
        let items = load_todos()?;

        if !items.is_empty() {
            state.select(Some(0));
        }

        let pending_count = items.iter().filter(|item| !item.done).count();

        Ok(App {
            exit: false,
            state,
            items,
            pending_count,
        })
    }
}

impl Default for App {
    fn default() -> Self {
        panic!("Use App::new() instead of App::default() to handle errors properly");
    }
}

impl App {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        use ratatui::layout::{Constraint, Direction, Layout};

        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let main_area = chunks[0];
        let help_area = chunks[1];

        // Split main area between pending and done sections
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(main_area);

        // Render pending section
        let pending_items: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.done)
            .map(|(original_idx, _)| {
                ratatui::widgets::ListItem::new(Text::from(self.display_text(original_idx)))
            })
            .collect();

        let pending_widget = List::new(pending_items)
            .block(Block::default().title("Pending").borders(Borders::ALL))
            .highlight_style(Style::default().fg(Color::Yellow));

        // Render done section
        let done_items: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.done)
            .map(|(original_idx, _)| {
                let text = self.display_text(original_idx);
                let styled_text =
                    Text::styled(text, Style::default().add_modifier(Modifier::CROSSED_OUT));
                ratatui::widgets::ListItem::new(styled_text)
            })
            .collect();

        let done_widget = List::new(done_items)
            .block(Block::default().title("Done").borders(Borders::ALL))
            .highlight_style(Style::default().fg(Color::Green));

        // Determine which section to highlight based on selection
        if let Some(selected_idx) = self.state.selected() {
            if let Some(selected_item) = self.items.get(selected_idx) {
                if selected_item.done {
                    // Highlight done section
                    frame.render_widget(pending_widget, sections[0]);
                    let mut done_state = ListState::default();
                    let done_count = self.items[..=selected_idx]
                        .iter()
                        .filter(|item| item.done)
                        .count();
                    if done_count > 0 {
                        done_state.select(Some(done_count - 1));
                    }
                    frame.render_stateful_widget(done_widget, sections[1], &mut done_state);
                } else {
                    // Highlight pending section
                    let mut pending_state = ListState::default();
                    let pending_count = self.items[..=selected_idx]
                        .iter()
                        .filter(|item| !item.done)
                        .count();
                    if pending_count > 0 {
                        pending_state.select(Some(pending_count - 1));
                    }
                    frame.render_stateful_widget(pending_widget, sections[0], &mut pending_state);
                    frame.render_widget(done_widget, sections[1]);
                }
            } else {
                // No valid selection, render both without highlighting
                frame.render_widget(pending_widget, sections[0]);
                frame.render_widget(done_widget, sections[1]);
            }
        } else {
            // No selection, render both without highlighting
            frame.render_widget(pending_widget, sections[0]);
            frame.render_widget(done_widget, sections[1]);
        }

        let help_widget = Paragraph::new(HELP_TEXT).block(Block::default().borders(Borders::TOP));
        frame.render_widget(help_widget, help_area);
    }

    fn display_text(&self, index: usize) -> String {
        let todo = &self.items[index];
        let base = if todo.expanded {
            todo.expanded_text()
        } else {
            todo.collapsed_summary()
        };
        let is_selected = Some(index) == self.state.selected();
        let cursor_prefix = if is_selected { "▶ " } else { "  " };
        let checkbox = if todo.selected { "[x] " } else { "[ ] " };

        if let Some((first, rest)) = base.split_once('\n') {
            format!("{}{}{}\n{}", cursor_prefix, checkbox, first, rest)
        } else {
            format!("{}{}{}", cursor_prefix, checkbox, base)
        }
    }

    fn handle_events(&mut self) -> io::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        //dbg!(key_event);
        match key_event.code {
            KEY_QUIT => self.exit(),
            KEY_NEXT_ITEM => self.select_next(),
            KEY_PREVIOUS_ITEM => self.select_previous(),
            KEY_TOGGLE_EXPAND => self.toggle_selected(),
            KEY_TOGGLE_DONE => self.toggle_done(),
            KEY_TOGGLE_SELECT => self.toggle_select(),
            _ => {}
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(i) = self.state.selected() {
            if let Some(item) = self.items.get_mut(i) {
                item.expanded = !item.expanded;
            }
        }
    }

    fn select_next(&mut self) {
        let len = self.items.len();
        if len == 0 {
            return;
        }

        let current = self.state.selected().unwrap_or(0);
        let next = if current + 1 >= len { 0 } else { current + 1 };
        self.state.select(Some(next));
    }

    fn select_previous(&mut self) {
        let len = self.items.len();
        if len == 0 {
            return;
        }

        let current = self.state.selected().unwrap_or(0);
        let previous = if current == 0 { len - 1 } else { current - 1 };
        self.state.select(Some(previous));
    }

    fn toggle_done(&mut self) {
        let selected_indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
            .collect();

        if !selected_indices.is_empty() {
            // If there are selected items, toggle their done status
            for i in selected_indices {
                if let Some(item) = self.items.get_mut(i) {
                    item.done = !item.done;
                    item.selected = false; // Deselect after toggling
                    // Collapse item when marked as done
                    if item.done {
                        item.expanded = false;
                    }
                }
            }
        } else if let Some(cursor_idx) = self.state.selected() {
            // If no items are selected, toggle the item under cursor
            if let Some(item) = self.items.get_mut(cursor_idx) {
                item.done = !item.done;
                // Collapse item when marked as done
                if item.done {
                    item.expanded = false;
                }
            }
        }

        // Update pending count
        self.pending_count = self.items.iter().filter(|item| !item.done).count();
    }

    fn toggle_select(&mut self) {
        if let Some(i) = self.state.selected() {
            if let Some(item) = self.items.get_mut(i) {
                item.selected = !item.selected;
            }
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn load_todos_parses_comments() {
        let todos = load_todos().expect("load TODOs");
        assert_eq!(todos.len(), 6);
        assert_eq!(todos[0].title, "Item 1");
        let comment = todos[0].comment.as_deref().expect("comment for first item");
        assert!(comment.starts_with("This is a comment for item 1."));
        assert!(comment.contains("It can span multiple lines."));
        assert!(!todos[0].expanded);
    }

    #[test]
    fn toggle_selected_via_key_event() {
        let mut state = ListState::default();
        state.select(Some(0));
        let mut app = App {
            exit: false,
            state,
            items: vec![Todo {
                title: String::from("a"),
                comment: Some(String::from("comment")),
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
            }],
            pending_count: 1,
        };

        app.handle_key_event(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(app.items[0].expanded);
        app.handle_key_event(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(!app.items[0].expanded);
    }

    #[test]
    fn collapsed_summary_marks_expandable_items() {
        let with_comment = Todo {
            title: String::from("a"),
            comment: Some(String::from("comment")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(with_comment.collapsed_summary(), "a >");

        let without_comment = Todo {
            title: String::from("b"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(without_comment.collapsed_summary(), "b");
    }

    #[test]
    fn expanded_text_indents_comment() {
        let todo = Todo {
            title: String::from("a"),
            comment: Some(String::from("line1\nline2")),
            expanded: true,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(
            todo.expanded_text(),
            "a >>>\n         line1\n         line2"
        );
    }

    #[test]
    fn display_text_prefixes_cursor() {
        let mut state = ListState::default();
        state.select(Some(0));
        let app = App {
            exit: false,
            state,
            items: vec![
                Todo {
                    title: String::from("a"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: false,
                    due_date: None,
                },
                Todo {
                    title: String::from("b"),
                    comment: Some(String::from("c1\nc2")),
                    expanded: true,
                    done: false,
                    selected: false,
                    due_date: None,
                },
            ],
            pending_count: 2,
        };

        assert_eq!(app.display_text(0), "▶ [ ] a");
        assert_eq!(app.display_text(1), "  [ ] b >>>\n         c1\n         c2");
    }

    #[test]
    fn visual_indicators_for_todo_states() {
        // Test collapsed item with comment (shows 📋)
        let collapsed_with_comment = Todo {
            title: String::from("Task with details"),
            comment: Some(String::from("Some details")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(
            collapsed_with_comment.collapsed_summary(),
            "Task with details >"
        );

        // Test expanded item with comment (shows 📖)
        let expanded_with_comment = Todo {
            title: String::from("Task with details"),
            comment: Some(String::from("Some details")),
            expanded: true,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(
            expanded_with_comment.expanded_text(),
            "Task with details >>>\n         Some details"
        );

        // Test item without comment (no icon)
        let no_comment = Todo {
            title: String::from("Simple task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(no_comment.collapsed_summary(), "Simple task");

        // Test item with empty comment (no icon)
        let empty_comment = Todo {
            title: String::from("Task with empty comment"),
            comment: Some(String::from("   ")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
        };
        assert_eq!(empty_comment.collapsed_summary(), "Task with empty comment");
    }

    #[test]
    fn draw_displays_help_text() {
        use ratatui::{Terminal, backend::TestBackend};

        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App {
            exit: false,
            state: ListState::default(),
            items: Vec::new(),
            pending_count: 0,
        };

        terminal.draw(|f| app.draw(f)).unwrap();

        let buf = terminal.backend().buffer();
        let bottom_y = buf.area.bottom() - 1;
        let line: String = (0..buf.area.width)
            .map(|x| buf[(x, bottom_y)].symbol())
            .collect();

        assert_eq!(line.trim_end(), HELP_TEXT);
    }

    #[test]
    fn toggle_done_via_key_event() {
        let mut state = ListState::default();
        state.select(Some(0));
        let mut app = App {
            exit: false,
            state,
            items: vec![
                Todo {
                    title: String::from("pending task"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: false,
                    due_date: None,
                },
                Todo {
                    title: String::from("done task"),
                    comment: None,
                    expanded: false,
                    done: true,
                    selected: false,
                    due_date: None,
                },
            ],
            pending_count: 1,
        };

        // Toggle first item from pending to done
        app.handle_key_event(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(app.items[0].done);
        assert_eq!(app.pending_count, 0);

        // Toggle back to pending
        app.handle_key_event(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(!app.items[0].done);
        assert_eq!(app.pending_count, 1);
    }

    #[test]
    fn load_todos_handles_done_field() {
        let todos = load_todos().expect("load TODOs");
        assert_eq!(todos.len(), 6);

        // First four items should default to not done
        assert!(!todos[0].done);
        assert!(!todos[1].done);
        assert!(!todos[2].done);
        assert!(!todos[3].done);

        // Fifth item should be marked as done
        assert!(todos[4].done);
        assert_eq!(todos[4].title, "Completed task example");

        // Sixth item should default to not done
        assert!(!todos[5].done);
    }

    #[test]
    fn toggle_done_works_on_selected_items() {
        let mut state = ListState::default();
        state.select(Some(1)); // Cursor on second item
        let mut app = App {
            exit: false,
            state,
            items: vec![
                Todo {
                    title: String::from("task 1"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: true, // Selected
                    due_date: None,
                },
                Todo {
                    title: String::from("task 2"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: false, // Not selected (cursor is here)
                    due_date: None,
                },
                Todo {
                    title: String::from("task 3"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: true, // Selected
                    due_date: None,
                },
            ],
            pending_count: 3,
        };

        // Toggle done - should affect only selected items (0 and 2), not cursor item (1)
        app.handle_key_event(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));

        assert!(app.items[0].done); // Selected item should be marked done
        assert!(!app.items[1].done); // Cursor item should remain unchanged
        assert!(app.items[2].done); // Selected item should be marked done
        assert_eq!(app.pending_count, 1); // Only one pending item left

        // Items should be deselected after toggling
        assert!(!app.items[0].selected);
        assert!(!app.items[1].selected);
        assert!(!app.items[2].selected);
    }

    #[test]
    fn toggle_done_works_on_cursor_when_no_selection() {
        let mut state = ListState::default();
        state.select(Some(1)); // Cursor on second item
        let mut app = App {
            exit: false,
            state,
            items: vec![
                Todo {
                    title: String::from("task 1"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: false, // Not selected
                    due_date: None,
                },
                Todo {
                    title: String::from("task 2"),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: false, // Not selected (cursor is here)
                    due_date: None,
                },
            ],
            pending_count: 2,
        };

        // Toggle done - should affect cursor item since no items are selected
        app.handle_key_event(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));

        assert!(!app.items[0].done); // First item should remain unchanged
        assert!(app.items[1].done); // Cursor item should be marked done
        assert_eq!(app.pending_count, 1); // One pending item left
    }
}
