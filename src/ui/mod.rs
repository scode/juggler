mod editor;
mod event;
mod render;
mod state;
mod todo;
mod widgets;

use crossterm::event::KeyCode;
use ratatui::DefaultTerminal;

use crate::error::Result;
use crate::time::{SharedClock, system_clock};

pub use editor::{ExternalEditor, TodoEditor};
pub use todo::Todo;

#[cfg(test)]
use crate::time::fixed_clock;
#[cfg(test)]
pub use editor::{MockEditor, NoOpEditor};

use state::{TodoItems, UiState};
use widgets::AppMode;

pub const HELP_TEXT: &str = "o-open, j/k-nav, x-select, e-done, E-edit, c-new, s:+1d, S:-1d, p:+7d, P:-7d, t-custom, q-quit, Q-quit+sync. Ops affect selected; if none, the cursored item.";

pub const KEY_QUIT: KeyCode = KeyCode::Char('q');
pub const KEY_QUIT_WITH_SYNC: KeyCode = KeyCode::Char('Q');
pub const KEY_TOGGLE_EXPAND: KeyCode = KeyCode::Char('o');
pub const KEY_NEXT_ITEM: KeyCode = KeyCode::Char('j');
pub const KEY_PREVIOUS_ITEM: KeyCode = KeyCode::Char('k');
pub const KEY_TOGGLE_DONE: KeyCode = KeyCode::Char('e');
pub const KEY_EDIT: KeyCode = KeyCode::Char('E');
pub const KEY_TOGGLE_SELECT: KeyCode = KeyCode::Char('x');
pub const KEY_SNOOZE_DAY: KeyCode = KeyCode::Char('s');
pub const KEY_UNSNOOZE_DAY: KeyCode = KeyCode::Char('S');
pub const KEY_POSTPONE_WEEK: KeyCode = KeyCode::Char('p');
pub const KEY_PREPONE_WEEK: KeyCode = KeyCode::Char('P');
pub const KEY_CREATE: KeyCode = KeyCode::Char('c');
pub const KEY_CUSTOM_DELAY: KeyCode = KeyCode::Char('t');

pub struct App {
    exit: bool,
    sync_on_exit: bool,
    pub(self) items: TodoItems,
    pub(self) ui_state: UiState,
    pub(self) editor: Box<dyn TodoEditor>,
    pub(self) clock: SharedClock,
    pub(self) mode: AppMode,
}

/// The main application state and controller for the Juggler TUI.
///
/// `App` manages the event loop, coordinates rendering, and dispatches user input
/// to the appropriate handlers based on the current [`AppMode`]. It maintains the
/// list of todo items (split into pending and done sections), navigation state,
/// and handles transitions between normal operation and modal prompts.
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

    pub fn items(&self) -> Vec<Todo> {
        self.items.to_vec()
    }

    pub fn should_sync_on_exit(&self) -> bool {
        self.sync_on_exit
    }

    pub fn new_with_clock(
        items: Vec<Todo>,
        editor: Box<dyn TodoEditor>,
        clock: SharedClock,
    ) -> Self {
        let items = TodoItems::new(items);
        let ui_state = UiState::new(items.pending_count());

        App {
            exit: false,
            sync_on_exit: false,
            items,
            ui_state,
            editor,
            clock,
            mode: AppMode::Normal,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw_internal(frame))?;
            self.handle_events(terminal)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        text::{Span, Text},
        widgets::Widget,
    };

    use state::Section;
    use todo::{format_duration_compact, parse_relative_duration};
    use widgets::{AppMode, PromptAction, PromptOverlay, PromptWidget};

    fn spans_to_string(spans: &[Span]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn text_to_string(text: &Text) -> String {
        text.lines
            .iter()
            .map(|line| spans_to_string(&line.spans))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn get_all_items(app: &App) -> Vec<Todo> {
        app.items.to_vec()
    }

    struct TodoBuilder {
        todo: Todo,
    }

    impl TodoBuilder {
        fn new(title: &str) -> Self {
            Self {
                todo: Todo {
                    title: String::from(title),
                    comment: None,
                    expanded: false,
                    done: false,
                    selected: false,
                    due_date: None,
                    google_task_id: None,
                },
            }
        }

        fn comment(mut self, comment: &str) -> Self {
            self.todo.comment = Some(String::from(comment));
            self
        }

        fn expanded(mut self) -> Self {
            self.todo.expanded = true;
            self
        }

        fn done(mut self) -> Self {
            self.todo.done = true;
            self
        }

        fn selected(mut self) -> Self {
            self.todo.selected = true;
            self
        }

        fn due_date(mut self, due: chrono::DateTime<chrono::Utc>) -> Self {
            self.todo.due_date = Some(due);
            self
        }

        fn build(self) -> Todo {
            self.todo
        }
    }

    #[test]
    fn toggle_cursored_expanded_via_key_event() {
        let items = vec![TodoBuilder::new("a").comment("comment").build()];
        let mut app = App::new(items, Box::new(NoOpEditor));
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].expanded);
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(!get_all_items(&app)[0].expanded);
    }

    #[test]
    fn quit_with_sync_key_sets_sync_flag() {
        let items = vec![TodoBuilder::new("test item").build()];
        let mut app = App::new(items, Box::new(NoOpEditor));

        assert!(!app.exit);
        assert!(!app.should_sync_on_exit());

        app.handle_key_event_internal(KeyEvent::new(KEY_QUIT_WITH_SYNC, KeyModifiers::NONE));

        assert!(app.exit);
        assert!(app.should_sync_on_exit());
    }

    #[test]
    fn regular_quit_key_does_not_set_sync_flag() {
        let items = vec![TodoBuilder::new("test item").build()];
        let mut app = App::new(items, Box::new(NoOpEditor));

        assert!(!app.exit);
        assert!(!app.should_sync_on_exit());

        app.handle_key_event_internal(KeyEvent::new(KEY_QUIT, KeyModifiers::NONE));

        assert!(app.exit);
        assert!(!app.should_sync_on_exit());
    }

    #[test]
    fn expanded_text_indents_comment() {
        let todo = TodoBuilder::new("a")
            .comment("line1\nline2")
            .expanded()
            .build();
        assert_eq!(
            text_to_string(&todo.expanded_text(Utc::now())),
            "a >>>\n           line1\n           line2"
        );
    }

    #[test]
    fn display_text_prefixes_cursor() {
        let items = vec![
            TodoBuilder::new("a").build(),
            TodoBuilder::new("b").comment("c1\nc2").expanded().build(),
        ];
        let base = Utc::now();
        let app = App::new_with_clock(items, Box::new(NoOpEditor), fixed_clock(base));

        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ] a"
        );
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 1)),
            "  [ ] b >>>\n           c1\n           c2"
        );
    }

    #[test]
    fn display_text_shows_relative_time_for_future_due_date() {
        let base = Utc::now();
        let items = vec![
            TodoBuilder::new("future task")
                .due_date(base + chrono::Duration::hours(50))
                .build(),
        ];
        let app = App::new_with_clock(items, Box::new(NoOpEditor), fixed_clock(base));

        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ]   2d future task"
        );
    }

    #[test]
    fn expanded_display_includes_relative_time_and_comment_lines() {
        let base = Utc::now();
        let items = vec![
            TodoBuilder::new("a").build(),
            TodoBuilder::new("b")
                .comment("c1\nc2")
                .expanded()
                .due_date(base + chrono::Duration::hours(50))
                .build(),
        ];

        let app = App::new_with_clock(items, Box::new(NoOpEditor), fixed_clock(base));

        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ]   2d b >>>\n           c1\n           c2"
        );
    }

    #[test]
    fn selection_indicator_shows_x_for_selected_items() {
        let items = vec![
            TodoBuilder::new("first").build(),
            TodoBuilder::new("second").build(),
        ];
        let mut app = App::new(items, Box::new(NoOpEditor));

        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ] first"
        );

        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_SELECT, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [x] first"
        );

        app.select_next_internal();
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_SELECT, KeyModifiers::NONE));
        assert!(get_all_items(&app)[1].selected);
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 1)),
            "▶ [x] second"
        );
    }

    #[test]
    fn visual_indicators_for_todo_states() {
        let expanded_with_comment = Todo {
            title: String::from("Task with details"),
            comment: Some(String::from("Some details")),
            expanded: true,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            text_to_string(&expanded_with_comment.expanded_text(Utc::now())),
            "Task with details >>>\n           Some details"
        );
    }

    #[test]
    fn draw_displays_help_text() {
        let width = (HELP_TEXT.len() as u16).saturating_add(2);
        let backend = TestBackend::new(width, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(Vec::new(), Box::new(NoOpEditor));

        terminal.draw(|f| app.draw_internal(f)).unwrap();

        let buf = terminal.backend().buffer();
        let bottom_y = buf.area.bottom() - 1;
        let line: String = (0..buf.area.width)
            .map(|x| buf[(x, bottom_y)].symbol())
            .collect();

        assert_eq!(line.trim_end(), HELP_TEXT);
    }

    #[test]
    fn toggle_done_via_key_event() {
        let items = vec![
            TodoBuilder::new("pending task").build(),
            TodoBuilder::new("done task").done().build(),
        ];
        let mut app = App::new(items, Box::new(NoOpEditor));

        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].done);
        assert_eq!(app.items.pending_count(), 0);

        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(!get_all_items(&app)[0].done);
        assert_eq!(app.items.pending_count(), 1);
    }

    #[test]
    fn toggle_done_works_on_selected_items() {
        let items = vec![
            TodoBuilder::new("task 1").selected().build(),
            TodoBuilder::new("task 2").build(),
            TodoBuilder::new("task 3").selected().build(),
        ];
        let mut app = App::new(items, Box::new(NoOpEditor));
        app.select_next_internal();

        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));

        let final_items = get_all_items(&app);
        let task1 = final_items.iter().find(|t| t.title == "task 1").unwrap();
        let task2 = final_items.iter().find(|t| t.title == "task 2").unwrap();
        let task3 = final_items.iter().find(|t| t.title == "task 3").unwrap();

        assert!(task1.done);
        assert!(!task2.done);
        assert!(task3.done);
        assert_eq!(app.items.pending_count(), 1);

        assert!(!task1.selected);
        assert!(!task2.selected);
        assert!(!task3.selected);
    }

    #[test]
    fn toggle_done_works_on_cursor_when_no_selection() {
        let items = vec![
            Todo {
                title: String::from("task 1"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("task 2"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
        ];
        let mut app = App::new(items, Box::new(NoOpEditor));
        app.select_next_internal();

        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));

        assert!(!get_all_items(&app)[0].done);
        assert!(get_all_items(&app)[1].done);
        assert_eq!(app.items.pending_count(), 1);
    }

    #[test]
    fn snooze_functionality() {
        let items = vec![Todo {
            title: String::from("task 1"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let base = Utc::now();
        let mut app = App::new_with_clock(items, Box::new(NoOpEditor), fixed_clock(base));

        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let due1 = get_all_items(&app)[0]
            .due_date
            .expect("due set after snooze day");
        assert_eq!(due1, base + Duration::days(1));

        let prev = due1;
        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let due2 = get_all_items(&app)[0]
            .due_date
            .expect("due set after postpone week");
        assert_eq!(due2, prev + Duration::days(7));

        let prev2 = due2;
        app.handle_key_event_internal(KeyEvent::new(KEY_UNSNOOZE_DAY, KeyModifiers::NONE));
        let due3 = get_all_items(&app)[0]
            .due_date
            .expect("due set after unsnooze day");
        assert_eq!(due3, prev2 - Duration::days(1));

        let prev3 = due3;
        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        let due4 = get_all_items(&app)[0]
            .due_date
            .expect("due set after prepone week");
        assert_eq!(due4, prev3 - Duration::days(7));
    }

    #[test]
    fn snooze_with_past_due_date() {
        let base = Utc::now();
        let past_date = base - Duration::days(2);
        let items = vec![Todo {
            title: String::from("overdue task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(past_date),
            google_task_id: None,
        }];
        let mut app = App::new_with_clock(items, Box::new(NoOpEditor), fixed_clock(base));

        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));

        let new_due_date = get_all_items(&app)[0].due_date.unwrap();

        let expected = base + Duration::days(1);

        assert_eq!(new_due_date, expected);
        assert!(new_due_date > base);
    }

    #[test]
    fn snooze_with_future_due_date() {
        let base = Utc::now();
        let future_date = base + Duration::days(3);
        let items = vec![Todo {
            title: String::from("future task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(future_date),
            google_task_id: None,
        }];
        let mut app = App::new(items, Box::new(NoOpEditor));

        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));

        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        let expected_due_date = future_date + Duration::days(1);

        assert_eq!(new_due_date, expected_due_date);
        assert!(new_due_date > future_date);
    }

    #[test]
    fn snooze_with_no_due_date() {
        let items = vec![Todo {
            title: String::from("task without due date"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let app = App::new(items, Box::new(NoOpEditor));

        let base = Utc::now();
        let mut app =
            App::new_with_clock(app.items.to_vec(), Box::new(NoOpEditor), fixed_clock(base));
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        let expected = base + Duration::days(1);
        assert_eq!(new_due_date, expected);
    }

    #[test]
    fn postpone_week_with_no_due_date_sets_future() {
        let items = vec![Todo {
            title: String::from("task without due date"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let app = App::new(items, Box::new(NoOpEditor));

        let base = Utc::now();
        let mut app =
            App::new_with_clock(app.items.to_vec(), Box::new(NoOpEditor), fixed_clock(base));
        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        let expected = base + Duration::days(7);
        assert_eq!(new_due_date, expected);
    }

    #[test]
    fn prepone_week_with_no_due_date_sets_past() {
        let items = vec![Todo {
            title: String::from("task without due date"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let app = App::new(items, Box::new(NoOpEditor));

        let base = Utc::now();
        let mut app =
            App::new_with_clock(app.items.to_vec(), Box::new(NoOpEditor), fixed_clock(base));
        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        let expected = base - Duration::days(7);
        assert_eq!(new_due_date, expected);
    }

    #[test]
    fn postpone_week_from_future_due_adds_7() {
        let future_date = Utc::now() + Duration::days(3);
        let items = vec![Todo {
            title: String::from("task with future due"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(future_date),
            google_task_id: None,
        }];
        let mut app = App::new(items, Box::new(NoOpEditor));

        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        assert_eq!(new_due_date, future_date + Duration::days(7));
    }

    #[test]
    fn prepone_week_from_future_due_subtracts_7() {
        let future_date = Utc::now() + Duration::days(3);
        let items = vec![Todo {
            title: String::from("task with future due"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(future_date),
            google_task_id: None,
        }];
        let mut app = App::new(items, Box::new(NoOpEditor));

        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        assert_eq!(new_due_date, future_date - Duration::days(7));
    }

    #[test]
    fn snooze_multiple_selected_items_mixed_due_dates() {
        let base = Utc::now();
        let past_date = base - Duration::days(1);
        let future_date = base + Duration::days(2);

        let items = vec![
            Todo {
                title: String::from("overdue task"),
                comment: None,
                expanded: false,
                done: false,
                selected: true,
                due_date: Some(past_date),
                google_task_id: None,
            },
            Todo {
                title: String::from("future task"),
                comment: None,
                expanded: false,
                done: false,
                selected: true,
                due_date: Some(future_date),
                google_task_id: None,
            },
            Todo {
                title: String::from("no due date task"),
                comment: None,
                expanded: false,
                done: false,
                selected: true,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("not selected task"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: Some(past_date),
                google_task_id: None,
            },
        ];
        let app = App::new(items, Box::new(NoOpEditor));

        let mut app =
            App::new_with_clock(app.items.to_vec(), Box::new(NoOpEditor), fixed_clock(base));
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));

        let items_after_snooze = get_all_items(&app);
        let overdue = items_after_snooze
            .iter()
            .find(|t| t.title == "overdue task")
            .unwrap();
        let future = items_after_snooze
            .iter()
            .find(|t| t.title == "future task")
            .unwrap();
        let no_due = items_after_snooze
            .iter()
            .find(|t| t.title == "no due date task")
            .unwrap();

        let d1_s = overdue.due_date.unwrap();
        let d2_s = future.due_date.unwrap();
        let d3_s = no_due.due_date.unwrap();

        assert_eq!(d1_s, base + Duration::days(1));
        assert_eq!(d2_s, future_date + Duration::days(1));
        assert_eq!(d3_s, base + Duration::days(1));

        app.handle_key_event_internal(KeyEvent::new(KEY_UNSNOOZE_DAY, KeyModifiers::NONE));
        let items_after_unsnooze = get_all_items(&app);
        let overdue = items_after_unsnooze
            .iter()
            .find(|t| t.title == "overdue task")
            .unwrap();
        let future = items_after_unsnooze
            .iter()
            .find(|t| t.title == "future task")
            .unwrap();
        let no_due = items_after_unsnooze
            .iter()
            .find(|t| t.title == "no due date task")
            .unwrap();

        let d1_uns = overdue.due_date.unwrap();
        let d2_uns = future.due_date.unwrap();
        let d3_uns = no_due.due_date.unwrap();
        assert_eq!(d1_uns, d1_s - Duration::days(1));
        assert_eq!(d2_uns, d2_s - Duration::days(1));
        assert_eq!(d3_uns, d3_s - Duration::days(1));

        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let items_after_postpone = get_all_items(&app);
        let overdue = items_after_postpone
            .iter()
            .find(|t| t.title == "overdue task")
            .unwrap();
        let future = items_after_postpone
            .iter()
            .find(|t| t.title == "future task")
            .unwrap();
        let no_due = items_after_postpone
            .iter()
            .find(|t| t.title == "no due date task")
            .unwrap();

        let d1_p = overdue.due_date.unwrap();
        let d2_p = future.due_date.unwrap();
        let d3_p = no_due.due_date.unwrap();
        assert_eq!(d1_p, d1_uns + Duration::days(7));
        assert_eq!(d2_p, d2_uns + Duration::days(7));
        assert_eq!(d3_p, d3_uns + Duration::days(7));

        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        let items_after_prepone = get_all_items(&app);
        let overdue = items_after_prepone
            .iter()
            .find(|t| t.title == "overdue task")
            .unwrap();
        let future = items_after_prepone
            .iter()
            .find(|t| t.title == "future task")
            .unwrap();
        let no_due = items_after_prepone
            .iter()
            .find(|t| t.title == "no due date task")
            .unwrap();

        let d1_prep = overdue.due_date.unwrap();
        let d2_prep = future.due_date.unwrap();
        let d3_prep = no_due.due_date.unwrap();
        assert_eq!(d1_prep, d1_p - Duration::days(7));
        assert_eq!(d2_prep, d2_p - Duration::days(7));
        assert_eq!(d3_prep, d3_p - Duration::days(7));

        let final_items = get_all_items(&app);
        let not_selected = final_items
            .iter()
            .find(|t| t.title == "not selected task")
            .unwrap();
        assert_eq!(not_selected.due_date, Some(past_date));
    }

    #[test]
    fn selection_persists_for_due_date_operations() {
        use crossterm::event::KeyModifiers;

        let items = vec![
            Todo {
                title: String::from("a"),
                comment: None,
                expanded: false,
                done: false,
                selected: true,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("b"),
                comment: None,
                expanded: false,
                done: false,
                selected: true,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("c"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
        ];
        let mut app = App::new(items, Box::new(NoOpEditor));

        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        app.handle_key_event_internal(KeyEvent::new(KEY_UNSNOOZE_DAY, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        app.mode = AppMode::Prompt(PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });
        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);
    }

    #[test]
    fn create_new_item_adds_valid_todo() {
        let initial_items = vec![Todo {
            title: String::from("existing task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let new_todo = Todo {
            title: String::from("new task"),
            comment: Some(String::from("new comment")),
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(Utc::now()),
            google_task_id: None,
        };

        let mock_editor = MockEditor::new(new_todo.clone());
        let mut app = App::new(initial_items, Box::new(mock_editor));

        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);
        assert_eq!(app.ui_state.current_section, Section::Pending);
        assert_eq!(app.ui_state.pending_index, 0);

        app.create_new_item();

        assert_eq!(get_all_items(&app).len(), 2);
        assert_eq!(get_all_items(&app)[1].title, "new task");
        assert_eq!(
            get_all_items(&app)[1].comment,
            Some(String::from("new comment"))
        );
        assert!(!get_all_items(&app)[1].done);
        assert_eq!(app.items.pending_count(), 2);

        assert_eq!(app.ui_state.current_section, Section::Pending);
        assert_eq!(app.ui_state.pending_index, 1);
    }

    #[test]
    fn create_new_item_rejects_empty_title() {
        let initial_items = vec![Todo {
            title: String::from("existing task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let empty_todo = Todo {
            title: String::from("   "),
            comment: Some(String::from("comment")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };

        let mock_editor = MockEditor::new(empty_todo);
        let mut app = App::new(initial_items, Box::new(mock_editor));

        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);

        app.create_new_item();

        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);
        assert_eq!(get_all_items(&app)[0].title, "existing task");
    }

    #[test]
    fn create_new_item_handles_done_todo() {
        let initial_items = vec![Todo {
            title: String::from("existing task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];

        let done_todo = Todo {
            title: String::from("completed task"),
            comment: None,
            expanded: false,
            done: true,
            selected: false,
            due_date: None,
            google_task_id: None,
        };

        let mock_editor = MockEditor::new(done_todo);
        let mut app = App::new(initial_items, Box::new(mock_editor));

        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);
        assert_eq!(app.ui_state.current_section, Section::Pending);

        app.create_new_item();

        assert_eq!(get_all_items(&app).len(), 2);
        assert_eq!(get_all_items(&app)[1].title, "completed task");
        assert!(get_all_items(&app)[1].done);
        assert_eq!(app.items.pending_count(), 1);

        assert_eq!(app.ui_state.current_section, Section::Done);
        assert_eq!(app.ui_state.done_index, 0);
    }

    #[test]
    fn create_new_item_in_empty_list() {
        let new_todo = Todo {
            title: String::from("first task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };

        let mock_editor = MockEditor::new(new_todo);
        let mut app = App::new(Vec::new(), Box::new(mock_editor));

        assert_eq!(get_all_items(&app).len(), 0);
        assert_eq!(app.items.pending_count(), 0);

        app.create_new_item();

        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(get_all_items(&app)[0].title, "first task");
        assert!(!get_all_items(&app)[0].done);
        assert_eq!(app.items.pending_count(), 1);

        assert_eq!(app.ui_state.current_section, Section::Pending);
        assert_eq!(app.ui_state.pending_index, 0);
    }

    #[test]
    fn prompt_widget_clears_area_and_renders_text() {
        use ratatui::{buffer::Buffer, layout::Rect};

        let area = Rect::new(0, 0, 20, 2);
        let mut buf = Buffer::empty(area);

        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_symbol("X");
            }
        }

        let message = "Prompt: ";
        let input = "abc";
        PromptWidget::new(message, input).render(area, &mut buf);

        let line0: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        let expected_content = format!("{}{}", message, input);
        let mut expected_line0 = expected_content.clone();
        if expected_line0.len() < area.width as usize {
            expected_line0.push_str(&" ".repeat(area.width as usize - expected_line0.len()));
        } else {
            expected_line0.truncate(area.width as usize);
        }
        assert_eq!(line0, expected_line0);

        let line1: String = (0..area.width)
            .map(|x| buf[(x, area.y + 1)].symbol())
            .collect();
        assert_eq!(line1, " ".repeat(area.width as usize));
    }

    #[test]
    fn prompt_widget_truncates_to_width() {
        use ratatui::{buffer::Buffer, layout::Rect};

        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);

        PromptWidget::new("Hello", "World").render(area, &mut buf);

        let line: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        assert_eq!(line, "Hello");
    }

    #[test]
    fn prompt_widget_handles_multibyte_utf8() {
        use ratatui::{buffer::Buffer, layout::Rect};

        // "café" is 4 characters but 5 bytes (é is 2 bytes in UTF-8).
        // With width=5, all 4 characters should fit without truncation.
        // The old bug used byte count (5) > width (5) which was false,
        // but it compared inconsistently. This test ensures we use
        // character count correctly.
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);

        PromptWidget::new("", "café!").render(area, &mut buf);

        let line: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        assert_eq!(line, "café!");
    }

    #[test]
    fn prompt_widget_truncates_multibyte_utf8() {
        use ratatui::{buffer::Buffer, layout::Rect};

        // "café" is 4 characters. With width=3, only "caf" should render.
        let area = Rect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);

        PromptWidget::new("", "café").render(area, &mut buf);

        let line: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        assert_eq!(line, "caf");
    }

    #[test]
    fn prompt_mode_key_end_to_end_sets_due_from_now() {
        use crossterm::event::KeyModifiers;

        let items = vec![Todo {
            title: String::from("task 1"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let base = Utc::now();
        let mut app = App::new_with_clock(items, Box::new(NoOpEditor), fixed_clock(base));

        app.mode = AppMode::Prompt(PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });

        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let due = get_all_items(&app)[0]
            .due_date
            .expect("due date should be set");
        let expected = base + Duration::days(1);
        assert_eq!(due, expected);
        assert!(matches!(app.mode, AppMode::Normal));
    }

    #[test]
    fn parse_relative_duration_valid_inputs() {
        let cases = [
            ("0s", Duration::seconds(0)),
            ("5s", Duration::seconds(5)),
            ("59s", Duration::seconds(59)),
            ("1m", Duration::minutes(1)),
            ("10m", Duration::minutes(10)),
            ("59m", Duration::minutes(59)),
            ("1h", Duration::hours(1)),
            ("12h", Duration::hours(12)),
            ("23h", Duration::hours(23)),
            ("1d", Duration::days(1)),
            ("10d", Duration::days(10)),
            ("-5s", Duration::seconds(-5)),
            ("-2m", Duration::minutes(-2)),
            ("-3h", Duration::hours(-3)),
            ("-4d", Duration::days(-4)),
            ("  5d  ", Duration::days(5)),
            ("+7d", Duration::days(7)),
            ("5 m", Duration::minutes(5)),
        ];

        for (input, expected) in cases {
            let got = parse_relative_duration(input).expect("should parse");
            assert_eq!(got, expected, "input={input}");
        }
    }

    #[test]
    fn parse_relative_duration_invalid_inputs() {
        let cases = [
            "", " ", "s", "d", "+", "-", "+d", "-h", "5", "d5", "5x", "5days", "--5d", "++5d",
        ];

        for input in cases {
            assert!(parse_relative_duration(input).is_none(), "input={input}");
        }
    }

    #[test]
    fn duration_compact_format_round_trip_for_canonical_strings() {
        let canonical = [
            "0s", "1s", "59s", "1m", "2m", "59m", "1h", "2h", "23h", "1d", "2d", "10d", "-1s",
            "-59s", "-1m", "-59m", "-1h", "-23h", "-1d", "-10d",
        ];

        for s in canonical {
            let dur = parse_relative_duration(s).expect("parse canonical");
            let back = format_duration_compact(dur);
            assert_eq!(back, s, "round-trip failed for {s}");
        }
    }
}
