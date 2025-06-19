use std::io;

mod store;
mod ui;

use store::load_todos;
use ui::App;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let items = load_todos()?;
    let mut app = App::new(items);
    let app_result = app.run(&mut terminal);
    ratatui::restore();
    app_result
}
