use std::{fs, io};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Color, Style},
    widgets::{Block, List, ListState},
};

fn load_todos() -> io::Result<Vec<String>> {
    let content = fs::read_to_string("TODOs.yaml")?;
    let todos: Vec<String> = serde_yaml::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(todos)
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
    items: Vec<String>,
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
            .map(|i| ratatui::widgets::ListItem::new(i.as_str()))
            .collect::<Vec<_>>();
        let list_widget = List::new(list_items)
            .block(Block::default().title("TODOs"))
            .highlight_style(Style::default().fg(Color::Yellow));
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
            _ => {}
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
