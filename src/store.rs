use std::{env, fs, io, io::Write, process::Command};

use tempfile::NamedTempFile;

use chrono::{DateTime, Utc};

#[cfg(test)]
use crate::config::DEFAULT_TODOS_FILE;
use crate::config::{
    DEFAULT_EDITOR
};
use crate::ui::Todo;

/// Configuration constants are centralized in the `config` module

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TodoItem {
    pub title: String,
    pub comment: Option<String>,
    #[serde(default)]
    pub done: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub google_task_id: Option<String>,
}

pub fn load_todos<P: AsRef<std::path::Path>>(file_path: P) -> io::Result<Vec<Todo>> {
    let content = match fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(e) if e.kind() == io::ErrorKind::NotFound => "[]".to_string(),
        Err(e) => return Err(e),
    };

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
        google_task_id: todo.google_task_id.clone(),
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

    // Get editor from environment or default to configured value
    let editor = env::var("EDITOR").unwrap_or_else(|_| DEFAULT_EDITOR.to_string());

    // Launch editor
    let status = Command::new(&editor).arg(temp_path).status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "Editor {editor} exited with non-zero status"
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

pub fn store_todos<P: AsRef<std::path::Path>>(todos: &[Todo], file_path: P) -> io::Result<()> {
    let file_path = file_path.as_ref();

    // Convert Todo items to TodoItem for serialization
    let todo_items: Vec<TodoItem> = todos
        .iter()
        .map(|todo| TodoItem {
            title: todo.title.clone(),
            comment: todo.comment.clone(),
            done: todo.done,
            due_date: todo.due_date,
            google_task_id: todo.google_task_id.clone(),
        })
        .collect();

    // Serialize to YAML
    let yaml_content = serde_yaml::to_string(&todo_items)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Get the directory of the target file for temporary file creation
    let target_dir = file_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let temp_file = NamedTempFile::new_in(target_dir)?;
    let temp_path = temp_file.path();

    // Write content to temp file
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(temp_path)?;

    file.write_all(yaml_content.as_bytes())?;

    // Ensure data is written to disk before rename
    file.flush()?;
    file.sync_all()?;

    // Close the file explicitly
    drop(file);

    // Atomically replace the original file
    fs::rename(temp_path, file_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_todos_parses_comments() {
        let todos = load_todos(DEFAULT_TODOS_FILE).expect("load TODOs");
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
        let todos = load_todos(DEFAULT_TODOS_FILE).expect("load TODOs");
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
            google_task_id: None,
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

    #[test]
    fn store_todos_roundtrip() {
        use tempfile::TempDir;

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("test_todos.yaml");

        // Create test todos
        let test_todos = vec![
            Todo {
                title: "Test todo 1".to_string(),
                comment: Some("Test comment 1".to_string()),
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: "Test todo 2".to_string(),
                comment: None,
                expanded: false,
                done: true,
                selected: false,
                due_date: Some(chrono::Utc::now()),
                google_task_id: Some("google_task_123".to_string()),
            },
        ];

        // Store the todos
        store_todos(&test_todos, &test_file).expect("store todos");

        // Verify the file was created
        assert!(test_file.exists());

        // Load them back
        let loaded_todos = load_todos(&test_file).expect("load todos");

        // Verify they match (accounting for sorting by due date)
        assert_eq!(loaded_todos.len(), test_todos.len());

        // Find todos by title since order may change due to sorting
        let loaded_todo1 = loaded_todos
            .iter()
            .find(|t| t.title == "Test todo 1")
            .expect("find todo 1");
        let loaded_todo2 = loaded_todos
            .iter()
            .find(|t| t.title == "Test todo 2")
            .expect("find todo 2");

        assert_eq!(loaded_todo1.comment, Some("Test comment 1".to_string()));
        assert!(!loaded_todo1.done);
        assert!(loaded_todo1.due_date.is_none());

        assert_eq!(loaded_todo2.comment, None);
        assert!(loaded_todo2.done);
        assert!(loaded_todo2.due_date.is_some());
    }

    #[test]
    fn load_todos_handles_missing_file() {
        use tempfile::TempDir;

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("create temp dir");
        let non_existent_file = temp_dir.path().join("non_existent_todos.yaml");

        // Try to load from a non-existent file
        let todos = load_todos(&non_existent_file).expect("load todos from non-existent file");

        // Should return empty vector
        assert_eq!(todos.len(), 0);
    }
}
