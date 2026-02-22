//! External editor integration for todo edit/create flows.
//!
//! This module provides the side-effect adapter that bridges pure UI state
//! transitions with user editing in an external program. It is invoked when the
//! reducer emits edit/create requests.
//!
//! The implementation resolves `$VISUAL`/`$EDITOR`, parses optional command-line
//! arguments safely, writes a temporary TOML payload, and then reads the edited
//! content back into a validated `Todo` value.
//!
//! The `TodoEditor` trait supports runtime and test implementations.

use std::{env, fs, io::Write, process::Command};

use tempfile::NamedTempFile;

use crate::config::DEFAULT_EDITOR;
use crate::error::{JugglerError, Result};
use crate::store::{TodoItem, parse_due_date};

use super::todo::Todo;

/// Editable TOML payload shown to users in external editors.
///
/// `todo_id` is intentionally excluded so users cannot accidentally mutate the
/// stable on-disk primary key.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct EditorTodoPayload {
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

impl From<&TodoItem> for EditorTodoPayload {
    fn from(item: &TodoItem) -> Self {
        Self {
            title: item.title.clone(),
            comment: item.comment.clone(),
            done: item.done,
            due_date: item.due_date.map(|date| date.to_rfc3339()),
            google_task_id: item.google_task_id.clone(),
        }
    }
}

/// Rebuild storage-facing data from user-edited TOML while preserving
/// the original stable `todo_id`.
fn todo_item_from_editor_payload(
    payload: EditorTodoPayload,
    original_todo_id: Option<String>,
) -> Result<TodoItem> {
    let due_date = match payload.due_date {
        Some(raw) => Some(parse_due_date(&raw)?),
        None => None,
    };

    Ok(TodoItem {
        todo_id: original_todo_id,
        title: payload.title,
        comment: payload.comment,
        done: payload.done,
        due_date,
        google_task_id: payload.google_task_id,
    })
}

/// Render editable TOML and append commented scaffolding for omitted optionals.
///
/// The scaffolding exists purely as UX hints for humans; parsers ignore these
/// comments and only read uncommented keys.
fn render_editor_toml(todo_item: &TodoItem) -> Result<String> {
    let payload = EditorTodoPayload::from(todo_item);
    let mut content = toml::to_string_pretty(&payload)?;

    let mut scaffold_lines: Vec<&str> = Vec::new();
    if payload.comment.is_none() {
        scaffold_lines.push("# comment = \"Optional details\"");
    }
    if payload.due_date.is_none() {
        scaffold_lines.push("# due_date = \"2025-01-07T09:00:00Z\"");
    }
    if payload.google_task_id.is_none() {
        scaffold_lines.push("# google_task_id = \"Set by sync\"");
    }

    if !scaffold_lines.is_empty() {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(&scaffold_lines.join("\n"));
        content.push('\n');
    }

    Ok(content)
}

pub trait TodoEditor {
    fn edit_todo(&self, todo: &Todo) -> Result<Todo>;
    fn needs_terminal_restoration(&self) -> bool;
}

pub struct ExternalEditor;

fn choose_editor_command(visual: Option<&str>, editor: Option<&str>) -> String {
    let visual = visual.filter(|value| !value.trim().is_empty());
    let editor = editor.filter(|value| !value.trim().is_empty());
    visual.or(editor).unwrap_or(DEFAULT_EDITOR).to_string()
}

fn editor_command_from_env() -> String {
    choose_editor_command(
        env::var("VISUAL").ok().as_deref(),
        env::var("EDITOR").ok().as_deref(),
    )
}

fn parse_editor_command(raw: &str) -> Result<(String, Vec<String>)> {
    // Parse the editor command with shell-like splitting so args like `code -w`
    // or `vim -u NONE` work as expected without invoking a shell.
    let mut parts = shlex::split(raw).ok_or_else(|| {
        JugglerError::Other(format!(
            "Failed to parse editor command from VISUAL/EDITOR: {raw}"
        ))
    })?;

    // shlex can return an empty vector for whitespace-only input; treat that as a config error.
    if parts.is_empty() {
        return Err(JugglerError::Other(
            "Editor command is empty; set VISUAL/EDITOR or unset to use the default".to_string(),
        ));
    }

    // First token is the executable; remaining tokens are its args.
    let editor = parts.remove(0);
    Ok((editor, parts))
}

fn resolve_editor_command() -> Result<(String, Vec<String>)> {
    parse_editor_command(&editor_command_from_env())
}

impl TodoEditor for ExternalEditor {
    fn edit_todo(&self, todo: &Todo) -> Result<Todo> {
        let todo_item = TodoItem::from(todo);

        let toml_content = render_editor_toml(&todo_item)?;

        let mut temp_file = NamedTempFile::with_suffix(".toml")?;
        temp_file.write_all(toml_content.as_bytes())?;
        temp_file.flush()?;

        let temp_path = temp_file.path();
        let (editor, args) = resolve_editor_command()?;

        let status = Command::new(&editor).args(args).arg(temp_path).status()?;
        if !status.success() {
            return Err(JugglerError::Other(format!(
                "Editor {editor} exited with non-zero status"
            )));
        }

        let modified_content = fs::read_to_string(temp_path)?;
        let modified_payload: EditorTodoPayload = toml::from_str(&modified_content)?;
        let modified_item = todo_item_from_editor_payload(modified_payload, todo_item.todo_id)?;

        let mut updated_todo: Todo = modified_item.into();
        updated_todo.expanded = todo.expanded;
        updated_todo.selected = todo.selected;

        Ok(updated_todo)
    }

    fn needs_terminal_restoration(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub struct NoOpEditor;

#[cfg(test)]
impl TodoEditor for NoOpEditor {
    fn edit_todo(&self, todo: &Todo) -> Result<Todo> {
        Ok(todo.clone())
    }

    fn needs_terminal_restoration(&self) -> bool {
        false
    }
}

#[cfg(test)]
pub struct MockEditor {
    return_todo: Todo,
}

#[cfg(test)]
impl MockEditor {
    pub fn new(return_todo: Todo) -> Self {
        MockEditor { return_todo }
    }
}

#[cfg(test)]
impl TodoEditor for MockEditor {
    fn edit_todo(&self, _todo: &Todo) -> Result<Todo> {
        Ok(self.return_todo.clone())
    }

    fn needs_terminal_restoration(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EditorTodoPayload, choose_editor_command, parse_editor_command, render_editor_toml,
        todo_item_from_editor_payload,
    };
    use crate::config::DEFAULT_EDITOR;
    use crate::store::TodoItem;

    #[test]
    fn choose_editor_prefers_visual() {
        let raw = choose_editor_command(Some("code -w"), Some("vim"));
        let (editor, args) = parse_editor_command(&raw).expect("parse editor");
        assert_eq!(editor, "code");
        assert_eq!(args, vec!["-w"]);
    }

    #[test]
    fn choose_editor_falls_back_to_editor() {
        let raw = choose_editor_command(None, Some("vim -u NONE"));
        let (editor, args) = parse_editor_command(&raw).expect("parse editor");
        assert_eq!(editor, "vim");
        assert_eq!(args, vec!["-u", "NONE"]);
    }

    #[test]
    fn choose_editor_ignores_blank_visual() {
        let raw = choose_editor_command(Some("   "), Some("nano"));
        let (editor, args) = parse_editor_command(&raw).expect("parse editor");
        assert_eq!(editor, "nano");
        assert!(args.is_empty());
    }

    #[test]
    fn choose_editor_uses_default_when_unset() {
        let raw = choose_editor_command(None, None);
        assert_eq!(raw, DEFAULT_EDITOR);
    }

    #[test]
    fn parse_editor_handles_quoted_args() {
        let (editor, args) = parse_editor_command("vim -u \"NONE\"").expect("parse editor");
        assert_eq!(editor, "vim");
        assert_eq!(args, vec!["-u", "NONE"]);
    }

    #[test]
    fn parse_editor_rejects_invalid_shell_syntax() {
        let err = parse_editor_command("vim \"unclosed")
            .expect_err("invalid syntax should error")
            .to_string();
        assert!(err.contains("Failed to parse editor command"));
    }

    #[test]
    fn parse_editor_rejects_empty_command() {
        let err = parse_editor_command("   ")
            .expect_err("empty command should error")
            .to_string();
        assert!(err.contains("Editor command is empty"));
    }

    #[test]
    fn render_editor_toml_includes_optional_field_scaffolding() {
        let item = TodoItem {
            todo_id: Some("T9".to_string()),
            title: "Test item".to_string(),
            comment: None,
            done: false,
            due_date: None,
            google_task_id: None,
        };

        let content = render_editor_toml(&item).expect("render toml");
        assert!(content.contains("title = \"Test item\""));
        assert!(content.contains("done = false"));
        assert!(content.contains("# comment = \"Optional details\""));
        assert!(content.contains("# due_date = \"2025-01-07T09:00:00Z\""));
        assert!(content.contains("# google_task_id = \"Set by sync\""));
        assert!(!content.contains("todo_id"));
    }

    #[test]
    fn editor_payload_roundtrip_preserves_todo_id() {
        let payload: EditorTodoPayload = toml::from_str(
            r#"title = "Updated"
done = true
comment = "Updated comment"
due_date = "2025-01-01T00:00:00Z"
"#,
        )
        .expect("parse payload");

        let item = todo_item_from_editor_payload(payload, Some("T42".to_string()))
            .expect("payload to todo item");
        assert_eq!(item.todo_id.as_deref(), Some("T42"));
        assert_eq!(item.title, "Updated");
        assert!(item.done);
        assert_eq!(item.comment.as_deref(), Some("Updated comment"));
        assert_eq!(
            item.due_date
                .expect("due date")
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "2025-01-01T00:00:00Z"
        );
    }
}
