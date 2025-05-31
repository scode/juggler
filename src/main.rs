use std::{fs, io};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Color, Style},
    text::Text,
    widgets::{Block, List, ListState},
};

#[derive(Debug, serde::Deserialize)]
struct TodoConfig {
    title: String,
    comment: Option<String>,
}

#[derive(Debug, Clone)]
struct Todo {
    title: String,
    comment: Option<String>,
    expanded: bool,
}

impl Todo {
    fn collapsed_summary(&self) -> String {
        let mut text = self.title.clone();
        if self
            .comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
        {
            text.push('>');
        }
        text
    }

    fn expanded_text(&self) -> String {
        let mut text = self.title.clone();
        if let Some(comment) = &self.comment {
            text.push('\n');
            let indented = comment
                .lines()
                .map(|line| format!("   {}", line))
                .collect::<Vec<_>>()
                .join("\n");
            text.push_str(&indented);
        }
        text
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
        })
        .collect())
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app_result = App::default().run(&mut terminal);
    ratatui::restore();
    app_result
}

#[derive(Debug)]
pub struct App {
    exit: bool,
    state: ListState,
    items: Vec<Todo>,
}

impl Default for App {
    fn default() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        let items = load_todos().unwrap_or_default();
        App {
            exit: false,
            state,
            items,
        }
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
        let list_items = (0..self.items.len())
            .map(|i| ratatui::widgets::ListItem::new(Text::from(self.display_text(i))))
            .collect::<Vec<_>>();
        let list_widget = List::new(list_items)
            .block(Block::default().title("TODOs"))
            .highlight_style(Style::default().fg(Color::Yellow));
        frame.render_stateful_widget(list_widget, frame.area(), &mut self.state);
    }

    fn display_text(&self, index: usize) -> String {
        let todo = &self.items[index];
        let base = if todo.expanded {
            todo.expanded_text()
        } else {
            todo.collapsed_summary()
        };
        let prefix = if Some(index) == self.state.selected() {
            ">> "
        } else {
            "   "
        };
        format!("{}{}", prefix, base)
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
            KeyCode::Char('q') => self.exit(),
            KeyCode::Down => self.state.select_next(),
            KeyCode::Up => self.state.select_previous(),
            KeyCode::Char('j') => self.state.select_next(),
            KeyCode::Char('k') => self.state.select_previous(),
            KeyCode::Char('o') => self.toggle_selected(),
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
        assert_eq!(todos.len(), 3);
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
            }],
        };

        app.handle_key_event(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert!(app.items[0].expanded);
        app.handle_key_event(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert!(!app.items[0].expanded);
    }

    #[test]
    fn collapsed_summary_marks_expandable_items() {
        let with_comment = Todo {
            title: String::from("a"),
            comment: Some(String::from("comment")),
            expanded: false,
        };
        assert_eq!(with_comment.collapsed_summary(), "a>");

        let without_comment = Todo {
            title: String::from("b"),
            comment: None,
            expanded: false,
        };
        assert_eq!(without_comment.collapsed_summary(), "b");
    }

    #[test]
    fn expanded_text_indents_comment() {
        let todo = Todo {
            title: String::from("a"),
            comment: Some(String::from("line1\nline2")),
            expanded: true,
        };
        assert_eq!(todo.expanded_text(), "a\n   line1\n   line2");
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
                },
                Todo {
                    title: String::from("b"),
                    comment: None,
                    expanded: false,
                },
            ],
        };

        assert_eq!(app.display_text(0), ">> a");
        assert_eq!(app.display_text(1), "   b");
    }
}
