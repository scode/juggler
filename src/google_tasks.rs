use log::info;

use crate::config::{GOOGLE_TASK_TITLE_PREFIX, GOOGLE_TASKS_BASE_URL, GOOGLE_TASKS_LIST_NAME};
use crate::error::{JugglerError, Result};
use crate::ui::Todo;

pub use crate::oauth::{GoogleOAuthClient, GoogleOAuthCredentials};

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
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GoogleTaskListsResponse {
    items: Option<Vec<GoogleTaskList>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GoogleTaskList {
    id: String,
    title: String,
}

trait PaginatedResponse {
    type Item;
    fn into_items(self) -> Option<Vec<Self::Item>>;
    fn next_page_token(&self) -> Option<&String>;
}

impl PaginatedResponse for GoogleTaskListsResponse {
    type Item = GoogleTaskList;
    fn into_items(self) -> Option<Vec<Self::Item>> {
        self.items
    }
    fn next_page_token(&self) -> Option<&String> {
        self.next_page_token.as_ref()
    }
}

impl PaginatedResponse for GoogleTasksListResponse {
    type Item = GoogleTask;
    fn into_items(self) -> Option<Vec<Self::Item>> {
        self.items
    }
    fn next_page_token(&self) -> Option<&String> {
        self.next_page_token.as_ref()
    }
}

// Helper to display Option<String> values in logs
fn display_opt(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("<none>")
}

async fn check_api_response(response: reqwest::Response) -> Result<reqwest::Response> {
    if !response.status().is_success() {
        return Err(JugglerError::google_tasks(format!(
            "Google Tasks API request failed with status {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response)
}

async fn fetch_all_paginated<R>(
    client: &reqwest::Client,
    access_token: &str,
    url: &str,
    query_params: &[(&str, &str)],
) -> Result<Vec<R::Item>>
where
    R: PaginatedResponse + serde::de::DeserializeOwned,
{
    let mut all_items: Vec<R::Item> = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut req = client.get(url).bearer_auth(access_token);
        req = req.query(query_params);
        if let Some(ref token) = page_token {
            req = req.query(&[("pageToken", token)]);
        }

        let resp = check_api_response(req.send().await?).await?;
        let payload: R = resp.json().await?;

        let next_token = payload.next_page_token().cloned();
        if let Some(items) = payload.into_items() {
            all_items.extend(items);
        }

        match next_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }

    Ok(all_items)
}

// Parse a Google API 'due' RFC3339 string into a full UTC DateTime
fn parse_google_due(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

// Google Tasks treats 'due' as a date-only field (midnight). Extract just the date.
fn parse_google_due_date_naive(s: &str) -> Option<chrono::NaiveDate> {
    parse_google_due(s).map(|dt| dt.date_naive())
}

fn due_dates_same_day_utc(
    google_due: &Option<String>,
    todo_due: &Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    match (google_due, todo_due) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(g), Some(t)) => parse_google_due_date_naive(g)
            .map(|gd| gd == t.date_naive())
            .unwrap_or(false),
    }
}

/// Imprecise due-date equivalence tailored to Google Tasks API semantics.
///
/// Google Tasks stores `due` as a date-only field; the time component is discarded
/// when setting or reading via the public API. See the official docs:
/// https://developers.google.com/workspace/tasks/reference/rest/v1/tasks (field `due`).
/// The Google Tasks UI may display time-of-day, but that precision is not exposed
/// through the public API. As a result, the API typically returns midnight UTC (00:00:00Z)
/// for `due`, while local data may carry intra-day times.
///
/// This function treats two dues as "equivalent" if they fall on the same UTC calendar day
/// OR if their absolute time difference is under one minute. The small tolerance accommodates
/// minor formatting or conversion differences without masking real date changes.
fn due_dates_equivalent(
    google_due: &Option<String>,
    todo_due: &Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    use chrono::Duration;
    match (google_due, todo_due) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(g), Some(t)) => {
            if due_dates_same_day_utc(google_due, todo_due) {
                return true;
            }
            if let Some(g_utc) = parse_google_due(g) {
                let diff = t.signed_duration_since(g_utc).num_seconds().abs();
                return diff < Duration::minutes(1).num_seconds();
            }
            false
        }
    }
}

fn format_due_midnight_z(d: &Option<chrono::DateTime<chrono::Utc>>) -> Option<String> {
    use chrono::{NaiveTime, SecondsFormat};
    d.map(|dt| {
        let date = dt.date_naive();
        let ndt =
            date.and_time(NaiveTime::from_hms_milli_opt(0, 0, 0, 0).expect("midnight is valid"));
        let utc_dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
        utc_dt.to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

struct DesiredTaskValues {
    title: String,
    notes: Option<String>,
    status: &'static str,
    due: Option<String>,
}

fn desired_task_values(todo: &Todo) -> DesiredTaskValues {
    DesiredTaskValues {
        title: format!("{}{}", GOOGLE_TASK_TITLE_PREFIX, todo.title),
        notes: todo.comment.clone(),
        status: if todo.done {
            "completed"
        } else {
            "needsAction"
        },
        due: format_due_midnight_z(&todo.due_date),
    }
}

async fn fetch_all_tasklists(
    client: &reqwest::Client,
    access_token: &str,
    base_url: &str,
) -> Result<Vec<GoogleTaskList>> {
    let url = format!("{base_url}/tasks/v1/users/@me/lists");
    fetch_all_paginated::<GoogleTaskListsResponse>(
        client,
        access_token,
        &url,
        &[("maxResults", "100")],
    )
    .await
}

fn pick_juggler_list(all_tasklists: Vec<GoogleTaskList>) -> Result<GoogleTaskList> {
    all_tasklists
        .into_iter()
        .find(|list| list.title == GOOGLE_TASKS_LIST_NAME)
        .ok_or_else(|| {
            JugglerError::google_tasks(format!(
                "No '{}' task list found in Google Tasks",
                GOOGLE_TASKS_LIST_NAME
            ))
        })
}

async fn fetch_all_tasks(
    client: &reqwest::Client,
    list_id: &str,
    access_token: &str,
    base_url: &str,
) -> Result<Vec<GoogleTask>> {
    let url = format!("{base_url}/tasks/v1/lists/{}/tasks", list_id);
    fetch_all_paginated::<GoogleTasksListResponse>(
        client,
        access_token,
        &url,
        &[
            ("maxResults", "100"),
            ("showCompleted", "true"),
            ("showHidden", "true"),
            ("showDeleted", "false"),
        ],
    )
    .await
}

/// Helper function to create a new Google Task from a Todo
async fn create_google_task(
    client: &reqwest::Client,
    todo: &mut Todo,
    list_id: &str,
    access_token: &str,
    dry_run: bool,
    base_url: &str,
) -> Result<()> {
    let desired = desired_task_values(todo);
    let new_task = GoogleTask {
        id: None,
        title: desired.title,
        notes: desired.notes,
        status: desired.status.to_string(),
        due: desired.due,
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
    } else {
        let response = check_api_response(
            client
                .post(&create_url)
                .bearer_auth(access_token)
                .json(&new_task)
                .send()
                .await?,
        )
        .await?;

        let created_task: GoogleTask = response.json().await?;
        todo.google_task_id = created_task.id;
        info!("Created Google Task with ID: {:?}", todo.google_task_id);
    }

    Ok(())
}

pub async fn sync_to_tasks_with_oauth(
    todos: &mut [Todo],
    oauth_client: GoogleOAuthClient,
    dry_run: bool,
) -> Result<()> {
    sync_to_tasks_with_oauth_and_base_url(todos, oauth_client, dry_run, GOOGLE_TASKS_BASE_URL).await
}

async fn sync_to_tasks_with_oauth_and_base_url(
    todos: &mut [Todo],
    mut oauth_client: GoogleOAuthClient,
    dry_run: bool,
    base_url: &str,
) -> Result<()> {
    let access_token = oauth_client.get_access_token().await?;
    let client = &oauth_client.client;
    sync_to_tasks_with_base_url(todos, &access_token, dry_run, base_url, client).await
}

fn log_task_diffs(
    google_task: &GoogleTask,
    todo: &Todo,
    task_id: &str,
    desired_title: &str,
    desired_notes: &Option<String>,
    desired_status: &str,
    desired_due: &Option<String>,
) {
    info!("Detected changes for Google Task (ID: {}):", task_id);
    if google_task.title == desired_title {
        info!(" - title: not changed");
    } else {
        info!(" - title: changed to: '{}'", desired_title);
    }
    if &google_task.notes == desired_notes {
        info!(" - notes: not changed");
    } else {
        info!(" - notes: changed to: {}", display_opt(desired_notes));
    }
    if (google_task.status == "completed") == todo.done {
        info!(" - status: not changed");
    } else {
        info!(" - status: changed to: '{}'", desired_status);
    }
    if due_dates_equivalent(&google_task.due, &todo.due_date) {
        info!(" - due: not changed");
    } else {
        let google_due_str = google_task.due.as_deref().unwrap_or("<none>");
        let google_date = google_task
            .due
            .as_deref()
            .and_then(parse_google_due_date_naive)
            .map(|d| d.to_string())
            .unwrap_or_else(|| "<n/a>".to_string());
        let todo_date = todo
            .due_date
            .map(|d| d.date_naive().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let diff_secs = match (
            google_task.due.as_deref().and_then(parse_google_due),
            &todo.due_date,
        ) {
            (Some(g), Some(t)) => t.signed_duration_since(g).num_seconds().abs(),
            _ => 0,
        };
        info!(
            " - due: changed (google='{}' date={} vs todo_date={}, |Δ|={}s)",
            google_due_str, google_date, todo_date, diff_secs
        );
        info!(" - due: changed to: {}", display_opt(desired_due));
    }
}

async fn delete_orphan_tasks(
    google_task_map: std::collections::HashMap<String, GoogleTask>,
    list_id: &str,
    access_token: &str,
    dry_run: bool,
    base_url: &str,
    client: &reqwest::Client,
) -> Result<()> {
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
            let delete_url = format!("{base_url}/tasks/v1/lists/{list_id}/tasks/{task_id}");
            check_api_response(
                client
                    .delete(&delete_url)
                    .bearer_auth(access_token)
                    .send()
                    .await?,
            )
            .await?;
            info!("Deleted orphaned Google Task: '{}'", google_task.title);
        }
    }
    Ok(())
}

async fn sync_to_tasks_with_base_url(
    todos: &mut [Todo],
    access_token: &str,
    dry_run: bool,
    base_url: &str,
    client: &reqwest::Client,
) -> Result<()> {
    if dry_run {
        info!("Starting sync in DRY RUN mode - no changes will be made");
    } else {
        info!("Starting sync with Google Tasks");
    }

    // First, find the task list for synchronization (across all pages)
    let all_tasklists = fetch_all_tasklists(client, access_token, base_url).await?;
    let juggler_list = pick_juggler_list(all_tasklists)?;
    info!("Parent task list ID: {}", juggler_list.id);
    // Get all existing tasks from the sync list (across all pages)
    let existing_tasks = fetch_all_tasks(client, &juggler_list.id, access_token, base_url).await?;

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
                    let desired = desired_task_values(todo);
                    let needs_update = google_task.title != desired.title
                        || google_task.notes.as_deref() != desired.notes.as_deref()
                        || (google_task.status == "completed") != todo.done
                        || !due_dates_equivalent(&google_task.due, &todo.due_date);

                    if needs_update {
                        log_task_diffs(
                            &google_task,
                            todo,
                            task_id,
                            &desired.title,
                            &desired.notes,
                            desired.status,
                            &desired.due,
                        );

                        let updated_task = GoogleTask {
                            id: Some(task_id.clone()),
                            title: desired.title,
                            notes: desired.notes,
                            status: desired.status.to_string(),
                            due: desired.due,
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
                            check_api_response(
                                client
                                    .put(&update_url)
                                    .bearer_auth(access_token)
                                    .json(&updated_task)
                                    .send()
                                    .await?,
                            )
                            .await?;
                        }
                    }
                } else {
                    // Task was deleted in Google Tasks, recreate it (one-way sync)
                    create_google_task(
                        client,
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
                    client,
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
    delete_orphan_tasks(
        google_task_map,
        &juggler_list.id,
        access_token,
        dry_run,
        base_url,
        client,
    )
    .await?;

    if dry_run {
        info!("DRY RUN complete - no changes were made");
    } else {
        info!("Sync complete");
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
    use crate::oauth::{GoogleOAuthClient, GoogleOAuthCredentials};
    use crate::time::{SharedClock, fixed_clock};
    use chrono::{TimeZone, Utc};
    use wiremock::matchers::{bearer_token, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_clock() -> SharedClock {
        fixed_clock(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap())
    }

    #[test]
    fn test_due_dates_equivalent_date_only_and_tolerance() {
        // This unit test verifies the behavior described in the docstring for
        // `due_dates_equivalent`: Google Tasks API exposes `due` as date-only, so
        // we consider dues equivalent when they fall on the same UTC day, and we
        // allow a very small (< 60s) tolerance across boundaries.

        // Same calendar day should be treated as equivalent (date-only semantics)
        let google_due_same_day = Some("2025-08-20T00:00:00Z".to_string());
        let todo_due_same_day = Some(
            chrono::Utc
                .with_ymd_and_hms(2025, 8, 20, 23, 25, 14)
                .unwrap(),
        );
        assert!(due_dates_equivalent(
            &google_due_same_day,
            &todo_due_same_day
        ));

        // Across a calendar boundary but within 60 seconds should still be equivalent
        let google_due_just_before = Some("2025-08-20T23:59:40Z".to_string());
        let todo_due_just_after =
            Some(chrono::Utc.with_ymd_and_hms(2025, 8, 21, 0, 0, 10).unwrap());
        assert!(due_dates_equivalent(
            &google_due_just_before,
            &todo_due_just_after
        ));

        // Different days and beyond the 1-minute tolerance should NOT be equivalent
        let google_due_far = Some("2025-08-20T00:00:00Z".to_string());
        let todo_due_far = Some(chrono::Utc.with_ymd_and_hms(2025, 8, 21, 0, 1, 1).unwrap());
        assert!(!due_dates_equivalent(&google_due_far, &todo_due_far));
    }

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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "invalid_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
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
            &reqwest::Client::new(),
        )
        .await;

        assert!(result.is_ok());
        // In dry run mode, no local mutation should occur
        assert!(todos[0].google_task_id.is_none());
    }

    #[tokio::test]
    async fn test_sync_with_due_dates() {
        let mock_server = MockServer::start().await;
        let base = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let test_due_date = base + chrono::Duration::days(1);

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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
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

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(
            todos[0].google_task_id,
            Some("completed_task_id".to_string())
        );
    }

    #[tokio::test]
    async fn test_sync_with_oauth_success() {
        let mock_server = MockServer::start().await;
        let oauth_mock_server = MockServer::start().await;

        // Mock OAuth token endpoint
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "oauth_access_token",
                "expires_in": 3600,
                "token_type": "Bearer"
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
        let oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            oauth_token_url,
            test_clock(),
        );

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
                "expires_in": 3600,
                "token_type": "Bearer"
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
        let oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            oauth_token_url,
            test_clock(),
        );

        let result = sync_to_tasks_with_oauth_and_base_url(
            &mut todos,
            oauth_client,
            true, // dry_run = true
            &mock_server.uri(),
        )
        .await;

        assert!(result.is_ok());
        // In dry run mode, no local mutation should occur
        assert!(todos[0].google_task_id.is_none());
    }

    #[tokio::test]
    async fn test_sync_dry_run_mode_no_update_calls() {
        let mock_server = MockServer::start().await;

        // Mock the task lists endpoint
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "list1", "title": "juggler" }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Existing tasks endpoint returns one task that needs updating
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/list1/tasks"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "t1", "title": "j:Old", "status": "needsAction" }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Local todo has same id but different title, which would trigger an update
        let mut todos = vec![Todo {
            title: "New".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: Some("t1".to_string()),
        }];

        // Dry-run should NOT issue a PUT; no PUT mock is defined
        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            true,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_dry_run_mode_no_delete_calls() {
        let mock_server = MockServer::start().await;

        // Mock the task lists endpoint
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "list1", "title": "juggler" }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Existing tasks endpoint returns one orphaned task
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/list1/tasks"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "orphan", "title": "j:Orphan", "status": "needsAction" }
                ]
            })))
            .mount(&mock_server)
            .await;

        let mut todos: Vec<Todo> = vec![]; // No local todos -> would delete remotely

        // Dry-run should NOT issue a DELETE; no DELETE mock is defined
        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            true,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
        .await;

        assert!(result.is_ok());
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
                "expires_in": 3600,
                "token_type": "Bearer"
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
        let oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            oauth_token_url,
            test_clock(),
        );

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
        let oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            oauth_token_url,
            test_clock(),
        );

        let result = sync_to_tasks_with_oauth_and_base_url(
            &mut todos,
            oauth_client,
            false,
            GOOGLE_TASKS_BASE_URL, // Won't be reached due to OAuth failure
        )
        .await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("OAuth token refresh failed"));
    }

    #[tokio::test]
    async fn test_tasklists_pagination_finds_list_on_second_page() {
        let mock_server = MockServer::start().await;

        // Page 2: contains the juggler list
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("test_token"))
            .and(query_param("pageToken", "p2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "j_list", "title": "juggler" }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Page 1: no juggler list, provides nextPageToken
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "other_list_id", "title": "Other" }
                ],
                "nextPageToken": "p2"
            })))
            .mount(&mock_server)
            .await;

        // Existing tasks for found list: empty, single page
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/j_list/tasks"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []
            })))
            .mount(&mock_server)
            .await;

        // Create task
        Mock::given(method("POST"))
            .and(path("/tasks/v1/lists/j_list/tasks"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "created_on_second_page_list",
                "title": "j:New",
                "status": "needsAction"
            })))
            .mount(&mock_server)
            .await;

        let mut todos = vec![Todo {
            title: "New".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(
            todos[0].google_task_id,
            Some("created_on_second_page_list".to_string())
        );
    }

    #[tokio::test]
    async fn test_tasks_pagination_deletes_across_pages() {
        let mock_server = MockServer::start().await;

        // Tasklists: return juggler list
        Mock::given(method("GET"))
            .and(path("/tasks/v1/users/@me/lists"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [ { "id": "list1", "title": "juggler" } ]
            })))
            .mount(&mock_server)
            .await;

        // Tasks page 2: another task
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/list1/tasks"))
            .and(bearer_token("test_token"))
            .and(query_param("pageToken", "tok2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "task_b", "title": "j:B", "status": "needsAction" }
                ]
            })))
            .mount(&mock_server)
            .await;

        // Tasks page 1: one task, provides nextPageToken
        Mock::given(method("GET"))
            .and(path("/tasks/v1/lists/list1/tasks"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "task_a", "title": "j:A", "status": "needsAction" }
                ],
                "nextPageToken": "tok2"
            })))
            .mount(&mock_server)
            .await;

        // Expect deletes for both tasks since there are no local todos
        Mock::given(method("DELETE"))
            .and(path("/tasks/v1/lists/list1/tasks/task_a"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        Mock::given(method("DELETE"))
            .and(path("/tasks/v1/lists/list1/tasks/task_b"))
            .and(bearer_token("test_token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut todos: Vec<Todo> = vec![];
        let result = sync_to_tasks_with_base_url(
            &mut todos,
            "test_token",
            false,
            &mock_server.uri(),
            &reqwest::Client::new(),
        )
        .await;
        assert!(result.is_ok());
    }
}
