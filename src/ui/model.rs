use chrono::{DateTime, Utc};

use super::todo::Todo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Pending,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptAction {
    CustomDelay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromptOverlay {
    pub(super) message: String,
    pub(super) buffer: String,
    pub(super) action: PromptAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AppMode {
    Normal,
    Prompt(PromptOverlay),
}

#[derive(Debug, Clone)]
pub(super) struct TodoItems {
    pub(super) pending: Vec<Todo>,
    pub(super) done: Vec<Todo>,
}

impl TodoItems {
    pub(super) fn new(mut items: Vec<Todo>) -> Self {
        items.sort_by_key(|todo| todo.due_date.unwrap_or(DateTime::<Utc>::MAX_UTC));

        let mut pending = Vec::new();
        let mut done = Vec::new();

        for item in items {
            if item.done {
                done.push(item);
            } else {
                pending.push(item);
            }
        }

        Self { pending, done }
    }

    pub(super) fn get(&self, section: Section, index: usize) -> Option<&Todo> {
        match section {
            Section::Pending => self.pending.get(index),
            Section::Done => self.done.get(index),
        }
    }

    pub(super) fn get_mut(&mut self, section: Section, index: usize) -> Option<&mut Todo> {
        match section {
            Section::Pending => self.pending.get_mut(index),
            Section::Done => self.done.get_mut(index),
        }
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn done_count(&self) -> usize {
        self.done.len()
    }

    pub(super) fn toggle_done(&mut self, section: Section, index: usize) {
        match section {
            Section::Pending => {
                if index < self.pending.len() {
                    let mut item = self.pending.remove(index);
                    item.done = true;
                    item.expanded = false;
                    item.selected = false;
                    self.done.push(item);
                }
            }
            Section::Done => {
                if index < self.done.len() {
                    let mut item = self.done.remove(index);
                    item.done = false;
                    item.selected = false;
                    self.pending.push(item);
                }
            }
        }
    }

    pub(super) fn to_vec(&self) -> Vec<Todo> {
        self.pending
            .iter()
            .chain(self.done.iter())
            .cloned()
            .collect()
    }

    pub(super) fn pending_iter(&self) -> impl Iterator<Item = (usize, &Todo)> {
        self.pending.iter().enumerate()
    }

    pub(super) fn done_iter(&self) -> impl Iterator<Item = (usize, &Todo)> {
        self.done.iter().enumerate()
    }

    pub(super) fn pending_selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.pending_iter()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
    }

    pub(super) fn done_selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.done_iter()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
    }

    pub(super) fn push(&mut self, item: Todo) {
        if item.done {
            self.done.push(item);
        } else {
            self.pending.push(item);
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct UiState {
    pub(super) current_section: Section,
    pub(super) pending_index: usize,
    pub(super) done_index: usize,
}

impl UiState {
    pub(super) fn new(pending_count: usize) -> Self {
        let current_section = if pending_count > 0 {
            Section::Pending
        } else {
            Section::Done
        };

        Self {
            current_section,
            pending_index: 0,
            done_index: 0,
        }
    }

    pub(super) fn select_next(&mut self, pending_count: usize, done_count: usize) {
        self.navigate(true, pending_count, done_count);
    }

    pub(super) fn select_previous(&mut self, pending_count: usize, done_count: usize) {
        self.navigate(false, pending_count, done_count);
    }

    fn navigate(&mut self, forward: bool, pending_count: usize, done_count: usize) {
        let (current_count, current_idx, other_count) = match self.current_section {
            Section::Pending => (pending_count, self.pending_index, done_count),
            Section::Done => (done_count, self.done_index, pending_count),
        };

        if current_count == 0 {
            return;
        }

        let at_boundary = if forward {
            current_idx + 1 >= current_count
        } else {
            current_idx == 0
        };

        if at_boundary && other_count > 0 {
            let other_idx = if forward { 0 } else { other_count - 1 };
            match self.current_section {
                Section::Pending => {
                    self.current_section = Section::Done;
                    self.done_index = other_idx;
                }
                Section::Done => {
                    self.current_section = Section::Pending;
                    self.pending_index = other_idx;
                }
            }
        } else {
            let new_idx = if at_boundary {
                if forward { 0 } else { current_count - 1 }
            } else if forward {
                current_idx + 1
            } else {
                current_idx - 1
            };
            match self.current_section {
                Section::Pending => self.pending_index = new_idx,
                Section::Done => self.done_index = new_idx,
            }
        }
    }

    pub(super) fn current_index(&self) -> usize {
        match self.current_section {
            Section::Pending => self.pending_index,
            Section::Done => self.done_index,
        }
    }

    pub(super) fn get_cursored_item_mut<'a>(
        &self,
        items: &'a mut TodoItems,
    ) -> Option<&'a mut Todo> {
        items.get_mut(self.current_section, self.current_index())
    }

    pub(super) fn adjust_indices(&mut self, pending_count: usize, done_count: usize) {
        if pending_count == 0 {
            self.pending_index = 0;
            if self.current_section == Section::Pending && done_count > 0 {
                self.current_section = Section::Done;
                self.done_index = 0;
            }
        } else if self.pending_index >= pending_count {
            self.pending_index = pending_count - 1;
        }

        if done_count == 0 {
            self.done_index = 0;
            if self.current_section == Section::Done && pending_count > 0 {
                self.current_section = Section::Pending;
                self.pending_index = 0;
            }
        } else if self.done_index >= done_count {
            self.done_index = done_count - 1;
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct AppModel {
    pub(super) exit: bool,
    pub(super) sync_on_exit: bool,
    pub(super) items: TodoItems,
    pub(super) ui_state: UiState,
    pub(super) mode: AppMode,
}

impl AppModel {
    pub(super) fn new(items: Vec<Todo>) -> Self {
        let items = TodoItems::new(items);
        let ui_state = UiState::new(items.pending_count());
        Self {
            exit: false,
            sync_on_exit: false,
            items,
            ui_state,
            mode: AppMode::Normal,
        }
    }
}
