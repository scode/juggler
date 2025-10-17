#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, io::Write};

use tempfile::NamedTempFile;

use chrono::{DateTime, Utc};

use crate::error::{JugglerError, Result};
use crate::time::{Clock, SharedClock, system_clock};
use crate::ui::Todo;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TodoItem {
    pub title: String,
    pub comment: Option<String>,
    #[serde(default)]
    pub done: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub google_task_id: Option<String>,
}

pub fn load_todos<P: AsRef<std::path::Path>>(file_path: P) -> Result<Vec<Todo>> {
    let content = match fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "[]".to_string(),
        Err(e) => return Err(e.into()),
    };

    let items: Vec<TodoItem> = serde_yaml::from_str(&content)?;
    let todos: Vec<Todo> = items.into_iter().map(|item| item.into()).collect();

    Ok(todos)
}

pub fn store_todos_with_clock<P: AsRef<std::path::Path>>(
    todos: &[Todo],
    file_path: P,
    clock: SharedClock,
) -> Result<()> {
    let file_path = file_path.as_ref();

    if let Some(parent) = file_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(parent)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(parent, perms)?;
        }
    }

    if file_path.exists() {
        archive_todos_file(file_path, clock.as_ref())?;
    }

    let mut todo_items: Vec<TodoItem> = todos
        .iter()
        .map(|todo| TodoItem {
            title: todo.title.clone(),
            comment: todo.comment.clone(),
            done: todo.done,
            due_date: todo.due_date,
            google_task_id: todo.google_task_id.clone(),
        })
        .collect();

    // Use a deterministic order to optimize the user experience when
    // using "diff -u" on the store manually.
    todo_items.sort_by(|a, b| match (&a.google_task_id, &b.google_task_id) {
        (Some(id_a), Some(id_b)) => id_a.cmp(id_b).then_with(|| a.title.cmp(&b.title)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.title.cmp(&b.title),
    });

    let yaml_content = serde_yaml::to_string(&todo_items)?;

    let target_dir = file_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut temp_file = NamedTempFile::new_in(target_dir)?;

    {
        let file = temp_file.as_file_mut();
        file.write_all(yaml_content.as_bytes())?;

        file.flush()?;
        file.sync_all()?;
    }

    temp_file
        .persist(file_path)
        .map_err(|e| JugglerError::Io(e.error))?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(file_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(file_path, perms)?;
    }

    Ok(())
}

pub fn store_todos<P: AsRef<std::path::Path>>(todos: &[Todo], file_path: P) -> Result<()> {
    store_todos_with_clock(todos, file_path, system_clock())
}

fn archive_todos_file(file_path: &std::path::Path, clock: &dyn Clock) -> Result<()> {
    let parent = file_path
        .parent()
        .ok_or_else(|| JugglerError::Other("File path has no parent directory".to_string()))?;

    let now = clock.now();
    let timestamp_str = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let archive_name = format!("TODOs_{timestamp_str}.yaml");
    let archive_path = parent.join(archive_name);

    fs::copy(file_path, archive_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::fixed_clock;

    const TEST_TODOS_FILE: &str = "TODOs.yaml";

    #[test]
    fn load_todos_parses_comments() {
        let todos = load_todos(TEST_TODOS_FILE).expect("load TODOs");
        assert_eq!(todos.len(), 6);
        let item1 = todos.iter().find(|t| t.title == "Item 1").expect("Item 1");
        let comment = item1.comment.as_deref().expect("comment for Item 1");
        assert!(comment.starts_with("This is a comment for item 1."));
        assert!(comment.contains("It can span multiple lines."));
        assert!(!item1.expanded);
    }

    #[test]
    fn load_todos_handles_done_field() {
        let todos = load_todos(TEST_TODOS_FILE).expect("load TODOs");
        assert_eq!(todos.len(), 6);

        let completed = todos
            .iter()
            .find(|t| t.title == "Completed task example")
            .expect("Completed task example");
        assert!(completed.done);

        // All other items should default to not done
        for todo in &todos {
            if todo.title != "Completed task example" {
                assert!(!todo.done);
            }
        }
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

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("test_todos.yaml");

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
                due_date: Some(
                    chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                ),
                google_task_id: Some("google_task_123".to_string()),
            },
        ];

        store_todos(&test_todos, &test_file).expect("store todos");
        assert!(test_file.exists());

        let loaded_todos = load_todos(&test_file).expect("load todos");
        assert_eq!(loaded_todos.len(), test_todos.len());

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
    fn store_todos_creates_archive() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("test_todos.yaml");

        let initial_todos = vec![Todo {
            title: "Initial todo".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = fixed_clock(fixed_now);

        store_todos_with_clock(&initial_todos, &test_file, clock.clone())
            .expect("store initial todos");
        assert!(test_file.exists());

        let updated_todos = vec![Todo {
            title: "Updated todo".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        store_todos_with_clock(&updated_todos, &test_file, clock.clone())
            .expect("store updated todos");
        assert!(test_file.exists());

        let expected_archive = format!("TODOs_{}.yaml", fixed_now.format("%Y-%m-%dT%H-%M-%S"));
        let archive_path = temp_dir.path().join(&expected_archive);
        assert!(archive_path.exists());

        let archived_todos = load_todos(&archive_path).expect("load archived todos");
        assert_eq!(archived_todos.len(), 1);
        assert_eq!(archived_todos[0].title, "Initial todo");

        let current_todos = load_todos(&test_file).expect("load current todos");
        assert_eq!(current_todos.len(), 1);
        assert_eq!(current_todos[0].title, "Updated todo");
    }

    #[cfg(unix)]
    #[test]
    fn store_todos_sets_permissions_unix() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        // Create a temporary directory and a nested target path so the code
        // creates the parent directory with restricted permissions.
        let temp_dir = TempDir::new().expect("create temp dir");
        let nested_dir = temp_dir.path().join("nested");
        let test_file = nested_dir.join("perms_todos.yaml");

        let todos = vec![Todo {
            title: "Perms todo".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        // Store the todos (this will create the parent directory if missing)
        store_todos(&todos, &test_file).expect("store todos");
        assert!(test_file.exists());
        let file_mode = fs::metadata(&test_file)
            .expect("file metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            file_mode, 0o600,
            "expected file mode 0600, got {:o}",
            file_mode
        );

        let dir_mode = fs::metadata(&nested_dir)
            .expect("dir metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            dir_mode, 0o700,
            "expected dir mode 0700, got {:o}",
            dir_mode
        );
    }
    #[test]
    fn load_todos_handles_missing_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let non_existent_file = temp_dir.path().join("non_existent_todos.yaml");

        let todos = load_todos(&non_existent_file).expect("load todos from non-existent file");

        assert_eq!(todos.len(), 0);
    }

    /// We want to ensure we store in a deterministic order to optimize the user experience when
    /// using "diff -u" on the store manually.
    #[test]
    fn store_todos_sorts_by_id_then_title() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("sorted_todos.yaml");

        // Create todos in intentionally unsorted order
        let todos = vec![
            Todo {
                title: "Zebra".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: Some("id_3".to_string()),
            },
            Todo {
                title: "Apple".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: "Banana".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: Some("id_1".to_string()),
            },
            Todo {
                title: "Cherry".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: "Date".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: Some("id_2".to_string()),
            },
        ];

        store_todos(&todos, &test_file).expect("store todos");

        let loaded = load_todos(&test_file).expect("load todos");
        assert_eq!(loaded[0].title, "Banana"); // id_1
        assert_eq!(loaded[1].title, "Date"); // id_2
        assert_eq!(loaded[2].title, "Zebra"); // id_3
        assert_eq!(loaded[3].title, "Apple"); // no ID, alphabetically first
        assert_eq!(loaded[4].title, "Cherry"); // no ID, alphabetically second
    }
}
