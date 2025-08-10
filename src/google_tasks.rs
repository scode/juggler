use chrono::Utc;
use log::info;

use crate::config::{GOOGLE_OAUTH_TOKEN_URL, GOOGLE_TASKS_BASE_URL, GOOGLE_TASKS_LIST_NAME};
use crate::ui::Todo;

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

#[derive(Debug, serde::Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct GoogleOAuthCredentials {
    pub client_id: String,
    pub refresh_token: String,
}

pub struct GoogleOAuthClient {
    credentials: GoogleOAuthCredentials,
    client: reqwest::Client,
    cached_access_token: Option<String>,
    token_expires_at: Option<chrono::DateTime<Utc>>,
    oauth_token_url: String,
}

impl GoogleOAuthClient {
    pub fn new(credentials: GoogleOAuthCredentials) -> Self {
        Self {
            credentials,
            client: reqwest::Client::new(),
            cached_access_token: None,
            token_expires_at: None,
            oauth_token_url: GOOGLE_OAUTH_TOKEN_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub fn new_with_custom_oauth_url(
        credentials: GoogleOAuthCredentials,
        oauth_token_url: String,
    ) -> Self {
        Self {
            credentials,
            client: reqwest::Client::new(),
            cached_access_token: None,
            token_expires_at: None,
            oauth_token_url,
        }
    }

    pub async fn get_access_token(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        // Check if we have a valid cached token
        if let (Some(token), Some(expires_at)) = (&self.cached_access_token, &self.token_expires_at)
            && Utc::now() < *expires_at - chrono::Duration::minutes(5)
        {
            return Ok(token.clone());
        }

        // Refresh the token
        self.refresh_access_token().await
    }

    async fn refresh_access_token(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let token_url = &self.oauth_token_url;

        // Check for JUGGLER_CLIENT_SECRET environment variable as a workaround
        let client_secret = std::env::var("JUGGLER_CLIENT_SECRET").ok();

        let params = if let Some(secret) = &client_secret {
            info!(
                "Using client_secret from JUGGLER_CLIENT_SECRET environment variable for token refresh"
            );
            vec![
                ("client_id", self.credentials.client_id.as_str()),
                ("refresh_token", self.credentials.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
                ("client_secret", secret.as_str()),
            ]
        } else {
            info!("No client_secret - using public client token refresh");
            vec![
                ("client_id", self.credentials.client_id.as_str()),
                ("refresh_token", self.credentials.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ]
        };

        let response = self.client.post(token_url).form(&params).send().await?;

        if !response.status().is_success() {
            return Err(format!(
                "OAuth token refresh failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )
            .into());
        }

        let token_response: OAuthTokenResponse = response.json().await?;

        // Cache the new token
        self.cached_access_token = Some(token_response.access_token.clone());
        self.token_expires_at = Some(
            Utc::now()
                + chrono::Duration::seconds(token_response.expires_in.unwrap_or(3600) as i64),
        );

        Ok(token_response.access_token)
    }
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
    sync_to_tasks_with_base_url(todos, access_token, dry_run, GOOGLE_TASKS_BASE_URL).await
}

pub async fn sync_to_tasks_with_oauth(
    todos: &mut [Todo],
    oauth_client: GoogleOAuthClient,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    sync_to_tasks_with_oauth_and_base_url(todos, oauth_client, dry_run, GOOGLE_TASKS_BASE_URL).await
}

pub async fn sync_to_tasks_with_oauth_and_base_url(
    todos: &mut [Todo],
    mut oauth_client: GoogleOAuthClient,
    dry_run: bool,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let access_token = oauth_client.get_access_token().await?;
    sync_to_tasks_with_base_url(todos, &access_token, dry_run, base_url).await
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
    /// Any test that relies on a real client id would by definition be buggy
    /// unless it is an integration test meant to exercise the real oauth flow,
    /// so use a fake test id here.
    const GOOGLE_OAUTH_CLIENT_ID: &str = "test-client-id";
    use super::*;
    use wiremock::matchers::{bearer_token, body_string_contains, method, path};
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
            sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri()).await;

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
            sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri()).await;

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
            sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri()).await;

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
            sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri()).await;

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
            sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri()).await;

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
            sync_to_tasks_with_base_url(&mut todos, "test_token", false, &mock_server.uri()).await;

        assert!(result.is_ok());
        assert_eq!(
            todos[0].google_task_id,
            Some("completed_task_id".to_string())
        );
    }

    // OAuth Tests
    #[tokio::test]
    async fn test_oauth_client_token_refresh() {
        let mock_server = MockServer::start().await;

        // Mock OAuth token endpoint
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains(format!(
                "client_id={GOOGLE_OAUTH_CLIENT_ID}"
            )))
            .and(body_string_contains("refresh_token=test_refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new_access_token",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .mount(&mock_server)
            .await;

        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", mock_server.uri());
        let mut oauth_client =
            GoogleOAuthClient::new_with_custom_oauth_url(credentials, oauth_token_url);

        // Test initial state
        assert!(oauth_client.cached_access_token.is_none());
        assert!(oauth_client.token_expires_at.is_none());

        // Test token refresh
        let token = oauth_client.get_access_token().await.unwrap();
        assert_eq!(token, "new_access_token");
        assert!(oauth_client.cached_access_token.is_some());
        assert!(oauth_client.token_expires_at.is_some());
    }

    #[tokio::test]
    async fn test_oauth_client_token_caching() {
        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let mut oauth_client = GoogleOAuthClient::new(credentials);

        // Manually set a cached token that's still valid
        oauth_client.cached_access_token = Some("cached_token".to_string());
        oauth_client.token_expires_at = Some(Utc::now() + chrono::Duration::hours(1));

        // This should return the cached token without making a network request
        let token = oauth_client.get_access_token().await.unwrap();
        assert_eq!(token, "cached_token");
    }

    #[tokio::test]
    async fn test_sync_with_oauth_success() {
        let mock_server = MockServer::start().await;
        let oauth_mock_server = MockServer::start().await;

        // Mock OAuth token endpoint
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains(format!(
                "client_id={GOOGLE_OAUTH_CLIENT_ID}"
            )))
            .and(body_string_contains("refresh_token=test_refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "oauth_access_token",
                "expires_in": 3600
            })))
            .mount(&oauth_mock_server)
            .await;

        // Mock the task lists endpoint
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("oauth_access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "oauth_list_id",
                        "title": "juggler"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Mock the existing tasks endpoint (empty list)
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/oauth_list_id/tasks"))
            .and(bearer_token("oauth_access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []
            })))
            .mount(&mock_server)
            .await;

        // Mock the create task endpoint
        Mock::given(method("POST"))
            .and(path("/tasks/v1/lists/oauth_list_id/tasks"))
            .and(bearer_token("oauth_access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "oauth_task_id",
                "title": "j:OAuth Test Task",
                "status": "needsAction"
            })))
            .mount(&mock_server)
            .await;

        let mut todos = vec![Todo {
            title: "OAuth Test Task".to_string(),
            comment: Some("OAuth comment".to_string()),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", oauth_mock_server.uri());
        let oauth_client =
            GoogleOAuthClient::new_with_custom_oauth_url(credentials, oauth_token_url);

        let result = sync_to_tasks_with_oauth_and_base_url(
            &mut todos,
            oauth_client,
            false,
            &mock_server.uri(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(todos[0].google_task_id, Some("oauth_task_id".to_string()));
    }

    #[tokio::test]
    async fn test_sync_with_oauth_dry_run() {
        let mock_server = MockServer::start().await;
        let oauth_mock_server = MockServer::start().await;

        // Mock OAuth token endpoint
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "oauth_access_token_dry_run",
                "expires_in": 3600
            })))
            .mount(&oauth_mock_server)
            .await;

        // Mock the task lists endpoint
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("oauth_access_token_dry_run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "dry_run_list_id",
                        "title": "juggler"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Mock the existing tasks endpoint (empty list)
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/dry_run_list_id/tasks"))
            .and(bearer_token("oauth_access_token_dry_run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []
            })))
            .mount(&mock_server)
            .await;

        // No POST/PUT/DELETE mocks because dry-run shouldn't make those calls

        let mut todos = vec![Todo {
            title: "Dry Run OAuth Test".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", oauth_mock_server.uri());
        let oauth_client =
            GoogleOAuthClient::new_with_custom_oauth_url(credentials, oauth_token_url);

        let result = sync_to_tasks_with_oauth_and_base_url(
            &mut todos,
            oauth_client,
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
    async fn test_sync_with_oauth_update_existing_task() {
        let mock_server = MockServer::start().await;
        let oauth_mock_server = MockServer::start().await;

        // Mock OAuth token endpoint
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "oauth_update_token",
                "expires_in": 3600
            })))
            .mount(&oauth_mock_server)
            .await;

        // Mock the task lists endpoint
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("oauth_update_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "update_list_id",
                        "title": "juggler"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Mock the existing tasks endpoint with one task
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/update_list_id/tasks"))
            .and(bearer_token("oauth_update_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "existing_oauth_task_id",
                        "title": "j:Old OAuth Title",
                        "notes": "Old comment",
                        "status": "needsAction"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Mock the update task endpoint
        Mock::given(method("PUT"))
            .and(path(
                "/tasks/v1/lists/update_list_id/tasks/existing_oauth_task_id",
            ))
            .and(bearer_token("oauth_update_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "existing_oauth_task_id",
                "title": "j:Updated OAuth Title",
                "notes": "Updated comment",
                "status": "needsAction"
            })))
            .mount(&mock_server)
            .await;

        let mut todos = vec![Todo {
            title: "Updated OAuth Title".to_string(),
            comment: Some("Updated comment".to_string()),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: Some("existing_oauth_task_id".to_string()),
        }];

        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", oauth_mock_server.uri());
        let oauth_client =
            GoogleOAuthClient::new_with_custom_oauth_url(credentials, oauth_token_url);

        let result = sync_to_tasks_with_oauth_and_base_url(
            &mut todos,
            oauth_client,
            false,
            &mock_server.uri(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(
            todos[0].google_task_id,
            Some("existing_oauth_task_id".to_string())
        );
    }

    #[tokio::test]
    async fn test_sync_with_oauth_authentication_failure() {
        let oauth_mock_server = MockServer::start().await;

        // Mock OAuth token endpoint with failure
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "The provided authorization grant is invalid"
            })))
            .mount(&oauth_mock_server)
            .await;

        let mut todos = vec![Todo {
            title: "OAuth Failure Test".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let credentials = GoogleOAuthCredentials {
            client_id: "invalid_client_id".to_string(),
            refresh_token: "invalid_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", oauth_mock_server.uri());
        let oauth_client =
            GoogleOAuthClient::new_with_custom_oauth_url(credentials, oauth_token_url);

        let result = sync_to_tasks_with_oauth_and_base_url(
            &mut todos,
            oauth_client,
            false,
            GOOGLE_TASKS_BASE_URL, // Won't be reached due to OAuth failure
        )
        .await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("OAuth token refresh failed with status 401"));
    }

    #[tokio::test]
    async fn test_oauth_token_refresh_failure() {
        let mock_server = MockServer::start().await;

        // Mock OAuth token endpoint with failure
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "The provided authorization grant is invalid"
            })))
            .mount(&mock_server)
            .await;

        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "invalid_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", mock_server.uri());
        let mut oauth_client =
            GoogleOAuthClient::new_with_custom_oauth_url(credentials, oauth_token_url);

        // Test that token refresh failure is handled properly
        let result = oauth_client.get_access_token().await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("OAuth token refresh failed with status 400"));
    }

    #[tokio::test]
    async fn test_oauth_credentials_structure() {
        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        // Test that credentials are properly stored
        assert_eq!(credentials.client_id, GOOGLE_OAUTH_CLIENT_ID);
        assert_eq!(credentials.refresh_token, "test_refresh_token");

        // Test that the credentials can be cloned
        let cloned_credentials = credentials.clone();
        assert_eq!(cloned_credentials.client_id, credentials.client_id);
        assert_eq!(cloned_credentials.refresh_token, credentials.refresh_token);
    }

    #[tokio::test]
    async fn test_oauth_client_initialization() {
        let credentials = GoogleOAuthCredentials {
            client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_client = GoogleOAuthClient::new(credentials.clone());

        // Test initial state
        assert_eq!(oauth_client.credentials.client_id, credentials.client_id);
        assert_eq!(
            oauth_client.credentials.refresh_token,
            credentials.refresh_token
        );
        assert!(oauth_client.cached_access_token.is_none());
        assert!(oauth_client.token_expires_at.is_none());
        assert_eq!(oauth_client.oauth_token_url, GOOGLE_OAUTH_TOKEN_URL);
    }
}