use std::io;

mod store;
mod ui;

use store::{load_todos, store_todos};
use ui::{App, ExternalEditor};

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let items = load_todos()?;
    let mut app = App::new(items, ExternalEditor);
    let app_result = app.run(&mut terminal);
    ratatui::restore();

    // Save todos when exiting
    if let Err(e) = store_todos(app.items()) {
        eprintln!("Warning: Failed to save todos: {}", e);
    }

    app_result
}
