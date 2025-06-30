use std::{env, fs, io, io::Write, process::Command};

use tempfile::NamedTempFile;

use chrono::{DateTime, Utc};
use log::info;

use crate::ui::Todo;

/// The name of the Google Tasks list used for synchronization
const GOOGLE_TASKS_LIST_NAME: &str = "juggler";

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

    // Get editor from environment or default to emacs
    let editor = env::var("EDITOR").unwrap_or_else(|_| "emacs".to_string());

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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct GoogleTask {
    id: Option<String>,
    title: String,
    notes: Option<String>,
    status: String,
    due: Option<String>,
    updated: Option<String>,
    completed: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GoogleTasksListResponse {
    items: Option<Vec<GoogleTask>>,
}

#[derive(Debug, serde::Deserialize)]
struct GoogleTaskListsResponse {
    items: Option<Vec<GoogleTaskList>>,
}

#[derive(Debug, serde::Deserialize)]
struct GoogleTaskList {
    id: String,
    title: String,
}

/// Helper function to create a new Google Task from a Todo
async fn create_google_task(
    client: &reqwest::Client,
    todo: &mut Todo,
    list_id: &str,
    access_token: &str,
    dry_run: bool,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let new_task = GoogleTask {
        id: None,
        title: format!("j:{}", todo.title),
        notes: todo.comment.clone(),
        status: if todo.done {
            "completed".to_string()
        } else {
            "needsAction".to_string()
        },
        due: todo.due_date.map(|d| d.to_rfc3339()),
        updated: None,
        completed: None,
    };

    let create_url = format!("{base_url}/tasks/v1/lists/{list_id}/tasks");

    info!("Creating Google Task: '{}'", new_task.title);

    if dry_run {
        info!(
            "[DRY RUN] Would create task: {} with status: {}",
            new_task.title, new_task.status
        );
        // In dry run mode, generate a fake ID to keep the sync logic working
        todo.google_task_id = Some(format!("dry-run-id-{}", todo.title.len()));
    } else {
        let response = client
            .post(&create_url)
            .bearer_auth(access_token)
            .json(&new_task)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "Google Tasks API request failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )
            .into());
        }

        let created_task: GoogleTask = response.json().await?;
        todo.google_task_id = created_task.id;
        info!("Created Google Task with ID: {:?}", todo.google_task_id);
    }

    Ok(())
}

pub async fn sync_to_tasks(
    todos: &mut [Todo],
    access_token: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    sync_to_tasks_with_base_url(todos, access_token, dry_run, "https://tasks.googleapis.com").await
}

async fn sync_to_tasks_with_base_url(
    todos: &mut [Todo],
    access_token: &str,
    dry_run: bool,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if dry_run {
        info!("Starting sync in DRY RUN mode - no changes will be made");
    } else {
        info!("Starting sync with Google Tasks");
    }

    let client = reqwest::Client::new();

    // First, find the task list for synchronization
    let tasklists_url = format!("{base_url}/tasks/v1/users/@me/lists");
    let tasklists_response = client
        .get(tasklists_url)
        .bearer_auth(access_token)
        .send()
        .await?;

    if !tasklists_response.status().is_success() {
        return Err(format!(
            "Google Tasks API request failed with status {}: {}",
            tasklists_response.status(),
            tasklists_response.text().await.unwrap_or_default()
        )
        .into());
    }

    let tasklists: GoogleTaskListsResponse = tasklists_response.json().await?;
    let juggler_list = tasklists
        .items
        .unwrap_or_default()
        .into_iter()
        .find(|list| list.title == GOOGLE_TASKS_LIST_NAME)
        .ok_or(format!(
            "No '{GOOGLE_TASKS_LIST_NAME}' task list found in Google Tasks"
        ))?;
    info!("Parent task list ID: {}", juggler_list.id);
    // Get all existing tasks from the sync list
    let tasks_url = format!("{base_url}/tasks/v1/lists/{}/tasks", juggler_list.id);
    let tasks_response = client
        .get(&tasks_url)
        .bearer_auth(access_token)
        .send()
        .await?;

    if !tasks_response.status().is_success() {
        return Err(format!(
            "Google Tasks API request failed with status {}: {}",
            tasks_response.status(),
            tasks_response.text().await.unwrap_or_default()
        )
        .into());
    }

    let google_tasks: GoogleTasksListResponse = tasks_response.json().await?;
    let existing_tasks = google_tasks.items.unwrap_or_default();

    // Create a map of Google Task IDs to Google Tasks for quick lookup
    let mut google_task_map: std::collections::HashMap<String, GoogleTask> = existing_tasks
        .into_iter()
        .filter_map(|task| task.id.clone().map(|id| (id, task)))
        .collect();

    // Process each todo
    for todo in todos.iter_mut() {
        match &todo.google_task_id {
            Some(task_id) => {
                // Todo has a Google Task ID, check if it needs updating
                if let Some(google_task) = google_task_map.remove(task_id) {
                    // Task exists, check if it needs updating
                    let needs_update = google_task.title != format!("j:{}", todo.title)
                        || google_task.notes.as_deref() != todo.comment.as_deref()
                        || (google_task.status == "completed") != todo.done
                        || google_task.due != todo.due_date.map(|d| d.to_rfc3339());

                    if needs_update {
                        // Update the task
                        let updated_task = GoogleTask {
                            id: Some(task_id.clone()),
                            title: format!("j:{}", todo.title),
                            notes: todo.comment.clone(),
                            status: if todo.done {
                                "completed".to_string()
                            } else {
                                "needsAction".to_string()
                            },
                            due: todo.due_date.map(|d| d.to_rfc3339()),
                            updated: None,
                            completed: None,
                        };

                        info!(
                            "Updating Google Task: '{}' (ID: {})",
                            updated_task.title, task_id
                        );

                        if dry_run {
                            info!(
                                "[DRY RUN] Would update task '{}' with status: {}",
                                updated_task.title, updated_task.status
                            );
                        } else {
                            let update_url = format!(
                                "{base_url}/tasks/v1/lists/{}/tasks/{task_id}",
                                juggler_list.id
                            );
                            let response = client
                                .put(&update_url)
                                .bearer_auth(access_token)
                                .json(&updated_task)
                                .send()
                                .await?;

                            if !response.status().is_success() {
                                return Err(format!(
                                    "Google Tasks API request failed with status {}: {}",
                                    response.status(),
                                    response.text().await.unwrap_or_default()
                                )
                                .into());
                            }
                        }
                    }
                } else {
                    // Task was deleted in Google Tasks, recreate it (one-way sync)
                    create_google_task(
                        &client,
                        todo,
                        &juggler_list.id,
                        access_token,
                        dry_run,
                        base_url,
                    )
                    .await?;
                }
            }
            None => {
                // Todo doesn't have a Google Task ID, create a new task
                create_google_task(
                    &client,
                    todo,
                    &juggler_list.id,
                    access_token,
                    dry_run,
                    base_url,
                )
                .await?;
            }
        }
    }

    // Delete any remaining Google Tasks that don't have corresponding todos
    for (task_id, google_task) in google_task_map {
        info!(
            "Deleting orphaned Google Task: '{}' (ID: {})",
            google_task.title, task_id
        );

        if dry_run {
            info!(
                "[DRY RUN] Would delete orphaned task: '{}'",
                google_task.title
            );
        } else {
            let delete_url = format!(
                "{base_url}/tasks/v1/lists/{}/tasks/{task_id}",
                juggler_list.id
            );
            let response = client
                .delete(&delete_url)
                .bearer_auth(access_token)
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(format!(
                    "Google Tasks API request failed with status {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                )
                .into());
            }
            info!("Deleted orphaned Google Task: '{}'", google_task.title);
        }
    }

    if dry_run {
        info!("Sync completed in DRY RUN mode - no actual changes were made");
    } else {
        info!("Sync completed successfully");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_todos_parses_comments() {
        let todos = load_todos("TODOs.yaml").expect("load TODOs");
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
        let todos = load_todos("TODOs.yaml").expect("load TODOs");
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

    // Google Tasks Sync Tests
    mod sync_tests {
        use super::*;
        use wiremock::matchers::{bearer_token, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        #[tokio::test]
        async fn test_sync_successful_create_new_task() {
            let mock_server = MockServer::start().await;

            // Mock the task lists endpoint
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "test_list_id",
                            "title": "juggler"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the existing tasks endpoint (empty list)
            Mock::given(method("GET"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": []
                })))
                .mount(&mock_server)
                .await;

            // Mock the create task endpoint
            Mock::given(method("POST"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "new_task_id",
                    "title": "j:Test Task",
                    "notes": "Test comment",
                    "status": "needsAction"
                })))
                .mount(&mock_server)
                .await;

            let mut todos = vec![Todo {
                title: "Test Task".to_string(),
                comment: Some("Test comment".to_string()),
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            }];

            let result =
                sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_ok());
            assert_eq!(todos[0].google_task_id, Some("new_task_id".to_string()));
        }

        #[tokio::test]
        async fn test_sync_authentication_error() {
            let mock_server = MockServer::start().await;

            // Mock authentication failure
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("invalid_token"))
                .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                    "error": {
                        "code": 401,
                        "message": "Invalid credentials"
                    }
                })))
                .mount(&mock_server)
                .await;

            let mut todos = vec![Todo {
                title: "Test Task".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            }];

            let result =
                sync_to_tasks_with_base_url(&mut todos, "invalid_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("Google Tasks API request failed with status 401"));
        }

        #[tokio::test]
        async fn test_sync_task_list_not_found() {
            let mock_server = MockServer::start().await;

            // Mock task lists endpoint with no "juggler" list
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "other_list_id",
                            "title": "Other List"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            let mut todos = vec![Todo {
                title: "Test Task".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            }];

            let result =
                sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("No 'juggler' task list found in Google Tasks"));
        }

        #[tokio::test]
        async fn test_sync_update_existing_task() {
            let mock_server = MockServer::start().await;

            // Mock the task lists endpoint
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "test_list_id",
                            "title": "juggler"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the existing tasks endpoint with one task
            Mock::given(method("GET"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "existing_task_id",
                            "title": "j:Old Title",
                            "notes": "Old comment",
                            "status": "needsAction"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the update task endpoint
            Mock::given(method("PUT"))
                .and(path("/tasks/v1/lists/test_list_id/tasks/existing_task_id"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "existing_task_id",
                    "title": "j:Updated Title",
                    "notes": "Updated comment",
                    "status": "needsAction"
                })))
                .mount(&mock_server)
                .await;

            let mut todos = vec![Todo {
                title: "Updated Title".to_string(),
                comment: Some("Updated comment".to_string()),
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: Some("existing_task_id".to_string()),
            }];

            let result =
                sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_sync_delete_orphaned_task() {
            let mock_server = MockServer::start().await;

            // Mock the task lists endpoint
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "test_list_id",
                            "title": "juggler"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the existing tasks endpoint with orphaned task
            Mock::given(method("GET"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "orphaned_task_id",
                            "title": "j:Orphaned Task",
                            "notes": "This task has no local counterpart",
                            "status": "needsAction"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the delete task endpoint
            Mock::given(method("DELETE"))
                .and(path("/tasks/v1/lists/test_list_id/tasks/orphaned_task_id"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&mock_server)
                .await;

            let mut todos = vec![]; // No local todos

            let result =
                sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_sync_dry_run_mode() {
            let mock_server = MockServer::start().await;

            // Mock the task lists endpoint
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "test_list_id",
                            "title": "juggler"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the existing tasks endpoint (empty list)
            Mock::given(method("GET"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": []
                })))
                .mount(&mock_server)
                .await;

            // No POST/PUT/DELETE mocks because dry-run shouldn't make those calls

            let mut todos = vec![Todo {
                title: "Test Task".to_string(),
                comment: Some("Test comment".to_string()),
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            }];

            let result = sync_to_tasks_with_base_url(
                &mut todos,
                "test_token",
                true, // dry_run = true
                &mock_server.uri(),
            )
            .await;

            assert!(result.is_ok());
            // In dry run mode, a fake ID should be assigned
            assert!(todos[0].google_task_id.is_some());
            assert!(
                todos[0]
                    .google_task_id
                    .as_ref()
                    .unwrap()
                    .starts_with("dry-run-id-")
            );
        }

        #[tokio::test]
        async fn test_sync_with_due_dates() {
            let mock_server = MockServer::start().await;
            let test_due_date = chrono::Utc::now() + chrono::Duration::days(1);

            // Mock the task lists endpoint
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "test_list_id",
                            "title": "juggler"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the existing tasks endpoint (empty list)
            Mock::given(method("GET"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": []
                })))
                .mount(&mock_server)
                .await;

            // Mock the create task endpoint
            Mock::given(method("POST"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "new_task_id",
                    "title": "j:Task with Due Date",
                    "status": "needsAction",
                    "due": test_due_date.to_rfc3339()
                })))
                .mount(&mock_server)
                .await;

            let mut todos = vec![Todo {
                title: "Task with Due Date".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: Some(test_due_date),
                google_task_id: None,
            }];

            let result =
                sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_ok());
            assert_eq!(todos[0].google_task_id, Some("new_task_id".to_string()));
        }

        #[tokio::test]
        async fn test_sync_completed_task() {
            let mock_server = MockServer::start().await;

            // Mock the task lists endpoint
            Mock::given(method("GET"))
                .and(path("/tasks/v1/users/@me/lists"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {
                            "id": "test_list_id",
                            "title": "juggler"
                        }
                    ]
                })))
                .mount(&mock_server)
                .await;

            // Mock the existing tasks endpoint (empty list)
            Mock::given(method("GET"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": []
                })))
                .mount(&mock_server)
                .await;

            // Mock the create task endpoint
            Mock::given(method("POST"))
                .and(path("/tasks/v1/lists/test_list_id/tasks"))
                .and(bearer_token("test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "completed_task_id",
                    "title": "j:Completed Task",
                    "status": "completed"
                })))
                .mount(&mock_server)
                .await;

            let mut todos = vec![Todo {
                title: "Completed Task".to_string(),
                comment: None,
                expanded: false,
                done: true, // Task is completed
                selected: false,
                due_date: None,
                google_task_id: None,
            }];

            let result =
                sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri())
                    .await;

            assert!(result.is_ok());
            assert_eq!(
                todos[0].google_task_id,
                Some("completed_task_id".to_string())
            );
        }
    }
}
