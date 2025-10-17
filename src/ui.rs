use std::{env, fs, io::Write, process::Command};

use chrono::{DateTime, Duration, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, List, ListState, Paragraph},
};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use tempfile::NamedTempFile;

use crate::config::DEFAULT_EDITOR;
use crate::error::{JugglerError, Result};
use crate::store::TodoItem;
#[cfg(test)]
use crate::time::fixed_clock;
use crate::time::{SharedClock, system_clock};

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

#[derive(Debug, Clone)]
pub struct Todo {
    pub title: String,
    pub comment: Option<String>,
    pub expanded: bool,
    pub done: bool,
    pub selected: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub google_task_id: Option<String>,
}

impl Todo {
    pub fn format_relative_time(&self, now: DateTime<Utc>) -> Option<String> {
        self.due_date.map(|due| {
            let duration = due.signed_duration_since(now);
            let time_str = format_duration_compact(duration);
            // Right-pad to 4 characters for alignment
            format!("{time_str:>4}")
        })
    }

    pub fn due_date_urgency(&self, now: DateTime<Utc>) -> Option<DueDateUrgency> {
        self.due_date.map(|due| {
            let duration = due.signed_duration_since(now);
            let total_seconds = duration.num_seconds();

            if total_seconds < 0 {
                DueDateUrgency::Overdue
            } else if total_seconds <= 172800 {
                // 48 hours
                DueDateUrgency::DueSoon
            } else {
                DueDateUrgency::Normal
            }
        })
    }

    pub fn expanded_text(&self, now: DateTime<Utc>) -> Text<'_> {
        let mut first_line_spans = Vec::new();

        // Add relative time if due date exists
        if let Some(relative_time) = self.format_relative_time(now) {
            let color = match self.due_date_urgency(now) {
                Some(DueDateUrgency::Overdue) => Color::Red,
                Some(DueDateUrgency::DueSoon) => Color::Yellow,
                _ => Color::White,
            };
            first_line_spans.push(Span::styled(
                format!("{relative_time} "),
                Style::default().fg(color),
            ));
        }

        first_line_spans.push(Span::raw(&self.title));
        let has_comment = self.has_comment();
        if has_comment {
            first_line_spans.push(Span::raw(" >>>"));
        }

        let mut lines = vec![ratatui::text::Line::from(first_line_spans)];
        if self.expanded
            && has_comment
            && let Some(comment) = &self.comment
        {
            for line in comment.lines() {
                lines.push(ratatui::text::Line::from(vec![
                    Span::raw("           "),
                    Span::raw(line),
                ]));
            }
        }

        Text::from(lines)
    }

    pub fn has_comment(&self) -> bool {
        self.comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
    }

    #[cfg(test)]
    pub fn collapsed_summary(&self, now: DateTime<Utc>) -> Vec<Span<'_>> {
        let mut spans = Vec::new();

        // Add relative time if due date exists
        if let Some(relative_time) = self.format_relative_time(now) {
            let color = match self.due_date_urgency(now) {
                Some(DueDateUrgency::Overdue) => Color::Red,
                Some(DueDateUrgency::DueSoon) => Color::Yellow,
                _ => Color::White,
            };
            spans.push(Span::styled(
                format!("{relative_time} "),
                Style::default().fg(color),
            ));
        }

        spans.push(Span::raw(&self.title));
        if self.has_comment() {
            spans.push(Span::raw(" (...)"));
        }
        spans
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DueDateUrgency {
    Overdue,
    DueSoon,
    Normal,
}

#[derive(Debug, Clone)]
struct TodoItems {
    pending: Vec<Todo>,
    done: Vec<Todo>,
}

impl TodoItems {
    /// Create a new TodoItems collection from a flat list of todos.
    /// Items are sorted by due date before being split into pending/done.
    fn new(mut items: Vec<Todo>) -> Self {
        // Sort by due date (items without due dates go to the end)
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

    /// Get a reference to an item by section and index
    fn get(&self, section: Section, index: usize) -> Option<&Todo> {
        match section {
            Section::Pending => self.pending.get(index),
            Section::Done => self.done.get(index),
        }
    }

    /// Get a mutable reference to an item by section and index
    fn get_mut(&mut self, section: Section, index: usize) -> Option<&mut Todo> {
        match section {
            Section::Pending => self.pending.get_mut(index),
            Section::Done => self.done.get_mut(index),
        }
    }

    fn pending_count(&self) -> usize {
        self.pending.len()
    }

    fn done_count(&self) -> usize {
        self.done.len()
    }

    /// Move an item from pending to done or vice versa
    fn toggle_done(&mut self, section: Section, index: usize) {
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

    /// Convert back to a flat Vec containing pending items followed
    /// by done items.
    fn to_vec(&self) -> Vec<Todo> {
        self.pending
            .iter()
            .chain(self.done.iter())
            .cloned()
            .collect()
    }

    /// Iterator over pending items with their section indices
    fn pending_iter(&self) -> impl Iterator<Item = (usize, &Todo)> {
        self.pending.iter().enumerate()
    }

    /// Iterator over done items with their section indices
    fn done_iter(&self) -> impl Iterator<Item = (usize, &Todo)> {
        self.done.iter().enumerate()
    }

    /// Iterator over indices of selected pending items
    fn pending_selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.pending_iter()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
    }

    /// Iterator over indices of selected done items
    fn done_selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.done_iter()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
    }

    /// Add a new item to the appropriate section
    fn push(&mut self, item: Todo) {
        if item.done {
            self.done.push(item);
        } else {
            self.pending.push(item);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PromptAction {
    CustomDelay,
}

#[derive(Debug, Clone)]
struct PromptOverlay {
    message: String,
    buffer: String,
    action: PromptAction,
}

#[derive(Debug, Clone)]
struct PromptWidget {
    text: String,
}

impl PromptWidget {
    fn new(message: &str, buffer: &str) -> Self {
        Self {
            text: format!("{}{}", message, buffer),
        }
    }
}

impl Widget for PromptWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the entire area to ensure a blank background
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                let cell = &mut buf[(x, y)];
                cell.reset();
                cell.set_symbol(" ");
            }
        }

        // Render the prompt text on the first line of the area, truncated if necessary
        let max_width = area.width as usize;
        let content = if self.text.len() > max_width {
            self.text.chars().take(max_width).collect::<String>()
        } else {
            self.text
        };

        // Write characters into the buffer
        let mut x = area.x;
        let y = area.y;
        for ch in content.chars() {
            let cell = &mut buf[(x, y)];
            cell.set_symbol(ch.encode_utf8(&mut [0; 4]));
            cell.set_style(Style::default());
            x += 1;
        }
    }
}

#[derive(Debug)]
pub struct App<T: TodoEditor> {
    exit: bool,
    sync_on_exit: bool,
    items: TodoItems,
    ui_state: UiState,
    editor: T,
    clock: SharedClock,
    prompt_overlay: Option<PromptOverlay>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    Pending,
    Done,
}

#[derive(Debug, Clone)]
struct UiState {
    current_section: Section,
    pending_index: usize,
    done_index: usize,
}

impl UiState {
    fn new(pending_count: usize) -> Self {
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

    fn select_next(&mut self, pending_count: usize, done_count: usize) {
        match self.current_section {
            Section::Pending => {
                if pending_count > 0 {
                    self.pending_index += 1;
                    if self.pending_index >= pending_count {
                        // Move to done section if available
                        if done_count > 0 {
                            self.current_section = Section::Done;
                            self.done_index = 0;
                        } else {
                            // Wrap around to beginning of pending
                            self.pending_index = 0;
                        }
                    }
                }
            }
            Section::Done => {
                if done_count > 0 {
                    self.done_index += 1;
                    if self.done_index >= done_count {
                        // Move to pending section if available
                        if pending_count > 0 {
                            self.current_section = Section::Pending;
                            self.pending_index = 0;
                        } else {
                            // Wrap around to beginning of done
                            self.done_index = 0;
                        }
                    }
                }
            }
        }
    }

    fn select_previous(&mut self, pending_count: usize, done_count: usize) {
        match self.current_section {
            Section::Pending => {
                if pending_count > 0 {
                    if self.pending_index == 0 {
                        // Move to end of done section if available
                        if done_count > 0 {
                            self.current_section = Section::Done;
                            self.done_index = done_count - 1;
                        } else {
                            // Wrap around to end of pending
                            self.pending_index = pending_count - 1;
                        }
                    } else {
                        self.pending_index -= 1;
                    }
                }
            }
            Section::Done => {
                if done_count > 0 {
                    if self.done_index == 0 {
                        // Move to end of pending section if available
                        if pending_count > 0 {
                            self.current_section = Section::Pending;
                            self.pending_index = pending_count - 1;
                        } else {
                            // Wrap around to end of done
                            self.done_index = done_count - 1;
                        }
                    } else {
                        self.done_index -= 1;
                    }
                }
            }
        }
    }

    /// Get the current section index (either pending_index or done_index)
    fn current_index(&self) -> usize {
        match self.current_section {
            Section::Pending => self.pending_index,
            Section::Done => self.done_index,
        }
    }

    /// Get a mutable reference to the currently cursored item
    fn get_cursored_item_mut<'a>(&self, items: &'a mut TodoItems) -> Option<&'a mut Todo> {
        items.get_mut(self.current_section, self.current_index())
    }

    fn adjust_indices(&mut self, pending_count: usize, done_count: usize) {
        // Clamp indices to valid ranges
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

impl<T: TodoEditor> App<T> {
    pub fn new(items: Vec<Todo>, editor: T) -> Self {
        Self::new_with_clock(items, editor, system_clock())
    }

    pub fn items(&self) -> Vec<Todo> {
        self.items.to_vec()
    }

    pub fn should_sync_on_exit(&self) -> bool {
        self.sync_on_exit
    }

    pub fn new_with_clock(items: Vec<Todo>, editor: T, clock: SharedClock) -> Self {
        let items = TodoItems::new(items);
        let ui_state = UiState::new(items.pending_count());

        App {
            exit: false,
            sync_on_exit: false,
            items,
            ui_state,
            editor,
            clock,
            prompt_overlay: None,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw_internal(frame))?;
            self.handle_events(terminal)?;
        }
        Ok(())
    }

    fn render_pending_section(&self) -> List<'_> {
        let pending_items: Vec<_> = self
            .items
            .pending_iter()
            .map(|(idx, _)| {
                ratatui::widgets::ListItem::new(self.display_text_internal(Section::Pending, idx))
            })
            .collect();

        List::new(pending_items).block(Block::default().title("Pending").borders(Borders::ALL))
    }

    fn render_done_section(&self) -> List<'_> {
        let done_items: Vec<_> = self
            .items
            .done_iter()
            .map(|(idx, _)| {
                let mut text = self.display_text_internal(Section::Done, idx);
                // Apply crossed-out style to all spans
                for line in &mut text.lines {
                    for span in &mut line.spans {
                        span.style = span.style.add_modifier(Modifier::CROSSED_OUT);
                    }
                }
                ratatui::widgets::ListItem::new(text)
            })
            .collect();

        List::new(done_items).block(Block::default().title("Done").borders(Borders::ALL))
    }

    fn render_help_or_prompt(&self, area: Rect, frame: &mut Frame) {
        match &self.prompt_overlay {
            Some(prompt) => {
                frame.render_widget(PromptWidget::new(&prompt.message, &prompt.buffer), area);
            }
            None => {
                let help_widget =
                    Paragraph::new(HELP_TEXT).block(Block::default().borders(Borders::TOP));
                frame.render_widget(help_widget, area);
            }
        }
    }

    fn draw_internal(&mut self, frame: &mut Frame) {
        use ratatui::layout::{Constraint, Direction, Layout};

        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let main_area = chunks[0];
        let help_area = chunks[1];

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(main_area);

        let pending_widget = self.render_pending_section();
        let done_widget = self.render_done_section();

        match self.ui_state.current_section {
            Section::Pending => {
                let mut pending_state = ListState::default();
                pending_state.select(Some(self.ui_state.pending_index));
                frame.render_stateful_widget(pending_widget, sections[0], &mut pending_state);
                frame.render_widget(done_widget, sections[1]);
            }
            Section::Done => {
                frame.render_widget(pending_widget, sections[0]);
                let mut done_state = ListState::default();
                done_state.select(Some(self.ui_state.done_index));
                frame.render_stateful_widget(done_widget, sections[1], &mut done_state);
            }
        }

        self.render_help_or_prompt(help_area, frame);
    }

    fn display_text_internal(&self, section: Section, index: usize) -> Text<'_> {
        let todo = self.items.get(section, index).expect("valid index");
        let is_cursored =
            section == self.ui_state.current_section && index == self.ui_state.current_index();

        let cursor_prefix = if is_cursored { "▶ " } else { "  " };
        // Single status box: selection takes precedence over done
        let status_box = if todo.selected {
            "[x] "
        } else if todo.done {
            "[✓] "
        } else {
            "[ ] "
        };

        let mut first_line_spans = Vec::new();
        first_line_spans.push(Span::raw(cursor_prefix));
        first_line_spans.push(Span::raw(status_box));

        let now = self.clock.now();
        if let Some(relative_time) = todo.format_relative_time(now) {
            let color = match todo.due_date_urgency(now) {
                Some(DueDateUrgency::Overdue) => Color::Red,
                Some(DueDateUrgency::DueSoon) => Color::Yellow,
                _ => Color::White,
            };
            first_line_spans.push(Span::styled(
                format!("{relative_time} "),
                Style::default().fg(color),
            ));
        }

        if is_cursored {
            first_line_spans.push(Span::styled(
                &todo.title,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        } else {
            first_line_spans.push(Span::raw(&todo.title));
        }

        let has_comment = todo.has_comment();
        if has_comment {
            if todo.expanded {
                first_line_spans.push(Span::raw(" >>>"));
            } else {
                first_line_spans.push(Span::raw(" (...)"));
            }
        }

        let mut lines = vec![ratatui::text::Line::from(first_line_spans)];

        // For expanded items, append additional lines using expanded_text()
        if todo.expanded {
            let expanded_text = todo.expanded_text(now);
            for (i, line) in expanded_text.lines.iter().enumerate() {
                if i == 0 {
                    continue; // skip first line, we already built it with cursor/checkbox
                }
                lines.push(line.clone());
            }
        }

        Text::from(lines)
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                if self.prompt_overlay.is_some() {
                    // Modal prompt handling when overlay is active
                    self.handle_prompt_mode_key(key_event);
                } else {
                    self.handle_normal_mode_key(key_event, terminal)?;
                }
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_prompt_mode_key(&mut self, key_event: KeyEvent) {
        use crossterm::event::KeyModifiers;
        if let Some(overlay) = &mut self.prompt_overlay {
            match key_event.code {
                KeyCode::Enter => {
                    let finished = overlay.buffer.clone();
                    let action = overlay.action;
                    self.prompt_overlay = None;
                    match action {
                        PromptAction::CustomDelay => {
                            if let Some(duration) = parse_relative_duration(&finished) {
                                self.delay_from_now(duration);
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    self.prompt_overlay = None;
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

    fn handle_normal_mode_key(
        &mut self,
        key_event: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        if (key_event.code == KEY_EDIT || key_event.code == KEY_CREATE)
            && self.editor.needs_terminal_restoration()
        {
            // Special handling for external editor - restore and reinitialize terminal
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

    fn handle_key_event_internal(&mut self, key_event: KeyEvent) {
        //dbg!(key_event);
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

    fn select_next_internal(&mut self) {
        self.ui_state
            .select_next(self.items.pending_count(), self.items.done_count());
    }

    fn select_previous_internal(&mut self) {
        self.ui_state
            .select_previous(self.items.pending_count(), self.items.done_count());
    }

    fn toggle_done(&mut self) {
        // Collect selected items from both sections
        let mut pending_selected: Vec<usize> = self.items.pending_selected_indices().collect();
        let mut done_selected: Vec<usize> = self.items.done_selected_indices().collect();

        if !pending_selected.is_empty() || !done_selected.is_empty() {
            // Toggle selected items, starting from highest index to avoid invalidation
            pending_selected.sort_unstable();
            done_selected.sort_unstable();

            for i in pending_selected.into_iter().rev() {
                self.items.toggle_done(Section::Pending, i);
            }
            for i in done_selected.into_iter().rev() {
                self.items.toggle_done(Section::Done, i);
            }
        } else {
            // No items selected, toggle the cursored item
            let section = self.ui_state.current_section;
            let index = self.ui_state.current_index();
            self.items.toggle_done(section, index);
        }

        // Adjust indices after toggling done status
        self.adjust_indices_after_toggle();
    }

    fn toggle_select(&mut self) {
        if let Some(item) = self.ui_state.get_cursored_item_mut(&mut self.items) {
            item.selected = !item.selected;
        }
    }

    fn snooze(&mut self, duration: Duration) {
        let now = self.clock.now();

        // Helper to calculate new due date
        let calculate_new_due = |current_due: Option<DateTime<Utc>>| -> DateTime<Utc> {
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

        // Collect selected items from both sections
        let pending_selected: Vec<usize> = self.items.pending_selected_indices().collect();
        let done_selected: Vec<usize> = self.items.done_selected_indices().collect();

        if !pending_selected.is_empty() || !done_selected.is_empty() {
            // Snooze selected items (keep selection for repeated operations)
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
            // No items selected, snooze the cursored item
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

    fn edit_item(&mut self) {
        let section = self.ui_state.current_section;
        let index = self.ui_state.current_index();

        if let Some(item) = self.items.get(section, index) {
            let result = self.editor.edit_todo(item);

            match result {
                Ok(updated_item) => {
                    // Check if done status changed
                    let done_changed = item.done != updated_item.done;

                    if done_changed {
                        // Remove old item and add updated one to correct section
                        // This is simpler than trying to move between sections
                        let _ = match section {
                            Section::Pending => self.items.pending.remove(index),
                            Section::Done => self.items.done.remove(index),
                        };
                        self.items.push(updated_item);
                        self.adjust_indices_after_toggle();
                    } else {
                        // Just update in place
                        if let Some(existing) = self.items.get_mut(section, index) {
                            *existing = updated_item;
                        }
                    }
                }
                Err(_) => {
                    // Editor failed or was cancelled - do nothing
                }
            }
        }
    }

    fn create_new_item(&mut self) {
        // Create a new Todo with default values
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

        match result {
            Ok(created_item) => {
                // Only add the item if it has a non-empty title
                if !created_item.title.trim().is_empty() {
                    let is_done = created_item.done;
                    self.items.push(created_item);

                    // Move cursor to the newly created item (at end of appropriate section)
                    if !is_done {
                        self.ui_state.current_section = Section::Pending;
                        self.ui_state.pending_index = self.items.pending_count().saturating_sub(1);
                    } else {
                        self.ui_state.current_section = Section::Done;
                        self.ui_state.done_index = self.items.done_count().saturating_sub(1);
                    }
                }
            }
            Err(_) => {
                // Editor failed or was cancelled - do nothing
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
        let _ = terminal; // unused
        // Activate overlay; main loop will handle input and completion
        self.prompt_overlay = Some(PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: PromptAction::CustomDelay,
        });
    }

    fn delay_from_now(&mut self, duration: Duration) {
        let now = self.clock.now();
        let target_due = now + duration;

        // Collect selected items from both sections
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

fn parse_relative_duration(input: &str) -> Option<Duration> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    // Extract optional sign
    let (sign, rest) = match s.chars().next()? {
        '+' => (1i64, &s[1..]),
        '-' => (-1i64, &s[1..]),
        _ => (1i64, s),
    };

    // Split numeric part and unit
    let mut digits_end = 0usize;
    for ch in rest.chars() {
        if ch.is_ascii_digit() {
            digits_end += 1;
        } else {
            break;
        }
    }
    if digits_end == 0 || digits_end >= rest.len() {
        return None;
    }
    let number_str = &rest[..digits_end];
    let unit_str = rest[digits_end..].trim();

    let magnitude: i64 = number_str.parse().ok()?;
    let signed = magnitude.saturating_mul(sign);

    match unit_str {
        "s" => Some(Duration::seconds(signed)),
        "m" => Some(Duration::minutes(signed)),
        "h" => Some(Duration::hours(signed)),
        "d" => Some(Duration::days(signed)),
        _ => None,
    }
}

fn format_duration_compact(duration: Duration) -> String {
    let total_seconds = duration.num_seconds();
    let abs_seconds = total_seconds.abs();

    let (value, unit) = if abs_seconds < 60 {
        (abs_seconds, "s")
    } else if abs_seconds < 3600 {
        (abs_seconds / 60, "m")
    } else if abs_seconds < 86400 {
        (abs_seconds / 3600, "h")
    } else {
        (abs_seconds / 86400, "d")
    };

    if total_seconds < 0 {
        format!("-{value}{unit}")
    } else {
        format!("{value}{unit}")
    }
}

impl From<TodoItem> for Todo {
    fn from(item: TodoItem) -> Self {
        Todo {
            title: item.title,
            comment: item.comment,
            expanded: false,
            done: item.done,
            selected: false,
            due_date: item.due_date,
            google_task_id: item.google_task_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        text::{Span, Text},
    };

    // Helper function to convert spans to plain text for testing
    fn spans_to_string(spans: &[Span]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    // Helper function to convert Text to plain text for testing
    fn text_to_string(text: &Text) -> String {
        text.lines
            .iter()
            .map(|line| spans_to_string(&line.spans))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // Helper to get all items as a flat Vec for testing
    fn get_all_items<T: TodoEditor>(app: &App<T>) -> Vec<Todo> {
        app.items.to_vec()
    }

    // Test-only editor that doesn't do anything
    struct NoOpEditor;

    impl TodoEditor for NoOpEditor {
        fn edit_todo(&self, todo: &Todo) -> Result<Todo> {
            // Return the todo unchanged
            Ok(todo.clone())
        }

        fn needs_terminal_restoration(&self) -> bool {
            false
        }
    }

    // Test-only editor that returns a specific todo item
    struct MockEditor {
        return_todo: Todo,
    }

    impl MockEditor {
        fn new(return_todo: Todo) -> Self {
            MockEditor { return_todo }
        }
    }

    impl TodoEditor for MockEditor {
        fn edit_todo(&self, _todo: &Todo) -> Result<Todo> {
            Ok(self.return_todo.clone())
        }

        fn needs_terminal_restoration(&self) -> bool {
            false
        }
    }

    #[test]
    fn toggle_cursored_expanded_via_key_event() {
        let items = vec![Todo {
            title: String::from("a"),
            comment: Some(String::from("comment")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let mut app = App::new(items, NoOpEditor);
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].expanded);
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(!get_all_items(&app)[0].expanded);
    }

    #[test]
    fn quit_with_sync_key_sets_sync_flag() {
        let items = vec![Todo {
            title: String::from("test item"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let mut app = App::new(items, NoOpEditor);

        // Initially neither exit nor sync should be set
        assert!(!app.exit);
        assert!(!app.should_sync_on_exit());

        // Press 'Q' to quit with sync
        app.handle_key_event_internal(KeyEvent::new(KEY_QUIT_WITH_SYNC, KeyModifiers::NONE));

        // Both exit and sync should be set
        assert!(app.exit);
        assert!(app.should_sync_on_exit());
    }

    #[test]
    fn regular_quit_key_does_not_set_sync_flag() {
        let items = vec![Todo {
            title: String::from("test item"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        }];
        let mut app = App::new(items, NoOpEditor);

        // Initially neither exit nor sync should be set
        assert!(!app.exit);
        assert!(!app.should_sync_on_exit());

        // Press 'q' to quit normally
        app.handle_key_event_internal(KeyEvent::new(KEY_QUIT, KeyModifiers::NONE));

        // Only exit should be set, not sync
        assert!(app.exit);
        assert!(!app.should_sync_on_exit());
    }

    #[test]
    fn collapsed_summary_marks_expandable_items() {
        let with_comment = Todo {
            title: String::from("a"),
            comment: Some(String::from("comment")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            spans_to_string(&with_comment.collapsed_summary(Utc::now())),
            "a (...)"
        );

        let without_comment = Todo {
            title: String::from("b"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            spans_to_string(&without_comment.collapsed_summary(Utc::now())),
            "b"
        );
    }

    #[test]
    fn expanded_text_indents_comment() {
        let todo = Todo {
            title: String::from("a"),
            comment: Some(String::from("line1\nline2")),
            expanded: true,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            text_to_string(&todo.expanded_text(Utc::now())),
            "a >>>\n           line1\n           line2"
        );
    }

    #[test]
    fn display_text_prefixes_cursor() {
        let items = vec![
            Todo {
                title: String::from("a"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("b"),
                comment: Some(String::from("c1\nc2")),
                expanded: true,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
        ];
        let base = Utc::now();
        let app = App::new_with_clock(items, NoOpEditor, fixed_clock(base));

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
        let items = vec![Todo {
            title: String::from("future task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(base + chrono::Duration::hours(50)),
            google_task_id: None,
        }];
        let app = App::new_with_clock(items, NoOpEditor, fixed_clock(base));

        // 50h in the future should render as right-aligned "  2d"
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ]   2d future task"
        );
    }

    #[test]
    fn expanded_display_includes_relative_time_and_comment_lines() {
        let base = Utc::now();
        let items = vec![
            Todo {
                title: String::from("a"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("b"),
                comment: Some(String::from("c1\nc2")),
                expanded: true,
                done: false,
                selected: false,
                due_date: Some(base + chrono::Duration::hours(50)),
                google_task_id: None,
            },
        ];

        let app = App::new_with_clock(items, NoOpEditor, fixed_clock(base));

        // After sorting by due date, "b" (with due date) comes before "a" (no due date)
        // Index 0 has the cursor by default, so expect cursor prefix
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ]   2d b >>>\n           c1\n           c2"
        );
    }

    #[test]
    fn selection_indicator_shows_x_for_selected_items() {
        let items = vec![
            Todo {
                title: String::from("first"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("second"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
        ];
        let mut app = App::new(items, NoOpEditor);

        // Initially, not selected
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [ ] first"
        );

        // Toggle selection on current item
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_SELECT, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert_eq!(
            text_to_string(&app.display_text_internal(Section::Pending, 0)),
            "▶ [x] first"
        );

        // Move cursor and select second item as well
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
        // Test collapsed item with comment (shows 📋)
        let collapsed_with_comment = Todo {
            title: String::from("Task with details"),
            comment: Some(String::from("Some details")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            spans_to_string(&collapsed_with_comment.collapsed_summary(Utc::now())),
            "Task with details (...)"
        );

        // Test expanded item with comment (shows 📖)
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

        // Test item without comment (no icon)
        let no_comment = Todo {
            title: String::from("Simple task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            spans_to_string(&no_comment.collapsed_summary(Utc::now())),
            "Simple task"
        );

        // Test item with empty comment (no icon)
        let empty_comment = Todo {
            title: String::from("Task with empty comment"),
            comment: Some(String::from("   ")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(
            spans_to_string(&empty_comment.collapsed_summary(Utc::now())),
            "Task with empty comment"
        );
    }

    #[test]
    fn draw_displays_help_text() {
        let width = (HELP_TEXT.len() as u16).saturating_add(2);
        let backend = TestBackend::new(width, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(Vec::new(), NoOpEditor);

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
            Todo {
                title: String::from("pending task"),
                comment: None,
                expanded: false,
                done: false,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("done task"),
                comment: None,
                expanded: false,
                done: true,
                selected: false,
                due_date: None,
                google_task_id: None,
            },
        ];
        let mut app = App::new(items, NoOpEditor);

        // Toggle first item from pending to done
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].done);
        assert_eq!(app.items.pending_count(), 0);

        // Toggle back to pending
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(!get_all_items(&app)[0].done);
        assert_eq!(app.items.pending_count(), 1);
    }

    #[test]
    fn toggle_done_works_on_selected_items() {
        let items = vec![
            Todo {
                title: String::from("task 1"),
                comment: None,
                expanded: false,
                done: false,
                selected: true, // Selected
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("task 2"),
                comment: None,
                expanded: false,
                done: false,
                selected: false, // Not selected (cursor is here)
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("task 3"),
                comment: None,
                expanded: false,
                done: false,
                selected: true, // Selected
                due_date: None,
                google_task_id: None,
            },
        ];
        let mut app = App::new(items, NoOpEditor);
        // Manually set cursor to second item
        app.select_next_internal(); // Move from 0 to 1

        // Toggle done - should affect only selected items (0 and 2), not cursor item (1)
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));

        // After toggling, check by title since items have moved between sections
        let final_items = get_all_items(&app);
        let task1 = final_items.iter().find(|t| t.title == "task 1").unwrap();
        let task2 = final_items.iter().find(|t| t.title == "task 2").unwrap();
        let task3 = final_items.iter().find(|t| t.title == "task 3").unwrap();

        assert!(task1.done); // Selected item should be marked done
        assert!(!task2.done); // Cursor item should remain unchanged
        assert!(task3.done); // Selected item should be marked done
        assert_eq!(app.items.pending_count(), 1); // Only one pending item left

        // Items should be deselected after toggling
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
                selected: false, // Not selected
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("task 2"),
                comment: None,
                expanded: false,
                done: false,
                selected: false, // Not selected (cursor is here)
                due_date: None,
                google_task_id: None,
            },
        ];
        let mut app = App::new(items, NoOpEditor);
        // Manually set cursor to second item
        app.select_next_internal(); // Move from 0 to 1

        // Toggle done - should affect cursor item since no items are selected
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));

        assert!(!get_all_items(&app)[0].done); // First item should remain unchanged
        assert!(get_all_items(&app)[1].done); // Cursor item should be marked done
        assert_eq!(app.items.pending_count(), 1); // One pending item left
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
        let mut app = App::new_with_clock(items, NoOpEditor, fixed_clock(base));

        // snooze 1d when no due date -> base + 1d (exact)
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let due1 = get_all_items(&app)[0]
            .due_date
            .expect("due set after snooze day");
        assert_eq!(due1, base + Duration::days(1));

        // postpone 7d when due in future -> due + 7d (exact)
        let prev = due1;
        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let due2 = get_all_items(&app)[0]
            .due_date
            .expect("due set after postpone week");
        assert_eq!(due2, prev + Duration::days(7));

        // unsnooze 1d when due in future -> due - 1d (exact)
        let prev2 = due2;
        app.handle_key_event_internal(KeyEvent::new(KEY_UNSNOOZE_DAY, KeyModifiers::NONE));
        let due3 = get_all_items(&app)[0]
            .due_date
            .expect("due set after unsnooze day");
        assert_eq!(due3, prev2 - Duration::days(1));

        // prepone 7d when due in future -> due - 7d (exact)
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
        let mut app = App::new_with_clock(items, NoOpEditor, fixed_clock(base));

        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));

        let new_due_date = get_all_items(&app)[0].due_date.unwrap();

        // Should be set to base + 1 day, not past_date + 1 day
        let expected = base + Duration::days(1);

        assert_eq!(new_due_date, expected);
        assert!(new_due_date > base); // Should be in the future
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
        let mut app = App::new(items, NoOpEditor);

        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));

        let new_due_date = get_all_items(&app)[0].due_date.unwrap();
        let expected_due_date = future_date + Duration::days(1);

        // Should add 1 day to the existing future due date
        assert_eq!(new_due_date, expected_due_date);
        assert!(new_due_date > future_date); // Should be later than original
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
        let app = App::new(items, NoOpEditor);

        // Should set to base + 1 day
        let base = Utc::now();
        let mut app = App::new_with_clock(app.items.to_vec(), NoOpEditor, fixed_clock(base));
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
        let app = App::new(items, NoOpEditor);

        let base = Utc::now();
        let mut app = App::new_with_clock(app.items.to_vec(), NoOpEditor, fixed_clock(base));
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
        let app = App::new(items, NoOpEditor);

        let base = Utc::now();
        let mut app = App::new_with_clock(app.items.to_vec(), NoOpEditor, fixed_clock(base));
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
        let mut app = App::new(items, NoOpEditor);

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
        let mut app = App::new(items, NoOpEditor);

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
                selected: true, // Selected
                due_date: Some(past_date),
                google_task_id: None,
            },
            Todo {
                title: String::from("future task"),
                comment: None,
                expanded: false,
                done: false,
                selected: true, // Selected
                due_date: Some(future_date),
                google_task_id: None,
            },
            Todo {
                title: String::from("no due date task"),
                comment: None,
                expanded: false,
                done: false,
                selected: true, // Selected
                due_date: None,
                google_task_id: None,
            },
            Todo {
                title: String::from("not selected task"),
                comment: None,
                expanded: false,
                done: false,
                selected: false, // Not selected
                due_date: Some(past_date),
                google_task_id: None,
            },
        ];
        let app = App::new(items, NoOpEditor);

        // 1) s (snooze +1d)
        let mut app = App::new_with_clock(app.items.to_vec(), NoOpEditor, fixed_clock(base));
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));

        // Find items by title since sorting may change order
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

        // Overdue task: base + 1d (exact)
        assert_eq!(d1_s, base + Duration::days(1));
        // Future task: exact +1d
        assert_eq!(d2_s, future_date + Duration::days(1));
        // No due date task: base + 1d (exact)
        assert_eq!(d3_s, base + Duration::days(1));

        // 2) S (unsnooze -1d)
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

        // 3) p (postpone +7d)
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

        // 4) P (prepone -7d)
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

        // Not selected item should remain unchanged
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
        let mut app = App::new(items, NoOpEditor);

        // s (snooze +1d)
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        // S (unsnooze -1d)
        app.handle_key_event_internal(KeyEvent::new(KEY_UNSNOOZE_DAY, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        // p (postpone +7d)
        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        // P (prepone -7d)
        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        assert!(get_all_items(&app)[0].selected);
        assert!(get_all_items(&app)[1].selected);
        assert!(!get_all_items(&app)[2].selected);

        // t (custom delay) -> simulate entering "1d" and pressing Enter
        app.prompt_overlay = Some(super::PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: super::PromptAction::CustomDelay,
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
        let mut app = App::new(initial_items, mock_editor);

        // Verify initial state
        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);
        assert_eq!(app.ui_state.current_section, Section::Pending);
        assert_eq!(app.ui_state.pending_index, 0);

        // Create new item
        app.create_new_item();

        // Verify new item was added
        assert_eq!(get_all_items(&app).len(), 2);
        assert_eq!(get_all_items(&app)[1].title, "new task");
        assert_eq!(
            get_all_items(&app)[1].comment,
            Some(String::from("new comment"))
        );
        assert!(!get_all_items(&app)[1].done);
        assert_eq!(app.items.pending_count(), 2);

        // Verify cursor moved to new item
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
            title: String::from("   "), // Only whitespace
            comment: Some(String::from("comment")),
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };

        let mock_editor = MockEditor::new(empty_todo);
        let mut app = App::new(initial_items, mock_editor);

        // Verify initial state
        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);

        // Attempt to create new item with empty title
        app.create_new_item();

        // Verify item was not added
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
        let mut app = App::new(initial_items, mock_editor);

        // Verify initial state
        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(app.items.pending_count(), 1);
        assert_eq!(app.ui_state.current_section, Section::Pending);

        // Create new done item
        app.create_new_item();

        // Verify new item was added
        assert_eq!(get_all_items(&app).len(), 2);
        assert_eq!(get_all_items(&app)[1].title, "completed task");
        assert!(get_all_items(&app)[1].done);
        assert_eq!(app.items.pending_count(), 1); // Still only 1 pending item

        // Verify cursor moved to done section
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
        let mut app = App::new(Vec::new(), mock_editor);

        // Verify initial state
        assert_eq!(get_all_items(&app).len(), 0);
        assert_eq!(app.items.pending_count(), 0);

        // Create new item
        app.create_new_item();

        // Verify new item was added
        assert_eq!(get_all_items(&app).len(), 1);
        assert_eq!(get_all_items(&app)[0].title, "first task");
        assert!(!get_all_items(&app)[0].done);
        assert_eq!(app.items.pending_count(), 1);

        // Verify cursor positioned correctly
        assert_eq!(app.ui_state.current_section, Section::Pending);
        assert_eq!(app.ui_state.pending_index, 0);
    }

    #[test]
    fn prompt_widget_clears_area_and_renders_text() {
        use ratatui::{buffer::Buffer, layout::Rect};

        let area = Rect::new(0, 0, 20, 2);
        let mut buf = Buffer::empty(area);

        // Pre-fill buffer with non-space characters to ensure clearing works
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_symbol("X");
            }
        }

        // Render prompt
        let message = "Prompt: ";
        let input = "abc";
        PromptWidget::new(message, input).render(area, &mut buf);

        // Expect first line to contain message+input then spaces
        let line0: String = (0..area.width).map(|x| buf[(x, area.y)].symbol()).collect();
        let expected_content = format!("{}{}", message, input);
        let mut expected_line0 = expected_content.clone();
        if expected_line0.len() < area.width as usize {
            expected_line0.push_str(&" ".repeat(area.width as usize - expected_line0.len()));
        } else {
            expected_line0.truncate(area.width as usize);
        }
        assert_eq!(line0, expected_line0);

        // Second line should be cleared to spaces
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
    fn prompt_mode_key_end_to_end_sets_due_from_now() {
        use crossterm::event::KeyModifiers;

        // App with a single pending item
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
        let mut app = App::new_with_clock(items, NoOpEditor, fixed_clock(base));

        // Activate custom delay prompt
        app.prompt_overlay = Some(super::PromptOverlay {
            message: "Delay (e.g., 5d, -2h, 30m, 45s): ".to_string(),
            buffer: String::new(),
            action: super::PromptAction::CustomDelay,
        });

        // Type "1d" and press Enter
        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

        app.handle_prompt_mode_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let due = get_all_items(&app)[0]
            .due_date
            .expect("due date should be set");
        let expected = base + Duration::days(1);
        assert_eq!(due, expected);
        // Overlay should be cleared
        assert!(app.prompt_overlay.is_none());
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
            ("  5d  ", Duration::days(5)), // surrounding whitespace
            ("+7d", Duration::days(7)),    // explicit plus sign
            ("5 m", Duration::minutes(5)), // space before unit
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
        // Only include canonical strings that our formatter would produce
        // (seconds <60, minutes <60, hours <24, days otherwise)
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
