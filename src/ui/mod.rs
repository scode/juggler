//! TUI runtime orchestration.
//!
//! This module wires the UI submodules into the main event loop and exposes
//! `App` as the runtime entry point for interactive mode.
//!
//! The loop renders state, reads input, maps input to actions, runs the
//! reducer, and executes requested side effects such as external editing.

mod editor;
mod event;
mod keymap;
mod model;
mod todo;
mod update;
mod view;
mod widgets;

use ratatui::DefaultTerminal;

use crate::error::Result;
use crate::time::{SharedClock, system_clock};

pub use editor::{ExternalEditor, TodoEditor};
#[cfg(test)]
pub use editor::{MockEditor, NoOpEditor};
pub use todo::Todo;

use event::read_action;
use model::AppModel;
use update::{Action, SideEffect, update};
use view::draw;

pub struct App {
    model: AppModel,
    editor: Box<dyn TodoEditor>,
    clock: SharedClock,
}

/// The main application state and controller for the Juggler TUI.
///
/// `App` manages the event loop, coordinates rendering, and dispatches user input
/// to the appropriate handlers. It maintains todo list state and owns runtime-only
/// dependencies like editor integration and clock access.
///
/// # Example
///
/// ```ignore
/// let todos = store::load_todos(&path);
/// let mut app = App::new(todos, Box::new(ExternalEditor));
/// let mut terminal = ratatui::init();
/// app.run(&mut terminal)?;
/// let updated_todos = app.items();
/// ```
impl App {
    pub fn new(items: Vec<Todo>, editor: Box<dyn TodoEditor>) -> Self {
        Self::new_with_clock(items, editor, system_clock())
    }

    pub fn new_with_clock(
        items: Vec<Todo>,
        editor: Box<dyn TodoEditor>,
        clock: SharedClock,
    ) -> Self {
        Self {
            model: AppModel::new(items),
            editor,
            clock,
        }
    }

    pub fn items(&self) -> Vec<Todo> {
        self.model.items.to_vec()
    }

    pub fn should_sync_on_exit(&self) -> bool {
        self.model.sync_on_exit
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.model.exit {
            let now = self.clock.now();
            terminal.draw(|frame| draw(frame, &self.model, now))?;

            if let Some(action) = read_action(&self.model.mode)? {
                self.process_action(action, Some(terminal));
            }
        }
        Ok(())
    }

    fn process_action(&mut self, action: Action, terminal: Option<&mut DefaultTerminal>) {
        if let Some(side_effect) = update(&mut self.model, action, self.clock.now()) {
            self.handle_side_effect(side_effect, terminal);
        }
    }

    fn handle_side_effect(
        &mut self,
        side_effect: SideEffect,
        terminal: Option<&mut DefaultTerminal>,
    ) {
        match side_effect {
            SideEffect::EditItem {
                section,
                index,
                original,
            } => {
                if let Ok(updated_item) = self.run_editor(&original, terminal) {
                    let _ = update(
                        &mut self.model,
                        Action::ApplyEditedItem {
                            section,
                            index,
                            updated_item,
                        },
                        self.clock.now(),
                    );
                }
            }
            SideEffect::CreateItem { template } => {
                if let Ok(created_item) = self.run_editor(&template, terminal) {
                    let _ = update(
                        &mut self.model,
                        Action::ApplyCreatedItem { created_item },
                        self.clock.now(),
                    );
                }
            }
        }
    }

    fn run_editor(&self, todo: &Todo, terminal: Option<&mut DefaultTerminal>) -> Result<Todo> {
        if self.editor.needs_terminal_restoration() {
            if let Some(terminal) = terminal {
                ratatui::restore();
                let result = self.editor.edit_todo(todo);
                *terminal = ratatui::init();
                result
            } else {
                self.editor.edit_todo(todo)
            }
        } else {
            self.editor.edit_todo(todo)
        }
    }

    #[cfg(test)]
    fn dispatch_action_for_test(&mut self, action: Action) {
        self.process_action(action, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{JugglerError, Result};
    use crate::ui::keymap::Action as NormalAction;

    fn todo(title: &str) -> Todo {
        Todo {
            title: title.to_string(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }
    }

    struct FailingEditor;

    impl TodoEditor for FailingEditor {
        fn edit_todo(&self, _todo: &Todo) -> Result<Todo> {
            Err(JugglerError::new("editor failed"))
        }

        fn needs_terminal_restoration(&self) -> bool {
            false
        }
    }

    #[test]
    fn app_construction_and_accessors() {
        let app = App::new(vec![todo("a"), todo("b")], Box::new(NoOpEditor));
        assert_eq!(app.items().len(), 2);
        assert!(!app.should_sync_on_exit());
    }

    #[test]
    fn quit_action_sets_sync_flags_correctly() {
        let mut app = App::new(vec![todo("a")], Box::new(NoOpEditor));
        app.dispatch_action_for_test(Action::Normal(NormalAction::Quit));
        assert!(app.model.exit);
        assert!(!app.should_sync_on_exit());

        let mut app = App::new(vec![todo("a")], Box::new(NoOpEditor));
        app.dispatch_action_for_test(Action::Normal(NormalAction::QuitWithSync));
        assert!(app.model.exit);
        assert!(app.should_sync_on_exit());
    }

    #[test]
    fn edit_side_effect_round_trip_applies_editor_result() {
        let updated = Todo {
            title: "updated".to_string(),
            comment: Some("details".to_string()),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        let mut app = App::new(vec![todo("original")], Box::new(MockEditor::new(updated)));

        app.dispatch_action_for_test(Action::Normal(NormalAction::Edit));

        let items = app.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "updated");
        assert_eq!(items[0].comment.as_deref(), Some("details"));
    }

    #[test]
    fn edit_side_effect_preserves_state_on_editor_error() {
        let mut app = App::new(vec![todo("original")], Box::new(FailingEditor));

        app.dispatch_action_for_test(Action::Normal(NormalAction::Edit));

        let items = app.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "original");
    }

    #[test]
    fn edit_side_effect_ignores_empty_titles() {
        let mut empty_title = todo(" ");
        empty_title.comment = Some("ignored".to_string());
        let mut app = App::new(
            vec![todo("existing")],
            Box::new(MockEditor::new(empty_title)),
        );

        app.dispatch_action_for_test(Action::Normal(NormalAction::Edit));

        let items = app.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "existing");
        assert!(items[0].comment.is_none());
    }

    #[test]
    fn create_side_effect_round_trip_applies_editor_result() {
        let mut created = todo("new task");
        created.comment = Some("new comment".to_string());
        let mut app = App::new(vec![todo("existing")], Box::new(MockEditor::new(created)));

        app.dispatch_action_for_test(Action::Normal(NormalAction::Create));

        let items = app.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].title, "new task");
        assert_eq!(items[1].comment.as_deref(), Some("new comment"));
    }

    #[test]
    fn create_side_effect_ignores_empty_titles() {
        let mut empty_title = todo(" ");
        empty_title.comment = Some("ignored".to_string());
        let mut app = App::new(
            vec![todo("existing")],
            Box::new(MockEditor::new(empty_title)),
        );

        app.dispatch_action_for_test(Action::Normal(NormalAction::Create));

        let items = app.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "existing");
    }

    #[test]
    fn create_side_effect_preserves_state_on_editor_error() {
        let mut app = App::new(vec![todo("existing")], Box::new(FailingEditor));

        app.dispatch_action_for_test(Action::Normal(NormalAction::Create));

        let items = app.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "existing");
    }
}
