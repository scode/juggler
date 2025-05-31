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
        let list_items = self
            .items
            .iter()
            .map(|todo| {
                if todo.expanded {
                    let mut text = todo.title.clone();
                    if let Some(comment) = &todo.comment {
                        text.push('\n');
                        text.push_str(comment);
                    }
                    ratatui::widgets::ListItem::new(Text::from(text))
                } else {
                    ratatui::widgets::ListItem::new(Text::from(todo.title.as_str()))
                }
            })
            .collect::<Vec<_>>();
        let list_widget = List::new(list_items)
            .block(Block::default().title("TODOs"))
            .highlight_style(Style::default().fg(Color::Yellow))
            .repeat_highlight_symbol(true);
        frame.render_stateful_widget(list_widget, frame.area(), &mut self.state);
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

    #[test]
    fn render() {}

    #[test]
    fn handle_key_event() -> io::Result<()> {
        Ok(())
    }
}
