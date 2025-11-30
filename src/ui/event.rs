use chrono::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::error::Result;

use super::state::Section;
use super::todo::{Todo, parse_relative_duration};
use super::widgets::{AppMode, PromptAction, PromptOverlay};
use super::{
    App, KEY_CREATE, KEY_CUSTOM_DELAY, KEY_EDIT, KEY_NEXT_ITEM, KEY_POSTPONE_WEEK,
    KEY_PREPONE_WEEK, KEY_PREVIOUS_ITEM, KEY_QUIT, KEY_QUIT_WITH_SYNC, KEY_SNOOZE_DAY,
    KEY_TOGGLE_DONE, KEY_TOGGLE_EXPAND, KEY_TOGGLE_SELECT, KEY_UNSNOOZE_DAY,
};

impl App {
    pub(super) fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => match &self.mode {
                AppMode::Prompt(_) => self.handle_prompt_mode_key(key_event),
                AppMode::Normal => self.handle_normal_mode_key(key_event, terminal)?,
            },
            _ => {}
        };
        Ok(())
    }

    pub(super) fn handle_prompt_mode_key(&mut self, key_event: KeyEvent) {
        if let AppMode::Prompt(overlay) = &mut self.mode {
            match key_event.code {
                KeyCode::Enter => {
                    let finished = overlay.buffer.clone();
                    let action = overlay.action;
                    self.mode = AppMode::Normal;
                    match action {
                        PromptAction::CustomDelay => {
                            if let Some(duration) = parse_relative_duration(&finished) {
                                self.delay_from_now(duration);
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Char(c) => {
                    let modifiers = key_event.modifiers;
                    if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT {
                        overlay.buffer.push(c);
                    }
                }
                KeyCode::Backspace => {
                    overlay.buffer.pop();
                }
                _ => {}
            }
        }
    }

    pub(super) fn handle_normal_mode_key(
        &mut self,
        key_event: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        if (key_event.code == KEY_EDIT || key_event.code == KEY_CREATE)
            && self.editor.needs_terminal_restoration()
        {
            ratatui::restore();
            if key_event.code == KEY_EDIT {
                self.edit_item();
            } else {
                self.create_new_item();
            }
            *terminal = ratatui::init();
        } else if key_event.code == KEY_EDIT {
            self.edit_item();
        } else if key_event.code == KEY_CREATE {
            self.create_new_item();
        } else if key_event.code == KEY_CUSTOM_DELAY {
            self.handle_custom_delay(terminal);
        } else {
            self.handle_key_event_internal(key_event);
        }
        Ok(())
    }

    pub(super) fn handle_key_event_internal(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KEY_QUIT => self.exit(),
            KEY_QUIT_WITH_SYNC => self.exit_with_sync(),
            KEY_NEXT_ITEM => self.select_next_internal(),
            KEY_PREVIOUS_ITEM => self.select_previous_internal(),
            KEY_TOGGLE_EXPAND => self.toggle_cursored_expanded(),
            KEY_TOGGLE_DONE => self.toggle_done(),
            KEY_EDIT => self.edit_item(),
            KEY_TOGGLE_SELECT => self.toggle_select(),
            KEY_SNOOZE_DAY => self.snooze_day(),
            KEY_UNSNOOZE_DAY => self.unsnooze_day(),
            KEY_POSTPONE_WEEK => self.snooze_week(),
            KEY_PREPONE_WEEK => self.unsnooze_week(),
            KEY_CREATE => self.create_new_item(),
            _ => {}
        }
    }

    fn toggle_cursored_expanded(&mut self) {
        if let Some(item) = self.ui_state.get_cursored_item_mut(&mut self.items) {
            item.expanded = !item.expanded;
        }
    }

    pub(super) fn select_next_internal(&mut self) {
        self.ui_state
            .select_next(self.items.pending_count(), self.items.done_count());
    }

    pub(super) fn select_previous_internal(&mut self) {
        self.ui_state
            .select_previous(self.items.pending_count(), self.items.done_count());
    }

    fn toggle_done(&mut self) {
        let mut pending_selected: Vec<usize> = self.items.pending_selected_indices().collect();
        let mut done_selected: Vec<usize> = self.items.done_selected_indices().collect();

        if !pending_selected.is_empty() || !done_selected.is_empty() {
            pending_selected.sort_unstable();
            done_selected.sort_unstable();

            for i in pending_selected.into_iter().rev() {
                self.items.toggle_done(Section::Pending, i);
            }
            for i in done_selected.into_iter().rev() {
                self.items.toggle_done(Section::Done, i);
            }
        } else {
            let section = self.ui_state.current_section;
            let index = self.ui_state.current_index();
            self.items.toggle_done(section, index);
        }

        self.adjust_indices_after_toggle();
    }

    fn toggle_select(&mut self) {
        if let Some(item) = self.ui_state.get_cursored_item_mut(&mut self.items) {
            item.selected = !item.selected;
        }
    }

    fn snooze(&mut self, duration: Duration) {
        let now = self.clock.now();

        let calculate_new_due =
            |current_due: Option<chrono::DateTime<chrono::Utc>>| -> chrono::DateTime<chrono::Utc> {
                if let Some(current_due) = current_due {
                    if current_due <= now {
                        now + duration
                    } else {
                        current_due + duration
                    }
                } else {
                    now + duration
                }
            };

        let pending_selected: Vec<usize> = self.items.pending_selected_indices().collect();
        let done_selected: Vec<usize> = self.items.done_selected_indices().collect();

        if !pending_selected.is_empty() || !done_selected.is_empty() {
            for i in pending_selected {
                if let Some(item) = self.items.get_mut(Section::Pending, i) {
                    item.due_date = Some(calculate_new_due(item.due_date));
                }
            }
            for i in done_selected {
                if let Some(item) = self.items.get_mut(Section::Done, i) {
                    item.due_date = Some(calculate_new_due(item.due_date));
                }
            }
        } else if let Some(item) = self.ui_state.get_cursored_item_mut(&mut self.items) {
            item.due_date = Some(calculate_new_due(item.due_date));
        }
    }

    fn snooze_day(&mut self) {
        self.snooze(Duration::days(1));
    }

    fn unsnooze_day(&mut self) {
        self.snooze(Duration::days(-1));
    }

    fn snooze_week(&mut self) {
        self.snooze(Duration::days(7));
    }

    fn unsnooze_week(&mut self) {
        self.snooze(Duration::days(-7));
    }

    pub(super) fn edit_item(&mut self) {
        let section = self.ui_state.current_section;
        let index = self.ui_state.current_index();

        if let Some(item) = self.items.get(section, index) {
            let result = self.editor.edit_todo(item);

            if let Ok(updated_item) = result {
                let done_changed = item.done != updated_item.done;

                if done_changed {
                    let _ = match section {
                        Section::Pending => self.items.pending.remove(index),
                        Section::Done => self.items.done.remove(index),
                    };
                    self.items.push(updated_item);
                    self.adjust_indices_after_toggle();
                } else if let Some(existing) = self.items.get_mut(section, index) {
                    *existing = updated_item;
                }
            }
        }
    }

    pub(super) fn create_new_item(&mut self) {
        let new_todo = Todo {
            title: String::new(),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };

        let result = self.editor.edit_todo(&new_todo);

        if let Ok(created_item) = result
            && !created_item.title.trim().is_empty()
        {
            let is_done = created_item.done;
            self.items.push(created_item);

            if !is_done {
                self.ui_state.current_section = Section::Pending;
                self.ui_state.pending_index = self.items.pending_count().saturating_sub(1);
            } else {
                self.ui_state.current_section = Section::Done;
                self.ui_state.done_index = self.items.done_count().saturating_sub(1);
            }
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn exit_with_sync(&mut self) {
        self.exit = true;
        self.sync_on_exit = true;
    }

    fn adjust_indices_after_toggle(&mut self) {
        self.ui_state
            .adjust_indices(self.items.pending_count(), self.items.done_count());
    }

    fn handle_custom_delay(&mut self, terminal: &mut DefaultTerminal) {
        let _ = terminal;
        self.mode = AppMode::Prompt(PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });
    }

    fn delay_from_now(&mut self, duration: Duration) {
        let now = self.clock.now();
        let target_due = now + duration;

        let pending_selected: Vec<usize> = self.items.pending_selected_indices().collect();
        let done_selected: Vec<usize> = self.items.done_selected_indices().collect();

        if !pending_selected.is_empty() || !done_selected.is_empty() {
            for i in pending_selected {
                if let Some(item) = self.items.get_mut(Section::Pending, i) {
                    item.due_date = Some(target_due);
                }
            }
            for i in done_selected {
                if let Some(item) = self.items.get_mut(Section::Done, i) {
                    item.due_date = Some(target_due);
                }
            }
        } else if let Some(item) = self.ui_state.get_cursored_item_mut(&mut self.items) {
            item.due_date = Some(target_due);
        }
    }
}
