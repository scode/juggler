use chrono::{DateTime, Duration, Utc};

use super::keymap::Action as NormalAction;
use super::model::{AppMode, AppModel, PromptAction, PromptOverlay, Section};
use super::todo::{Todo, parse_relative_duration};

#[derive(Debug, Clone)]
pub(super) enum Action {
    Normal(NormalAction),
    PromptSubmit,
    PromptCancel,
    PromptInput(char),
    PromptBackspace,
    ApplyEditedItem {
        section: Section,
        index: usize,
        updated_item: Todo,
    },
    ApplyCreatedItem {
        created_item: Todo,
    },
}

#[derive(Debug, Clone)]
pub(super) enum SideEffect {
    EditItem {
        section: Section,
        index: usize,
        original: Todo,
    },
    CreateItem {
        template: Todo,
    },
}

pub(super) fn update(
    model: &mut AppModel,
    action: Action,
    now: DateTime<Utc>,
) -> Option<SideEffect> {
    match action {
        Action::Normal(action) => update_normal_action(model, action, now),
        Action::PromptSubmit => {
            submit_prompt(model, now);
            None
        }
        Action::PromptCancel => {
            model.mode = AppMode::Normal;
            None
        }
        Action::PromptInput(c) => {
            if let AppMode::Prompt(overlay) = &mut model.mode {
                overlay.buffer.push(c);
            }
            None
        }
        Action::PromptBackspace => {
            if let AppMode::Prompt(overlay) = &mut model.mode {
                overlay.buffer.pop();
            }
            None
        }
        Action::ApplyEditedItem {
            section,
            index,
            updated_item,
        } => {
            apply_edited_item(model, section, index, updated_item);
            None
        }
        Action::ApplyCreatedItem { created_item } => {
            apply_created_item(model, created_item);
            None
        }
    }
}

fn update_normal_action(
    model: &mut AppModel,
    action: NormalAction,
    now: DateTime<Utc>,
) -> Option<SideEffect> {
    match action {
        NormalAction::Quit => {
            model.exit = true;
            None
        }
        NormalAction::QuitWithSync => {
            model.exit = true;
            model.sync_on_exit = true;
            None
        }
        NormalAction::NextItem => {
            model
                .ui_state
                .select_next(model.items.pending_count(), model.items.done_count());
            None
        }
        NormalAction::PreviousItem => {
            model
                .ui_state
                .select_previous(model.items.pending_count(), model.items.done_count());
            None
        }
        NormalAction::ToggleExpand => {
            if let Some(item) = model.ui_state.get_cursored_item_mut(&mut model.items) {
                item.expanded = !item.expanded;
            }
            None
        }
        NormalAction::ToggleDone => {
            toggle_done(model);
            None
        }
        NormalAction::Edit => request_edit(model),
        NormalAction::ToggleSelect => {
            if let Some(item) = model.ui_state.get_cursored_item_mut(&mut model.items) {
                item.selected = !item.selected;
            }
            None
        }
        NormalAction::SnoozeDay => {
            snooze(model, Duration::days(1), now);
            None
        }
        NormalAction::UnsnoozeDay => {
            snooze(model, Duration::days(-1), now);
            None
        }
        NormalAction::PostponeWeek => {
            snooze(model, Duration::days(7), now);
            None
        }
        NormalAction::PreponeWeek => {
            snooze(model, Duration::days(-7), now);
            None
        }
        NormalAction::Create => Some(SideEffect::CreateItem {
            template: empty_todo(),
        }),
        NormalAction::CustomDelay => {
            open_custom_delay_prompt(model);
            None
        }
    }
}

fn request_edit(model: &AppModel) -> Option<SideEffect> {
    let section = model.ui_state.current_section;
    let index = model.ui_state.current_index();
    model
        .items
        .get(section, index)
        .cloned()
        .map(|original| SideEffect::EditItem {
            section,
            index,
            original,
        })
}

fn empty_todo() -> Todo {
    Todo {
        title: String::new(),
        comment: None,
        expanded: false,
        done: false,
        selected: false,
        due_date: None,
        google_task_id: None,
    }
}

fn sorted_indices<I: Iterator<Item = usize>>(iter: I) -> Vec<usize> {
    let mut v: Vec<usize> = iter.collect();
    v.sort_unstable();
    v
}

fn toggle_done(model: &mut AppModel) {
    let pending_selected = sorted_indices(model.items.pending_selected_indices());
    let done_selected = sorted_indices(model.items.done_selected_indices());

    if !pending_selected.is_empty() || !done_selected.is_empty() {
        for i in pending_selected.into_iter().rev() {
            model.items.toggle_done(Section::Pending, i);
        }
        for i in done_selected.into_iter().rev() {
            model.items.toggle_done(Section::Done, i);
        }
    } else {
        let section = model.ui_state.current_section;
        let index = model.ui_state.current_index();
        model.items.toggle_done(section, index);
    }

    adjust_indices_after_toggle(model);
}

fn adjust_indices_after_toggle(model: &mut AppModel) {
    model
        .ui_state
        .adjust_indices(model.items.pending_count(), model.items.done_count());
}

fn apply_to_selected_or_cursor<F>(model: &mut AppModel, mut op: F)
where
    F: FnMut(&mut Todo),
{
    let pending_selected: Vec<usize> = model.items.pending_selected_indices().collect();
    let done_selected: Vec<usize> = model.items.done_selected_indices().collect();

    if !pending_selected.is_empty() || !done_selected.is_empty() {
        for i in pending_selected {
            if let Some(item) = model.items.get_mut(Section::Pending, i) {
                op(item);
            }
        }
        for i in done_selected {
            if let Some(item) = model.items.get_mut(Section::Done, i) {
                op(item);
            }
        }
    } else if let Some(item) = model.ui_state.get_cursored_item_mut(&mut model.items) {
        op(item);
    }
}

fn snooze(model: &mut AppModel, duration: Duration, now: DateTime<Utc>) {
    apply_to_selected_or_cursor(model, |item| {
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

fn delay_from_now(model: &mut AppModel, duration: Duration, now: DateTime<Utc>) {
    let target_due = now + duration;
    apply_to_selected_or_cursor(model, |item| {
        item.due_date = Some(target_due);
    });
}

fn open_custom_delay_prompt(model: &mut AppModel) {
    model.mode = AppMode::Prompt(PromptOverlay {
        message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
        buffer: String::new(),
        action: PromptAction::CustomDelay,
    });
}

fn submit_prompt(model: &mut AppModel, now: DateTime<Utc>) {
    if let AppMode::Prompt(overlay) = &model.mode {
        let action = overlay.action;
        let buffer = overlay.buffer.clone();
        model.mode = AppMode::Normal;
        match action {
            PromptAction::CustomDelay => {
                if let Some(duration) = parse_relative_duration(&buffer) {
                    delay_from_now(model, duration, now);
                }
            }
        }
    }
}

fn apply_edited_item(model: &mut AppModel, section: Section, index: usize, updated_item: Todo) {
    let Some(done_changed) = model
        .items
        .get(section, index)
        .map(|item| item.done != updated_item.done)
    else {
        return;
    };

    if done_changed {
        let _removed = match section {
            Section::Pending => model.items.pending.remove(index),
            Section::Done => model.items.done.remove(index),
        };
        model.items.push(updated_item);
        adjust_indices_after_toggle(model);
    } else if let Some(existing) = model.items.get_mut(section, index) {
        *existing = updated_item;
    }
}

fn apply_created_item(model: &mut AppModel, created_item: Todo) {
    if created_item.title.trim().is_empty() {
        return;
    }

    let is_done = created_item.done;
    model.items.push(created_item);

    if !is_done {
        model.ui_state.current_section = Section::Pending;
        model.ui_state.pending_index = model.items.pending_count().saturating_sub(1);
    } else {
        model.ui_state.current_section = Section::Done;
        model.ui_state.done_index = model.items.done_count().saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::super::keymap::Action as NormalAction;
    use super::*;

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

    fn done_todo(title: &str) -> Todo {
        let mut item = todo(title);
        item.done = true;
        item
    }

    fn selected_todo(title: &str) -> Todo {
        let mut item = todo(title);
        item.selected = true;
        item
    }

    #[test]
    fn quit_actions_set_exit_and_sync_flags() {
        let now = Utc::now();
        let mut model = AppModel::new(vec![todo("a")]);

        let effect = update(&mut model, Action::Normal(NormalAction::Quit), now);
        assert!(effect.is_none());
        assert!(model.exit);
        assert!(!model.sync_on_exit);

        let mut model = AppModel::new(vec![todo("a")]);
        let effect = update(&mut model, Action::Normal(NormalAction::QuitWithSync), now);
        assert!(effect.is_none());
        assert!(model.exit);
        assert!(model.sync_on_exit);
    }

    #[test]
    fn toggle_expand_flips_only_the_cursored_item() {
        let now = Utc::now();
        let mut model = AppModel::new(vec![todo("a"), todo("b")]);
        model.ui_state.pending_index = 1;

        update(&mut model, Action::Normal(NormalAction::ToggleExpand), now);
        assert!(!model.items.pending[0].expanded);
        assert!(model.items.pending[1].expanded);

        update(&mut model, Action::Normal(NormalAction::ToggleExpand), now);
        assert!(!model.items.pending[1].expanded);
    }

    #[test]
    fn toggle_done_uses_selected_items_and_clears_selection() {
        let now = Utc::now();
        let mut model = AppModel::new(vec![
            selected_todo("task 1"),
            todo("task 2"),
            selected_todo("task 3"),
        ]);
        model.ui_state.pending_index = 1;

        update(&mut model, Action::Normal(NormalAction::ToggleDone), now);

        assert_eq!(model.items.pending_count(), 1);
        assert_eq!(model.items.done_count(), 2);

        let task1 = model
            .items
            .done
            .iter()
            .find(|item| item.title == "task 1")
            .unwrap();
        let task3 = model
            .items
            .done
            .iter()
            .find(|item| item.title == "task 3")
            .unwrap();
        assert!(task1.done);
        assert!(task3.done);
        assert!(!task1.selected);
        assert!(!task3.selected);
    }

    #[test]
    fn toggle_done_uses_cursor_when_nothing_is_selected() {
        let now = Utc::now();
        let mut model = AppModel::new(vec![todo("task 1"), todo("task 2")]);
        model.ui_state.pending_index = 1;

        update(&mut model, Action::Normal(NormalAction::ToggleDone), now);

        assert_eq!(model.items.pending_count(), 1);
        assert_eq!(model.items.done_count(), 1);
        assert_eq!(model.items.pending[0].title, "task 1");
        assert_eq!(model.items.done[0].title, "task 2");
    }

    #[test]
    fn navigation_wraps_between_sections() {
        let now = Utc::now();
        let mut model = AppModel::new(vec![todo("pending"), done_todo("done")]);
        model.ui_state.current_section = Section::Pending;
        model.ui_state.pending_index = 0;

        update(&mut model, Action::Normal(NormalAction::NextItem), now);
        assert_eq!(model.ui_state.current_section, Section::Done);
        assert_eq!(model.ui_state.done_index, 0);

        update(&mut model, Action::Normal(NormalAction::PreviousItem), now);
        assert_eq!(model.ui_state.current_section, Section::Pending);
        assert_eq!(model.ui_state.pending_index, 0);
    }

    #[test]
    fn snooze_handles_past_future_and_empty_due_dates() {
        let base = Utc::now();
        let past = base - Duration::days(2);
        let future = base + Duration::days(3);
        let mut overdue = selected_todo("overdue");
        overdue.due_date = Some(past);
        let mut upcoming = selected_todo("upcoming");
        upcoming.due_date = Some(future);
        let mut none = selected_todo("none");
        none.due_date = None;

        let mut model = AppModel::new(vec![overdue, upcoming, none, todo("untouched")]);
        update(&mut model, Action::Normal(NormalAction::SnoozeDay), base);

        let overdue = model
            .items
            .pending
            .iter()
            .find(|t| t.title == "overdue")
            .unwrap();
        let upcoming = model
            .items
            .pending
            .iter()
            .find(|t| t.title == "upcoming")
            .unwrap();
        let none = model
            .items
            .pending
            .iter()
            .find(|t| t.title == "none")
            .unwrap();
        let untouched = model
            .items
            .pending
            .iter()
            .find(|t| t.title == "untouched")
            .unwrap();

        assert_eq!(overdue.due_date, Some(base + Duration::days(1)));
        assert_eq!(upcoming.due_date, Some(future + Duration::days(1)));
        assert_eq!(none.due_date, Some(base + Duration::days(1)));
        assert_eq!(untouched.due_date, None);
    }

    #[test]
    fn due_date_operations_keep_selection_state() {
        let base = Utc::now();
        let mut model = AppModel::new(vec![selected_todo("a"), selected_todo("b"), todo("c")]);

        update(&mut model, Action::Normal(NormalAction::SnoozeDay), base);
        update(&mut model, Action::Normal(NormalAction::UnsnoozeDay), base);
        update(&mut model, Action::Normal(NormalAction::PostponeWeek), base);
        update(&mut model, Action::Normal(NormalAction::PreponeWeek), base);

        assert!(model.items.pending[0].selected);
        assert!(model.items.pending[1].selected);
        assert!(!model.items.pending[2].selected);
    }

    #[test]
    fn custom_delay_prompt_submit_and_cancel_work() {
        let base = Utc::now();
        let mut model = AppModel::new(vec![todo("a")]);

        update(&mut model, Action::Normal(NormalAction::CustomDelay), base);
        assert!(matches!(model.mode, AppMode::Prompt(_)));

        update(&mut model, Action::PromptInput('1'), base);
        update(&mut model, Action::PromptInput('d'), base);
        update(&mut model, Action::PromptSubmit, base);

        assert!(matches!(model.mode, AppMode::Normal));
        assert_eq!(
            model.items.pending[0].due_date,
            Some(base + Duration::days(1))
        );

        update(&mut model, Action::Normal(NormalAction::CustomDelay), base);
        update(&mut model, Action::PromptInput('x'), base);
        update(&mut model, Action::PromptCancel, base);
        assert!(matches!(model.mode, AppMode::Normal));
    }

    #[test]
    fn prompt_backspace_updates_buffer() {
        let base = Utc::now();
        let mut model = AppModel::new(vec![todo("a")]);
        update(&mut model, Action::Normal(NormalAction::CustomDelay), base);

        update(&mut model, Action::PromptInput('1'), base);
        update(&mut model, Action::PromptInput('0'), base);
        update(&mut model, Action::PromptBackspace, base);

        let AppMode::Prompt(prompt) = &model.mode else {
            panic!("prompt mode expected");
        };
        assert_eq!(prompt.buffer, "1");
    }

    #[test]
    fn create_action_returns_side_effect_and_apply_created_item_updates_cursor() {
        let base = Utc::now();
        let mut model = AppModel::new(vec![todo("existing")]);

        let side_effect = update(&mut model, Action::Normal(NormalAction::Create), base);
        assert!(matches!(side_effect, Some(SideEffect::CreateItem { .. })));

        update(
            &mut model,
            Action::ApplyCreatedItem {
                created_item: todo("new"),
            },
            base,
        );

        assert_eq!(model.items.pending_count(), 2);
        assert_eq!(model.ui_state.current_section, Section::Pending);
        assert_eq!(model.ui_state.pending_index, 1);
    }

    #[test]
    fn apply_created_item_rejects_empty_title() {
        let base = Utc::now();
        let mut model = AppModel::new(vec![todo("existing")]);
        update(
            &mut model,
            Action::ApplyCreatedItem {
                created_item: todo(" "),
            },
            base,
        );
        assert_eq!(model.items.pending_count(), 1);
    }

    #[test]
    fn edit_action_round_trip_updates_item_or_moves_when_done_changes() {
        let base = Utc::now();
        let mut model = AppModel::new(vec![todo("a")]);
        let effect = update(&mut model, Action::Normal(NormalAction::Edit), base);
        let Some(SideEffect::EditItem {
            section,
            index,
            original,
        }) = effect
        else {
            panic!("expected edit side effect");
        };
        assert_eq!(section, Section::Pending);
        assert_eq!(index, 0);
        assert_eq!(original.title, "a");

        let mut edited = original.clone();
        edited.title = "a edited".to_string();
        update(
            &mut model,
            Action::ApplyEditedItem {
                section,
                index,
                updated_item: edited,
            },
            base,
        );
        assert_eq!(model.items.pending[0].title, "a edited");

        let mut done = model.items.pending[0].clone();
        done.done = true;
        update(
            &mut model,
            Action::ApplyEditedItem {
                section: Section::Pending,
                index: 0,
                updated_item: done,
            },
            base,
        );
        assert_eq!(model.items.pending_count(), 0);
        assert_eq!(model.items.done_count(), 1);
    }
}
