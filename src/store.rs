use std::{env, fs, io, io::Write, process::Command};

use tempfile::NamedTempFile;

use chrono::{DateTime, Utc};

use crate::ui::Todo;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TodoItem {
    pub title: String,
    pub comment: Option<String>,
    #[serde(default)]
    pub done: bool,
    pub due_date: Option<DateTime<Utc>>,
}

pub fn load_todos() -> io::Result<Vec<Todo>> {
    let content = fs::read_to_string("TODOs.yaml")?;
    let items: Vec<TodoItem> = serde_yaml::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut todos: Vec<Todo> = items.into_iter().map(|item| item.into()).collect();

    // Sort by due date in ascending order
    // Items without due dates go to the end
    todos.sort_by_key(|todo| todo.due_date.unwrap_or(DateTime::<Utc>::MAX_UTC));

    Ok(todos)
}

pub fn edit_todo_item(todo: &Todo) -> io::Result<Todo> {
    // Convert Todo to TodoItem for serialization
    let todo_item = TodoItem {
        title: todo.title.clone(),
        comment: todo.comment.clone(),
        done: todo.done,
        due_date: todo.due_date,
    };

    // Serialize to YAML
    let yaml_content = serde_yaml::to_string(&todo_item)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Create secure temporary file with .yaml extension
    let mut temp_file = NamedTempFile::with_suffix(".yaml")?;
    temp_file.write_all(yaml_content.as_bytes())?;
    temp_file.flush()?;

    // Get the path to the temp file
    let temp_path = temp_file.path();

    // Get editor from environment or default to emacs
    let editor = env::var("EDITOR").unwrap_or_else(|_| "emacs".to_string());

    // Launch editor
    let status = Command::new(&editor).arg(temp_path).status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "Editor {} exited with non-zero status",
            editor
        )));
    }

    // Read back the modified content
    let modified_content = fs::read_to_string(temp_path)?;

    // Temp file is automatically cleaned up when temp_file goes out of scope

    // Parse the modified YAML
    let modified_item: TodoItem = serde_yaml::from_str(&modified_content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Convert back to Todo, preserving UI state
    let mut updated_todo: Todo = modified_item.into();
    updated_todo.expanded = todo.expanded;
    updated_todo.selected = todo.selected;

    Ok(updated_todo)
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

    #[test]
    fn todo_item_serialization() {
        let item = TodoItem {
            title: "Test item".to_string(),
            comment: Some("Test comment".to_string()),
            done: false,
            due_date: None,
        };

        let yaml = serde_yaml::to_string(&item).expect("serialize to YAML");
        assert!(yaml.contains("title: Test item"));
        assert!(yaml.contains("comment: Test comment"));
        // All fields should now be present
        assert!(yaml.contains("done: false"));
        assert!(yaml.contains("due_date: null"));
    }

    #[test]
    fn todo_item_deserialization() {
        let yaml = r#"
title: "Test item"
comment: "Test comment"
"#;

        let item: TodoItem = serde_yaml::from_str(yaml).expect("deserialize from YAML");
        assert_eq!(item.title, "Test item");
        assert_eq!(item.comment, Some("Test comment".to_string()));
        assert!(!item.done); // Should default to false
        assert!(item.due_date.is_none());
    }
}
