use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Color, Style},
    widgets::{Block, List, ListState},
};

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app_result = App::default().run(&mut terminal);
    ratatui::restore();
    app_result
}

#[derive(Debug, Default)]
pub struct App {
    exit: bool,
    state: ListState,
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
        let items = vec!["Item 1", "Item 2", "Item 3"];
        let list_widget = List::new(items)
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
