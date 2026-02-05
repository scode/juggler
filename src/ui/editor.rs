use std::{env, fs, io::Write, process::Command};

use tempfile::NamedTempFile;

use crate::config::DEFAULT_EDITOR;
use crate::error::{JugglerError, Result};
use crate::store::TodoItem;

use super::todo::Todo;

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

        let yaml_content = serde_yaml::to_string(&todo_item)?;

        let mut temp_file = NamedTempFile::with_suffix(".yaml")?;
        temp_file.write_all(yaml_content.as_bytes())?;
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
        let modified_item: TodoItem = serde_yaml::from_str(&modified_content)?;

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
    use super::{choose_editor_command, parse_editor_command};
    use crate::config::DEFAULT_EDITOR;

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
}
