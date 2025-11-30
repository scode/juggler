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

impl TodoEditor for ExternalEditor {
    fn edit_todo(&self, todo: &Todo) -> Result<Todo> {
        let todo_item = TodoItem {
            title: todo.title.clone(),
            comment: todo.comment.clone(),
            done: todo.done,
            due_date: todo.due_date,
            google_task_id: todo.google_task_id.clone(),
        };

        let yaml_content = serde_yaml::to_string(&todo_item)?;

        let mut temp_file = NamedTempFile::with_suffix(".yaml")?;
        temp_file.write_all(yaml_content.as_bytes())?;
        temp_file.flush()?;

        let temp_path = temp_file.path();
        let editor = env::var("EDITOR").unwrap_or_else(|_| DEFAULT_EDITOR.to_string());

        let status = Command::new(&editor).arg(temp_path).status()?;
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
