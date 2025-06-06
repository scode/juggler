use std::{fs, io};

use chrono::{DateTime, Utc};

mod store;
mod ui;

use store::TodoItem;
use ui::{App, Todo};

fn load_todos() -> io::Result<Vec<Todo>> {
    let content = fs::read_to_string("TODOs.yaml")?;
    let items: Vec<TodoItem> = serde_yaml::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut todos: Vec<Todo> = items.into_iter().map(|item| item.into()).collect();

    // Sort by due date in ascending order
    // Items without due dates go to the end
    todos.sort_by_key(|todo| todo.due_date.unwrap_or(DateTime::<Utc>::MAX_UTC));

    Ok(todos)
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let items = load_todos()?;
    let mut app = App::new(items);
    let app_result = app.run(&mut terminal);
    ratatui::restore();
    app_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_todos_parses_comments() {
        let todos = load_todos().expect("load TODOs");
        assert_eq!(todos.len(), 6);
        // After sorting, Item 1 is now at index 3 (2031 due date)
        assert_eq!(todos[3].title, "Item 1");
        let comment = todos[3].comment.as_deref().expect("comment for Item 1");
        assert!(comment.starts_with("This is a comment for item 1."));
        assert!(comment.contains("It can span multiple lines."));
        assert!(!todos[3].expanded);
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
}
