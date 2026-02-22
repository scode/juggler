//! Durable TOML storage for todos.
//!
//! This module maps between runtime `Todo` values and persisted records, and
//! loads/saves the TODO list from disk.
//!
//! Save paths use temporary files, atomic replacement, and timestamped archive
//! copies of previous files. It also handles directory creation and Unix
//! permission setup for the local data file.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{collections::HashSet, fs, io::Write};

use tempfile::NamedTempFile;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;

use crate::error::{JugglerError, Result};
use crate::time::{Clock, SharedClock, system_clock};
use crate::ui::Todo;

const FORMAT_VERSION_CURRENT: u32 = 1;
const JUGGLER_EDITION_CURRENT: u32 = 1;

/// Storage-facing todo representation used for disk format transforms.
///
/// This keeps persistence concerns (notably stable `todo_id`) separate from
/// runtime-only UI flags while still allowing lossless round-tripping.
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub todo_id: Option<String>,
    pub title: String,
    pub comment: Option<String>,
    pub done: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub google_task_id: Option<String>,
}

/// Legacy YAML record shape accepted only for one-way migration reads.
///
/// We intentionally keep this type scoped to fallback loading so the write path
/// cannot accidentally emit legacy YAML again.
#[derive(Debug, serde::Deserialize)]
struct LegacyTodoItem {
    title: String,
    comment: Option<String>,
    #[serde(default)]
    done: bool,
    due_date: Option<DateTime<Utc>>,
    google_task_id: Option<String>,
}

/// Version gate for persisted TODO files.
///
/// `format_version` governs machine-readable schema evolution while
/// `juggler_edition` gates behavior changes that may need human review.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Metadata {
    format_version: u32,
    juggler_edition: u32,
}

impl Metadata {
    fn current() -> Self {
        Self {
            format_version: FORMAT_VERSION_CURRENT,
            juggler_edition: JUGGLER_EDITION_CURRENT,
        }
    }
}

/// Serialized todo payload stored under `[todos.TN]` in TOML.
///
/// Optional fields are omitted when absent to keep files concise and avoid
/// introducing sentinel/null encodings in TOML.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TodoRecord {
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    #[serde(default)]
    done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    due_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    google_task_id: Option<String>,
}

/// Full TOML document shape for the TODO store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TodosFile {
    metadata: Metadata,
    #[serde(default)]
    todos: IndexMap<String, TodoRecord>,
}

impl From<&Todo> for TodoItem {
    fn from(todo: &Todo) -> Self {
        TodoItem {
            todo_id: todo.todo_id.clone(),
            title: todo.title.clone(),
            comment: todo.comment.clone(),
            done: todo.done,
            due_date: todo.due_date,
            google_task_id: todo.google_task_id.clone(),
        }
    }
}

impl From<&TodoItem> for TodoRecord {
    fn from(todo: &TodoItem) -> Self {
        TodoRecord {
            title: todo.title.clone(),
            comment: todo.comment.clone(),
            done: todo.done,
            due_date: todo.due_date.map(|date| date.to_rfc3339()),
            google_task_id: todo.google_task_id.clone(),
        }
    }
}

/// Load todos from canonical TOML, with YAML fallback only when TOML is absent.
///
/// If TOML exists but is invalid or version-incompatible, loading fails
/// deliberately rather than silently falling back to legacy data.
pub fn load_todos<P: AsRef<std::path::Path>>(file_path: P) -> Result<Vec<Todo>> {
    let file_path = file_path.as_ref();
    let content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return load_legacy_todos_or_empty(file_path);
        }
        Err(e) => return Err(e.into()),
    };

    let file: TodosFile = toml::from_str(&content)?;
    validate_metadata(&file.metadata)?;

    let mut parsed_items: Vec<(u64, TodoItem)> = Vec::with_capacity(file.todos.len());
    for (todo_id, record) in file.todos {
        let number = parse_todo_id(&todo_id).ok_or_else(|| {
            JugglerError::config(format!(
                "Invalid todo id '{todo_id}'; expected format T<N> with N >= 1"
            ))
        })?;

        let due_date = match record.due_date {
            Some(raw) => Some(parse_due_date(&raw)?),
            None => None,
        };

        parsed_items.push((
            number,
            TodoItem {
                todo_id: Some(todo_id),
                title: record.title,
                comment: record.comment,
                done: record.done,
                due_date,
                google_task_id: record.google_task_id,
            },
        ));
    }

    parsed_items.sort_by_key(|(number, _)| *number);
    let todos: Vec<Todo> = parsed_items
        .into_iter()
        .map(|(_, item)| item.into())
        .collect();

    Ok(todos)
}

/// Persist todos atomically, assigning missing stable IDs before serialization.
///
/// This mutates the provided slice to reflect any newly assigned `todo_id`
/// values so in-memory state remains consistent after save.
pub fn store_todos_with_clock<P: AsRef<std::path::Path>>(
    todos: &mut [Todo],
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

    assign_missing_todo_ids(todos)?;

    let mut todo_items: Vec<TodoItem> = todos.iter().map(TodoItem::from).collect();

    // Store in numeric todo-id order for stable user-visible IDs.
    todo_items.sort_by_key(|item| {
        item.todo_id
            .as_deref()
            .and_then(parse_todo_id)
            .expect("todo ids must be assigned and valid before sort")
    });

    let mut todo_map: IndexMap<String, TodoRecord> = IndexMap::new();
    for item in &todo_items {
        let todo_id = item
            .todo_id
            .as_ref()
            .ok_or_else(|| JugglerError::Other("todo_id missing after assignment".to_string()))?;
        todo_map.insert(todo_id.clone(), TodoRecord::from(item));
    }

    let file = TodosFile {
        metadata: Metadata::current(),
        todos: todo_map,
    };

    let toml_content = toml::to_string_pretty(&file)?;

    let target_dir = file_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut temp_file = NamedTempFile::new_in(target_dir)?;

    {
        let file = temp_file.as_file_mut();
        file.write_all(toml_content.as_bytes())?;

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

/// Save todos to disk, mutating input to assign missing stable ids as needed.
pub fn store_todos<P: AsRef<std::path::Path>>(todos: &mut [Todo], file_path: P) -> Result<()> {
    store_todos_with_clock(todos, file_path, system_clock())
}

/// Enforce strict version gating so unsupported files fail fast.
fn validate_metadata(metadata: &Metadata) -> Result<()> {
    if metadata.format_version != FORMAT_VERSION_CURRENT {
        return Err(JugglerError::config(format!(
            "TODO store metadata is malformed or from a newer juggler release: unsupported metadata.format_version={} (expected {})",
            metadata.format_version, FORMAT_VERSION_CURRENT
        )));
    }

    if metadata.juggler_edition != JUGGLER_EDITION_CURRENT {
        return Err(JugglerError::config(format!(
            "TODO store metadata is malformed or from a newer juggler release: unsupported metadata.juggler_edition={} (expected {})",
            metadata.juggler_edition, JUGGLER_EDITION_CURRENT
        )));
    }

    Ok(())
}

/// Ensure every todo has a unique `T<N>` ID, preserving existing IDs and
/// allocating new IDs monotonically from the current maximum.
fn assign_missing_todo_ids(todos: &mut [Todo]) -> Result<()> {
    let mut used_ids: HashSet<u64> = HashSet::new();
    let mut max_seen = 0u64;

    for todo in todos.iter() {
        if let Some(todo_id) = todo.todo_id.as_deref() {
            let number = parse_todo_id(todo_id).ok_or_else(|| {
                JugglerError::config(format!(
                    "Invalid todo_id '{}' in memory; expected format T<N> with N >= 1",
                    todo_id
                ))
            })?;
            if !used_ids.insert(number) {
                return Err(JugglerError::config(format!(
                    "Duplicate todo_id '{}' in memory",
                    todo_id
                )));
            }
            max_seen = max_seen.max(number);
        }
    }

    let mut next_number = max_seen.saturating_add(1);
    for todo in todos.iter_mut() {
        if todo.todo_id.is_none() {
            todo.todo_id = Some(format_todo_id(next_number));
            let inserted = used_ids.insert(next_number);
            debug_assert!(inserted);
            next_number = next_number.saturating_add(1);
        }
    }

    Ok(())
}

/// Parse storage key format `T<N>` where `N` is a non-zero positive integer.
fn parse_todo_id(input: &str) -> Option<u64> {
    let mut chars = input.chars();
    if chars.next()? != 'T' {
        return None;
    }

    let numeric = chars.as_str();
    if numeric.is_empty() || numeric.starts_with('0') {
        return None;
    }

    if !numeric.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    numeric.parse::<u64>().ok()
}

fn format_todo_id(number: u64) -> String {
    format!("T{number}")
}

/// Parse due date strings from persisted/editor TOML as RFC3339 timestamps.
pub(crate) fn parse_due_date(input: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(input)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| JugglerError::config(format!("Invalid due_date value '{}': {}", input, e)))
}

/// Read legacy YAML when canonical TOML is missing, assigning deterministic
/// `T1..Tn` IDs in source order for migration.
fn load_legacy_todos_or_empty(file_path: &std::path::Path) -> Result<Vec<Todo>> {
    let legacy_path = legacy_yaml_path(file_path);
    let content = match fs::read_to_string(&legacy_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    // Treat blank legacy files as an empty task list to match the "missing
    // file -> empty list" behavior during migration fallback.
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let legacy_items: Vec<LegacyTodoItem> = serde_yaml::from_str(&content)?;
    let todos = legacy_items
        .into_iter()
        .enumerate()
        .map(|(index, item)| Todo {
            title: item.title,
            comment: item.comment,
            expanded: false,
            done: item.done,
            selected: false,
            due_date: item.due_date,
            todo_id: Some(format_todo_id((index as u64) + 1)),
            google_task_id: item.google_task_id,
        })
        .collect();

    Ok(todos)
}

/// Compute sibling `TODOs.yaml` path from canonical `TODOs.toml`.
fn legacy_yaml_path(toml_path: &std::path::Path) -> std::path::PathBuf {
    let mut legacy = toml_path.to_path_buf();
    legacy.set_extension("yaml");
    legacy
}

fn archive_todos_file(file_path: &std::path::Path, clock: &dyn Clock) -> Result<()> {
    let parent = file_path
        .parent()
        .ok_or_else(|| JugglerError::Other("File path has no parent directory".to_string()))?;

    let now = clock.now();
    let timestamp_str = now.format("%Y-%m-%dT%H-%M-%S").to_string();

    const MAX_ARCHIVE_ATTEMPTS: u32 = 10_000;

    let archive_path = {
        let base_name = format!("TODOs_{timestamp_str}.toml");
        let base_path = parent.join(&base_name);
        if !base_path.exists() {
            base_path
        } else {
            let mut counter = 1u32;
            loop {
                if counter > MAX_ARCHIVE_ATTEMPTS {
                    return Err(JugglerError::Other(format!(
                        "Too many archives with timestamp {timestamp_str}"
                    )));
                }
                let numbered_name = format!("TODOs_{timestamp_str}_{counter}.toml");
                let numbered_path = parent.join(&numbered_name);
                if !numbered_path.exists() {
                    break numbered_path;
                }
                counter += 1;
            }
        }
    };

    fs::copy(file_path, archive_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::fixed_clock;

    fn make_todo(title: &str) -> Todo {
        Todo {
            title: title.to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            todo_id: None,
            google_task_id: None,
        }
    }

    fn make_toml_fixture() -> String {
        r#"[metadata]
format_version = 1
juggler_edition = 1

[todos.T1]
title = "Item 1"
comment = """This is a comment for item 1.
It can span multiple lines."""
done = false
due_date = "2031-01-08T09:00:00Z"
google_task_id = "id-1"

[todos.T2]
title = "Completed task example"
done = true
"#
        .to_string()
    }

    #[test]
    fn load_todos_parses_comments_and_ids_from_toml() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("TODOs.toml");
        fs::write(&test_file, make_toml_fixture()).expect("write fixture");

        let todos = load_todos(&test_file).expect("load todos");
        assert_eq!(todos.len(), 2);

        let item1 = todos.iter().find(|t| t.title == "Item 1").expect("Item 1");
        let comment = item1.comment.as_deref().expect("comment for Item 1");
        assert!(comment.starts_with("This is a comment for item 1."));
        assert!(comment.contains("It can span multiple lines."));
        assert_eq!(item1.todo_id.as_deref(), Some("T1"));
    }

    #[test]
    fn load_todos_handles_done_field() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("TODOs.toml");
        fs::write(&test_file, make_toml_fixture()).expect("write fixture");

        let todos = load_todos(&test_file).expect("load TODOs");
        assert_eq!(todos.len(), 2);

        let completed = todos
            .iter()
            .find(|t| t.title == "Completed task example")
            .expect("Completed task example");
        assert!(completed.done);

        for todo in &todos {
            if todo.title != "Completed task example" {
                assert!(!todo.done);
            }
        }
    }

    #[test]
    fn load_todos_missing_file_returns_empty() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let non_existent_file = temp_dir.path().join("non_existent_todos.toml");

        let todos = load_todos(&non_existent_file).expect("load todos from non-existent file");
        assert_eq!(todos.len(), 0);
    }

    #[test]
    fn load_todos_rejects_missing_metadata() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("TODOs.toml");
        fs::write(
            &test_file,
            r#"
[todos.T1]
title = "Item 1"
done = false
"#,
        )
        .expect("write fixture");

        let err = load_todos(&test_file).expect_err("missing metadata should error");
        assert!(err.to_string().contains("TOML parse error"));
    }

    #[test]
    fn load_todos_rejects_unsupported_format_version() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("TODOs.toml");
        fs::write(
            &test_file,
            r#"[metadata]
format_version = 2
juggler_edition = 1
"#,
        )
        .expect("write fixture");

        let err = load_todos(&test_file).expect_err("unsupported format should error");
        assert!(err.to_string().contains("metadata.format_version=2"));
        assert!(
            err.to_string()
                .contains("malformed or from a newer juggler release")
        );
    }

    #[test]
    fn load_todos_rejects_unsupported_juggler_edition() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("TODOs.toml");
        fs::write(
            &test_file,
            r#"[metadata]
format_version = 1
juggler_edition = 2
"#,
        )
        .expect("write fixture");

        let err = load_todos(&test_file).expect_err("unsupported edition should error");
        assert!(err.to_string().contains("metadata.juggler_edition=2"));
        assert!(
            err.to_string()
                .contains("malformed or from a newer juggler release")
        );
    }

    #[test]
    fn load_todos_rejects_invalid_todo_key() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("TODOs.toml");
        fs::write(
            &test_file,
            r#"[metadata]
format_version = 1
juggler_edition = 1

[todos.bad]
title = "Item"
done = false
"#,
        )
        .expect("write fixture");

        let err = load_todos(&test_file).expect_err("invalid key should error");
        assert!(err.to_string().contains("Invalid todo id 'bad'"));
    }

    #[test]
    fn load_todos_prefers_toml_when_toml_and_yaml_exist() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        fs::write(
            &toml_file,
            r#"[metadata]
format_version = 1
juggler_edition = 1

[todos.T1]
title = "from toml"
done = false
"#,
        )
        .expect("write toml fixture");

        fs::write(&yaml_file, "- title: from yaml\n").expect("write yaml fixture");

        let todos = load_todos(&toml_file).expect("load todos");
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "from toml");
    }

    #[test]
    fn load_todos_reads_legacy_yaml_when_toml_missing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        fs::write(
            &yaml_file,
            r#"- title: Item 1
  comment: Legacy comment
  done: false
  due_date: null
  google_task_id: legacy-google-id
- title: Completed task example
  comment: null
  done: true
  due_date: null
  google_task_id: null
"#,
        )
        .expect("write legacy yaml");

        let todos = load_todos(&toml_file).expect("load legacy todos");
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].todo_id.as_deref(), Some("T1"));
        assert_eq!(todos[1].todo_id.as_deref(), Some("T2"));
        assert_eq!(todos[0].title, "Item 1");
        assert_eq!(todos[1].title, "Completed task example");
    }

    #[test]
    fn load_todos_reads_empty_legacy_yaml_array_when_toml_missing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        fs::write(&yaml_file, "[]\n").expect("write empty legacy yaml array");

        let todos = load_todos(&toml_file).expect("load todos from empty yaml array");
        assert!(todos.is_empty());
    }

    #[test]
    fn load_todos_reads_blank_legacy_yaml_as_empty_when_toml_missing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        fs::write(&yaml_file, "   \n\t\n").expect("write blank legacy yaml");

        let todos = load_todos(&toml_file).expect("load todos from blank yaml");
        assert!(todos.is_empty());
    }

    #[test]
    fn store_todos_roundtrip_with_stable_ids() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("test_todos.toml");

        let mut test_todos = vec![
            Todo {
                title: "Test todo 1".to_string(),
                comment: Some("Test comment 1".to_string()),
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T7".to_string()),
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
                todo_id: None,
                google_task_id: Some("google_task_123".to_string()),
            },
        ];

        store_todos(&mut test_todos, &test_file).expect("store todos");
        assert!(test_file.exists());
        assert_eq!(test_todos[0].todo_id.as_deref(), Some("T7"));
        assert_eq!(test_todos[1].todo_id.as_deref(), Some("T8"));

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
        let test_file = temp_dir.path().join("test_todos.toml");

        let mut initial_todos = vec![make_todo("Initial todo")];

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = fixed_clock(fixed_now);

        store_todos_with_clock(&mut initial_todos, &test_file, clock.clone())
            .expect("store initial todos");
        assert!(test_file.exists());

        let mut updated_todos = vec![make_todo("Updated todo")];

        store_todos_with_clock(&mut updated_todos, &test_file, clock.clone())
            .expect("store updated todos");
        assert!(test_file.exists());

        let expected_archive = format!("TODOs_{}.toml", fixed_now.format("%Y-%m-%dT%H-%M-%S"));
        let archive_path = temp_dir.path().join(&expected_archive);
        assert!(archive_path.exists());

        let archived_todos = load_todos(&archive_path).expect("load archived todos");
        assert_eq!(archived_todos.len(), 1);
        assert_eq!(archived_todos[0].title, "Initial todo");

        let current_todos = load_todos(&test_file).expect("load current todos");
        assert_eq!(current_todos.len(), 1);
        assert_eq!(current_todos[0].title, "Updated todo");
    }

    #[test]
    fn store_todos_handles_archive_collision() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("test_todos.toml");

        let mut todo = vec![make_todo("Test todo")];

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = fixed_clock(fixed_now);

        store_todos_with_clock(&mut todo, &test_file, clock.clone()).expect("store 1");
        store_todos_with_clock(&mut todo, &test_file, clock.clone()).expect("store 2");
        store_todos_with_clock(&mut todo, &test_file, clock.clone()).expect("store 3");
        store_todos_with_clock(&mut todo, &test_file, clock).expect("store 4");

        let timestamp = fixed_now.format("%Y-%m-%dT%H-%M-%S");
        assert!(
            temp_dir
                .path()
                .join(format!("TODOs_{timestamp}.toml"))
                .exists()
        );
        assert!(
            temp_dir
                .path()
                .join(format!("TODOs_{timestamp}_1.toml"))
                .exists()
        );
        assert!(
            temp_dir
                .path()
                .join(format!("TODOs_{timestamp}_2.toml"))
                .exists()
        );
    }

    #[cfg(unix)]
    #[test]
    fn store_todos_sets_permissions_unix() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let nested_dir = temp_dir.path().join("nested");
        let test_file = nested_dir.join("perms_todos.toml");

        let mut todos = vec![make_todo("Perms todo")];

        store_todos(&mut todos, &test_file).expect("store todos");
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
    fn store_todos_orders_by_numeric_id() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("sorted_todos.toml");

        let mut todos = vec![
            Todo {
                title: "Zebra".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T10".to_string()),
                google_task_id: Some("id_3".to_string()),
            },
            Todo {
                title: "Apple".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T2".to_string()),
                google_task_id: None,
            },
            Todo {
                title: "Banana".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T1".to_string()),
                google_task_id: Some("id_1".to_string()),
            },
        ];

        store_todos(&mut todos, &test_file).expect("store todos");

        let loaded = load_todos(&test_file).expect("load todos");
        assert_eq!(loaded[0].todo_id.as_deref(), Some("T1"));
        assert_eq!(loaded[1].todo_id.as_deref(), Some("T2"));
        assert_eq!(loaded[2].todo_id.as_deref(), Some("T10"));
    }

    #[test]
    fn store_todos_assigns_monotonic_ids_without_reuse() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("ids_todos.toml");

        let mut todos = vec![
            Todo {
                title: "Existing".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T4".to_string()),
                google_task_id: None,
            },
            make_todo("New one"),
            Todo {
                title: "Existing high".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T9".to_string()),
                google_task_id: None,
            },
            make_todo("New two"),
        ];

        store_todos(&mut todos, &test_file).expect("store todos");

        assert_eq!(todos[0].todo_id.as_deref(), Some("T4"));
        assert_eq!(todos[1].todo_id.as_deref(), Some("T10"));
        assert_eq!(todos[2].todo_id.as_deref(), Some("T9"));
        assert_eq!(todos[3].todo_id.as_deref(), Some("T11"));
    }

    #[test]
    fn store_todos_rejects_duplicate_ids() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("dup_ids.toml");

        let mut todos = vec![
            Todo {
                title: "A".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T1".to_string()),
                google_task_id: None,
            },
            Todo {
                title: "B".to_string(),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                todo_id: Some("T1".to_string()),
                google_task_id: None,
            },
        ];

        let err = store_todos(&mut todos, &test_file).expect_err("duplicate IDs should fail");
        assert!(err.to_string().contains("Duplicate todo_id 'T1'"));
    }

    #[test]
    fn store_todos_rejects_invalid_id_format() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("bad_ids.toml");

        let mut todos = vec![Todo {
            title: "A".to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            todo_id: Some("T01".to_string()),
            google_task_id: None,
        }];

        let err = store_todos(&mut todos, &test_file).expect_err("invalid IDs should fail");
        assert!(err.to_string().contains("Invalid todo_id 'T01'"));
    }

    #[test]
    fn store_todos_omits_absent_optional_fields() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("optional_fields.toml");

        let mut todos = vec![make_todo("No optionals")];
        store_todos(&mut todos, &test_file).expect("store todos");

        let content = fs::read_to_string(&test_file).expect("read stored TOML");
        assert!(!content.contains("comment ="));
        assert!(!content.contains("due_date ="));
        assert!(!content.contains("google_task_id ="));
        assert!(content.contains("done = false"));
    }

    #[test]
    fn store_todos_always_writes_metadata_versions() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let test_file = temp_dir.path().join("metadata.toml");

        let mut todos = vec![make_todo("Has metadata")];
        store_todos(&mut todos, &test_file).expect("store todos");

        let content = fs::read_to_string(&test_file).expect("read stored TOML");
        assert!(content.contains("[metadata]"));
        assert!(content.contains("format_version = 1"));
        assert!(content.contains("juggler_edition = 1"));
    }

    #[test]
    fn migration_from_yaml_write_creates_toml_and_keeps_yaml_unchanged() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        let legacy_content = r#"- title: Legacy Item
  comment: null
  done: false
  due_date: null
  google_task_id: null
"#;
        fs::write(&yaml_file, legacy_content).expect("write yaml");

        let mut todos = load_todos(&toml_file).expect("load from yaml fallback");
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].todo_id.as_deref(), Some("T1"));

        store_todos(&mut todos, &toml_file).expect("store to toml");

        assert!(toml_file.exists());
        let yaml_after = fs::read_to_string(&yaml_file).expect("read yaml after");
        assert_eq!(yaml_after, legacy_content);

        let loaded = load_todos(&toml_file).expect("load toml after migration");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].todo_id.as_deref(), Some("T1"));
    }

    #[test]
    fn migration_roundtrip_preserves_due_date_and_google_task_id() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        let legacy_content = r#"- title: Legacy Item
  comment: keep this
  done: false
  due_date: 2025-02-03T04:05:06Z
  google_task_id: legacy-google-id
"#;
        fs::write(&yaml_file, legacy_content).expect("write yaml");

        let mut todos = load_todos(&toml_file).expect("load from yaml fallback");
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].todo_id.as_deref(), Some("T1"));
        assert_eq!(todos[0].google_task_id.as_deref(), Some("legacy-google-id"));
        assert_eq!(
            todos[0].due_date,
            Some(
                chrono::DateTime::parse_from_rfc3339("2025-02-03T04:05:06Z")
                    .expect("parse expected datetime")
                    .with_timezone(&Utc)
            )
        );

        store_todos(&mut todos, &toml_file).expect("store to toml");
        fs::write(
            &yaml_file,
            r#"- title: changed yaml that should now be ignored
  due_date: 2030-01-01T00:00:00Z
  google_task_id: changed-id
"#,
        )
        .expect("mutate legacy yaml after migration");

        let loaded = load_todos(&toml_file).expect("load toml after migration");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "Legacy Item");
        assert_eq!(loaded[0].todo_id.as_deref(), Some("T1"));
        assert_eq!(
            loaded[0].google_task_id.as_deref(),
            Some("legacy-google-id")
        );
        assert_eq!(
            loaded[0].due_date,
            Some(
                chrono::DateTime::parse_from_rfc3339("2025-02-03T04:05:06Z")
                    .expect("parse expected datetime")
                    .with_timezone(&Utc)
            )
        );
    }

    #[test]
    fn migration_follow_up_archives_use_toml_extension() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("create temp dir");
        let toml_file = temp_dir.path().join("TODOs.toml");
        let yaml_file = temp_dir.path().join("TODOs.yaml");

        fs::write(
            &yaml_file,
            r#"- title: Legacy Item
  done: false
"#,
        )
        .expect("write legacy yaml");

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let clock = fixed_clock(fixed_now);

        let mut todos = load_todos(&toml_file).expect("load from yaml fallback");
        store_todos_with_clock(&mut todos, &toml_file, clock.clone()).expect("first toml write");

        todos[0].title = "Updated item".to_string();
        store_todos_with_clock(&mut todos, &toml_file, clock.clone())
            .expect("second toml write with archive");

        let timestamp = fixed_now.format("%Y-%m-%dT%H-%M-%S");
        let toml_archive = temp_dir.path().join(format!("TODOs_{timestamp}.toml"));
        let yaml_archive = temp_dir.path().join(format!("TODOs_{timestamp}.yaml"));
        assert!(toml_archive.exists());
        assert!(!yaml_archive.exists());
    }
}
