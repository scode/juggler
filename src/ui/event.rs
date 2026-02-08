use chrono::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::error::Result;

use super::state::Section;
use super::todo::{Todo, parse_relative_duration};
use super::widgets::{AppMode, PromptAction, PromptOverlay};
use super::{Action, App, action_for_key, key_for_action};

fn sorted_indices<I: Iterator<Item = usize>>(iter: I) -> Vec<usize> {
    let mut v: Vec<usize> = iter.collect();
    v.sort_unstable();
    v
}

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
        let Some(action) = action_for_key(key_event.code) else {
            return Ok(());
        };

        let edit_key = key_for_action(Action::Edit);
        let create_key = key_for_action(Action::Create);
        let is_edit_or_create = key_event.code == edit_key || key_event.code == create_key;

        if is_edit_or_create && self.editor.needs_terminal_restoration() {
            ratatui::restore();
            if key_event.code == edit_key {
                self.edit_item();
            } else {
                self.create_new_item();
            }
            *terminal = ratatui::init();
        } else {
            self.handle_action_internal(action);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn handle_key_event_internal(&mut self, key_event: KeyEvent) {
        if let Some(action) = action_for_key(key_event.code) {
            self.handle_action_internal(action);
        }
    }

    fn handle_action_internal(&mut self, action: Action) {
        match action {
            Action::Quit => self.exit(),
            Action::QuitWithSync => self.exit_with_sync(),
            Action::NextItem => self.select_next_internal(),
            Action::PreviousItem => self.select_previous_internal(),
            Action::ToggleExpand => self.toggle_cursored_expanded(),
            Action::ToggleDone => self.toggle_done(),
            Action::Edit => self.edit_item(),
            Action::ToggleSelect => self.toggle_select(),
            Action::SnoozeDay => self.snooze_day(),
            Action::UnsnoozeDay => self.unsnooze_day(),
            Action::PostponeWeek => self.snooze_week(),
            Action::PreponeWeek => self.unsnooze_week(),
            Action::Create => self.create_new_item(),
            Action::CustomDelay => self.handle_custom_delay(),
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
        let pending_selected = sorted_indices(self.items.pending_selected_indices());
        let done_selected = sorted_indices(self.items.done_selected_indices());

        if !pending_selected.is_empty() || !done_selected.is_empty() {
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

    fn apply_to_selected_or_cursor<F>(&mut self, mut op: F)
    where
        F: FnMut(&mut Todo),
    {
        let pending_selected: Vec<usize> = self.items.pending_selected_indices().collect();
        let done_selected: Vec<usize> = self.items.done_selected_indices().collect();

        if !pending_selected.is_empty() || !done_selected.is_empty() {
            for i in pending_selected {
                if let Some(item) = self.items.get_mut(Section::Pending, i) {
                    op(item);
                }
            }
            for i in done_selected {
                if let Some(item) = self.items.get_mut(Section::Done, i) {
                    op(item);
                }
            }
        } else if let Some(item) = self.ui_state.get_cursored_item_mut(&mut self.items) {
            op(item);
        }
    }

    fn snooze(&mut self, duration: Duration) {
        let now = self.clock.now();
        self.apply_to_selected_or_cursor(|item| {
            let new_due = if let Some(current_due) = item.due_date {
                if current_due <= now {
                    now + duration
                } else {
                    current_due + duration
                }
            } else {
                now + duration
            };
            item.due_date = Some(new_due);
        });
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

    fn handle_custom_delay(&mut self) {
        self.mode = AppMode::Prompt(PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });
    }

    fn delay_from_now(&mut self, duration: Duration) {
        let target_due = self.clock.now() + duration;
        self.apply_to_selected_or_cursor(|item| {
            item.due_date = Some(target_due);
        });
    }
}
