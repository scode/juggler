use std::io;

use chrono::{DateTime, Duration, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, Clear, List, ListState, Paragraph},
};

use crate::store::{TodoItem, edit_todo_item};

pub trait TodoEditor {
    fn edit_todo(&self, todo: &Todo) -> io::Result<Todo>;
    fn needs_terminal_restoration(&self) -> bool;
}

pub struct ExternalEditor;

impl TodoEditor for ExternalEditor {
    fn edit_todo(&self, todo: &Todo) -> io::Result<Todo> {
        edit_todo_item(todo)
    }

    fn needs_terminal_restoration(&self) -> bool {
        true
    }
}

pub const HELP_TEXT: &str = "o - open, j/k - nav, x - select, e - done, E - edit, c - new, s - snooze 1d, S - unsnooze 1d, p - snooze 7d, P - prepone 7d, t - custom delay, q - quit";

pub const KEY_QUIT: KeyCode = KeyCode::Char('q');
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
    pub fn format_relative_time(&self) -> Option<String> {
        self.due_date.map(|due| {
            let now = Utc::now();
            let duration = due.signed_duration_since(now);

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

            let time_str = if total_seconds < 0 {
                format!("-{value}{unit}")
            } else {
                format!("{value}{unit}")
            };

            // Right-pad to 4 characters for alignment
            format!("{time_str:>4}")
        })
    }

    pub fn due_date_urgency(&self) -> Option<DueDateUrgency> {
        self.due_date.map(|due| {
            let now = Utc::now();
            let duration = due.signed_duration_since(now);
            let total_seconds = duration.num_seconds();

            if total_seconds < 0 {
                DueDateUrgency::Overdue
            } else if total_seconds <= 86400 {
                // 24 hours
                DueDateUrgency::DueSoon
            } else {
                DueDateUrgency::Normal
            }
        })
    }

    pub fn expanded_text(&self) -> Text<'_> {
        let mut first_line_spans = Vec::new();

        // Add relative time if due date exists
        if let Some(relative_time) = self.format_relative_time() {
            let color = match self.due_date_urgency() {
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
        let has_comment = self
            .comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false);
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
                    Span::raw("         "),
                    Span::raw(line),
                ]));
            }
        }

        Text::from(lines)
    }

    #[cfg(test)]
    pub fn has_comment(&self) -> bool {
        self.comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
    }

    #[cfg(test)]
    pub fn collapsed_summary(&self) -> Vec<Span<'_>> {
        let mut spans = Vec::new();

        // Add relative time if due date exists
        if let Some(relative_time) = self.format_relative_time() {
            let color = match self.due_date_urgency() {
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
            spans.push(Span::raw(" >"));
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

#[derive(Debug, Default, Clone)]
struct PromptOverlay {
    message: String,
    buffer: String,
}

#[derive(Debug)]
pub struct App<T: TodoEditor> {
    exit: bool,
    items: Vec<Todo>,
    pending_count: usize,
    current_section: Section,
    pending_index: usize,
    done_index: usize,
    editor: T,
    prompt_overlay: Option<PromptOverlay>,
}

#[derive(Debug, Clone, PartialEq)]
enum Section {
    Pending,
    Done,
}

impl<T: TodoEditor> App<T> {
    pub fn new(items: Vec<Todo>, editor: T) -> Self {
        let pending_count = items.iter().filter(|item| !item.done).count();
        let current_section = if pending_count > 0 {
            Section::Pending
        } else {
            Section::Done
        };

        App {
            exit: false,
            items,
            pending_count,
            current_section,
            pending_index: 0,
            done_index: 0,
            editor,
            prompt_overlay: None,
        }
    }

    pub fn items(&self) -> &[Todo] {
        &self.items
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw_internal(frame))?;
            self.handle_events(terminal)?;
        }
        Ok(())
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

        // Split main area between pending and done sections
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(main_area);

        // Render pending section
        let pending_items: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.done)
            .map(|(original_idx, _)| {
                ratatui::widgets::ListItem::new(self.display_text_internal(original_idx))
            })
            .collect();

        let pending_widget =
            List::new(pending_items).block(Block::default().title("Pending").borders(Borders::ALL));

        // Render done section
        let done_items: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.done)
            .map(|(original_idx, _)| {
                let mut text = self.display_text_internal(original_idx);
                // Apply crossed-out style to all spans
                for line in &mut text.lines {
                    for span in &mut line.spans {
                        span.style = span.style.add_modifier(Modifier::CROSSED_OUT);
                    }
                }
                ratatui::widgets::ListItem::new(text)
            })
            .collect();

        let done_widget =
            List::new(done_items).block(Block::default().title("Done").borders(Borders::ALL));

        // Determine which section to highlight based on current section
        match self.current_section {
            Section::Pending => {
                let mut pending_state = ListState::default();
                pending_state.select(Some(self.pending_index));
                frame.render_stateful_widget(pending_widget, sections[0], &mut pending_state);
                frame.render_widget(done_widget, sections[1]);
            }
            Section::Done => {
                frame.render_widget(pending_widget, sections[0]);
                let mut done_state = ListState::default();
                done_state.select(Some(self.done_index));
                frame.render_stateful_widget(done_widget, sections[1], &mut done_state);
            }
        }

        if let Some(prompt) = &self.prompt_overlay {
            // Replace help area with prompt on a blank background
            frame.render_widget(Clear, help_area);
            let text = format!("{}{}", prompt.message, prompt.buffer);
            frame.render_widget(Paragraph::new(text), help_area);
        } else {
            let help_widget =
                Paragraph::new(HELP_TEXT).block(Block::default().borders(Borders::TOP));
            frame.render_widget(help_widget, help_area);
        }
    }

    fn display_text_internal(&self, index: usize) -> Text<'_> {
        let todo = &self.items[index];
        let is_selected = Some(index) == self.get_selected_item_index();

        let cursor_prefix = if is_selected { "▶ " } else { "  " };
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

        if let Some(relative_time) = todo.format_relative_time() {
            let color = match todo.due_date_urgency() {
                Some(DueDateUrgency::Overdue) => Color::Red,
                Some(DueDateUrgency::DueSoon) => Color::Yellow,
                _ => Color::White,
            };
            first_line_spans.push(Span::styled(
                format!("{relative_time} "),
                Style::default().fg(color),
            ));
        }

        if is_selected {
            first_line_spans.push(Span::styled(
                &todo.title,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        } else {
            first_line_spans.push(Span::raw(&todo.title));
        }

        let has_comment = todo
            .comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false);
        if todo.expanded && has_comment {
            first_line_spans.push(Span::raw(" >>>"));
        }

        let mut lines = vec![ratatui::text::Line::from(first_line_spans)];

        // For expanded items, append additional lines using expanded_text()
        if todo.expanded {
            let expanded_text = todo.expanded_text();
            for (i, line) in expanded_text.lines.iter().enumerate() {
                if i == 0 {
                    continue; // skip first line, we already built it with cursor/checkbox
                }
                lines.push(line.clone());
            }
        }

        Text::from(lines)
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
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
                } else if key_event.code == KEY_CUSTOM_DELAY {
                    self.handle_custom_delay(terminal)?;
                } else {
                    self.handle_key_event_internal(key_event);
                }
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event_internal(&mut self, key_event: KeyEvent) {
        //dbg!(key_event);
        match key_event.code {
            KEY_QUIT => self.exit(),
            KEY_NEXT_ITEM => self.select_next_internal(),
            KEY_PREVIOUS_ITEM => self.select_previous_internal(),
            KEY_TOGGLE_EXPAND => self.toggle_selected(),
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

    fn toggle_selected(&mut self) {
        if let Some(i) = self.get_selected_item_index()
            && let Some(item) = self.items.get_mut(i)
        {
            item.expanded = !item.expanded;
        }
    }

    fn select_next_internal(&mut self) {
        let pending_items: Vec<_> = self.items.iter().filter(|item| !item.done).collect();
        let done_items: Vec<_> = self.items.iter().filter(|item| item.done).collect();

        match self.current_section {
            Section::Pending => {
                if !pending_items.is_empty() {
                    self.pending_index += 1;
                    if self.pending_index >= pending_items.len() {
                        // Move to done section if available
                        if !done_items.is_empty() {
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
                if !done_items.is_empty() {
                    self.done_index += 1;
                    if self.done_index >= done_items.len() {
                        // Move to pending section if available
                        if !pending_items.is_empty() {
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

    fn select_previous_internal(&mut self) {
        let pending_items: Vec<_> = self.items.iter().filter(|item| !item.done).collect();
        let done_items: Vec<_> = self.items.iter().filter(|item| item.done).collect();

        match self.current_section {
            Section::Pending => {
                if !pending_items.is_empty() {
                    if self.pending_index == 0 {
                        // Move to end of done section if available
                        if !done_items.is_empty() {
                            self.current_section = Section::Done;
                            self.done_index = done_items.len() - 1;
                        } else {
                            // Wrap around to end of pending
                            self.pending_index = pending_items.len() - 1;
                        }
                    } else {
                        self.pending_index -= 1;
                    }
                }
            }
            Section::Done => {
                if !done_items.is_empty() {
                    if self.done_index == 0 {
                        // Move to end of pending section if available
                        if !pending_items.is_empty() {
                            self.current_section = Section::Pending;
                            self.pending_index = pending_items.len() - 1;
                        } else {
                            // Wrap around to end of done
                            self.done_index = done_items.len() - 1;
                        }
                    } else {
                        self.done_index -= 1;
                    }
                }
            }
        }
    }

    fn toggle_done(&mut self) {
        let selected_indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
            .collect();

        if !selected_indices.is_empty() {
            // If there are selected items, toggle their done status
            for i in selected_indices {
                if let Some(item) = self.items.get_mut(i) {
                    item.done = !item.done;
                    item.selected = false; // Deselect after toggling
                    // Collapse item when marked as done
                    if item.done {
                        item.expanded = false;
                    }
                }
            }
        } else if let Some(cursor_idx) = self.get_selected_item_index() {
            // If no items are selected, toggle the item under cursor
            if let Some(item) = self.items.get_mut(cursor_idx) {
                item.done = !item.done;
                // Collapse item when marked as done
                if item.done {
                    item.expanded = false;
                }
            }
        }

        // Update pending count
        self.pending_count = self.items.iter().filter(|item| !item.done).count();

        // Adjust indices after toggling done status
        self.adjust_indices_after_toggle();
    }

    fn toggle_select(&mut self) {
        if let Some(i) = self.get_selected_item_index()
            && let Some(item) = self.items.get_mut(i)
        {
            item.selected = !item.selected;
        }
    }

    fn snooze(&mut self, duration: Duration) {
        let selected_indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
            .collect();

        if !selected_indices.is_empty() {
            // If there are selected items, snooze them
            for i in selected_indices {
                if let Some(item) = self.items.get_mut(i) {
                    let now = Utc::now();
                    let new_due_date = if let Some(current_due) = item.due_date {
                        if current_due <= now {
                            // If current due date is in the past, set to current time + snooze period
                            now + duration
                        } else {
                            // If current due date is in the future, add snooze period to existing due date
                            current_due + duration
                        }
                    } else {
                        // If no due date exists, set to current time + snooze period
                        now + duration
                    };
                    item.due_date = Some(new_due_date);
                    item.selected = false; // Deselect after snoozing
                }
            }
        } else if let Some(cursor_idx) = self.get_selected_item_index() {
            // If no items are selected, snooze the item under cursor
            if let Some(item) = self.items.get_mut(cursor_idx) {
                let now = Utc::now();
                let new_due_date = if let Some(current_due) = item.due_date {
                    if current_due <= now {
                        // If current due date is in the past, set to current time + snooze period
                        now + duration
                    } else {
                        // If current due date is in the future, add snooze period to existing due date
                        current_due + duration
                    }
                } else {
                    // If no due date exists, set to current time + snooze period
                    now + duration
                };
                item.due_date = Some(new_due_date);
            }
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
        if let Some(cursor_idx) = self.get_selected_item_index()
            && let Some(item) = self.items.get(cursor_idx)
        {
            let result = self.editor.edit_todo(item);

            match result {
                Ok(updated_item) => {
                    self.items[cursor_idx] = updated_item;
                    // Update pending count in case done status changed
                    self.pending_count = self.items.iter().filter(|item| !item.done).count();
                }
                Err(_) => {
                    // Editor failed or was cancelled - do nothing
                    // In a more sophisticated app, we might show an error message
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
                    // Update pending count
                    self.pending_count = self.items.iter().filter(|item| !item.done).count();

                    // Move cursor to the newly created item
                    if !self.items.is_empty() {
                        // If the new item is not done, it will be in the pending section
                        if !is_done {
                            self.current_section = Section::Pending;
                            // Find the position of the new item in the pending items
                            let pending_items: Vec<_> = self
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| !item.done)
                                .collect();
                            let new_item_idx = self.items.len() - 1;
                            if let Some(pos) = pending_items
                                .iter()
                                .position(|(idx, _)| *idx == new_item_idx)
                            {
                                self.pending_index = pos;
                            }
                        } else {
                            // If the new item is done, it will be in the done section
                            self.current_section = Section::Done;
                            // Find the position of the new item in the done items
                            let done_items: Vec<_> = self
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| item.done)
                                .collect();
                            let new_item_idx = self.items.len() - 1;
                            if let Some(pos) =
                                done_items.iter().position(|(idx, _)| *idx == new_item_idx)
                            {
                                self.done_index = pos;
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // Editor failed or was cancelled - do nothing
                // In a more sophisticated app, we might show an error message
            }
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn get_selected_item_index(&self) -> Option<usize> {
        let pending_items: Vec<(usize, &Todo)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.done)
            .collect();
        let done_items: Vec<(usize, &Todo)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.done)
            .collect();

        match self.current_section {
            Section::Pending => {
                if self.pending_index < pending_items.len() {
                    Some(pending_items[self.pending_index].0)
                } else {
                    None
                }
            }
            Section::Done => {
                if self.done_index < done_items.len() {
                    Some(done_items[self.done_index].0)
                } else {
                    None
                }
            }
        }
    }

    fn adjust_indices_after_toggle(&mut self) {
        let pending_items: Vec<_> = self.items.iter().filter(|item| !item.done).collect();
        let done_items: Vec<_> = self.items.iter().filter(|item| item.done).collect();

        // Clamp indices to valid ranges
        if pending_items.is_empty() {
            self.pending_index = 0;
            if self.current_section == Section::Pending && !done_items.is_empty() {
                self.current_section = Section::Done;
                self.done_index = 0;
            }
        } else if self.pending_index >= pending_items.len() {
            self.pending_index = pending_items.len() - 1;
        }

        if done_items.is_empty() {
            self.done_index = 0;
            if self.current_section == Section::Done && !pending_items.is_empty() {
                self.current_section = Section::Pending;
                self.pending_index = 0;
            }
        } else if self.done_index >= done_items.len() {
            self.done_index = done_items.len() - 1;
        }
    }

    fn handle_custom_delay(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        if let Some(input) = self.prompt_input(terminal, "Delay (e.g., 5d, -2h, 30m, 45s): ")?
            && let Some(duration) = parse_relative_duration(&input)
        {
            self.delay_from_now(duration);
        }
        Ok(())
    }

    fn prompt_input(
        &mut self,
        terminal: &mut DefaultTerminal,
        prompt: &str,
    ) -> io::Result<Option<String>> {
        use crossterm::event::{KeyEvent, KeyModifiers};

        // Activate overlay
        self.prompt_overlay = Some(PromptOverlay {
            message: prompt.to_string(),
            buffer: String::new(),
        });

        loop {
            terminal.draw(|frame| self.draw_internal(frame))?;

            match event::read()? {
                Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    ..
                }) => {
                    let result = self
                        .prompt_overlay
                        .as_ref()
                        .map(|p| p.buffer.clone())
                        .unwrap_or_default();
                    self.prompt_overlay = None;
                    return Ok(Some(result));
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Esc, ..
                }) => {
                    self.prompt_overlay = None;
                    return Ok(None);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers,
                    ..
                }) => {
                    if (modifiers.is_empty() || modifiers == KeyModifiers::SHIFT)
                        && let Some(p) = self.prompt_overlay.as_mut()
                    {
                        p.buffer.push(c);
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                }) => {
                    if let Some(p) = self.prompt_overlay.as_mut() {
                        p.buffer.pop();
                    }
                }
                _ => {}
            }
        }
    }

    fn delay_from_now(&mut self, duration: Duration) {
        let now = Utc::now();
        let target_due = now + duration;

        let selected_indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| if item.selected { Some(i) } else { None })
            .collect();

        if !selected_indices.is_empty() {
            for i in selected_indices {
                if let Some(item) = self.items.get_mut(i) {
                    item.due_date = Some(target_due);
                    item.selected = false;
                }
            }
        } else if let Some(cursor_idx) = self.get_selected_item_index()
            && let Some(item) = self.items.get_mut(cursor_idx)
        {
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

    // Test-only editor that doesn't do anything
    struct NoOpEditor;

    impl TodoEditor for NoOpEditor {
        fn edit_todo(&self, todo: &Todo) -> io::Result<Todo> {
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
        fn edit_todo(&self, _todo: &Todo) -> io::Result<Todo> {
            Ok(self.return_todo.clone())
        }

        fn needs_terminal_restoration(&self) -> bool {
            false
        }
    }

    #[test]
    fn toggle_selected_via_key_event() {
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
        assert!(app.items[0].expanded);
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_EXPAND, KeyModifiers::NONE));
        assert!(!app.items[0].expanded);
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
        assert_eq!(spans_to_string(&with_comment.collapsed_summary()), "a >");

        let without_comment = Todo {
            title: String::from("b"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: None,
            google_task_id: None,
        };
        assert_eq!(spans_to_string(&without_comment.collapsed_summary()), "b");
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
            text_to_string(&todo.expanded_text()),
            "a >>>\n         line1\n         line2"
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
        let app = App::new(items, NoOpEditor);

        assert_eq!(text_to_string(&app.display_text_internal(0)), "▶ [ ] a");
        assert_eq!(
            text_to_string(&app.display_text_internal(1)),
            "  [ ] b >>>\n         c1\n         c2"
        );
    }

    #[test]
    fn display_text_shows_relative_time_for_future_due_date() {
        let items = vec![Todo {
            title: String::from("future task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(chrono::Utc::now() + chrono::Duration::hours(50)),
            google_task_id: None,
        }];
        let app = App::new(items, NoOpEditor);

        // 50h in the future should render as right-aligned "  2d"
        assert_eq!(
            text_to_string(&app.display_text_internal(0)),
            "▶ [ ]   2d future task"
        );
    }

    #[test]
    fn expanded_display_includes_relative_time_and_comment_lines() {
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
                due_date: Some(chrono::Utc::now() + chrono::Duration::hours(50)),
                google_task_id: None,
            },
        ];
        let app = App::new(items, NoOpEditor);

        assert_eq!(
            text_to_string(&app.display_text_internal(1)),
            "  [ ]   2d b >>>\n         c1\n         c2"
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
        assert_eq!(text_to_string(&app.display_text_internal(0)), "▶ [ ] first");

        // Toggle selection on current item
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_SELECT, KeyModifiers::NONE));
        assert!(app.items[0].selected);
        assert_eq!(text_to_string(&app.display_text_internal(0)), "▶ [x] first");

        // Move cursor and select second item as well
        app.select_next_internal();
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_SELECT, KeyModifiers::NONE));
        assert!(app.items[1].selected);
        assert_eq!(
            text_to_string(&app.display_text_internal(1)),
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
            spans_to_string(&collapsed_with_comment.collapsed_summary()),
            "Task with details >"
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
            text_to_string(&expanded_with_comment.expanded_text()),
            "Task with details >>>\n         Some details"
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
            spans_to_string(&no_comment.collapsed_summary()),
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
            spans_to_string(&empty_comment.collapsed_summary()),
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
        assert!(app.items[0].done);
        assert_eq!(app.pending_count, 0);

        // Toggle back to pending
        app.handle_key_event_internal(KeyEvent::new(KEY_TOGGLE_DONE, KeyModifiers::NONE));
        assert!(!app.items[0].done);
        assert_eq!(app.pending_count, 1);
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

        assert!(app.items[0].done); // Selected item should be marked done
        assert!(!app.items[1].done); // Cursor item should remain unchanged
        assert!(app.items[2].done); // Selected item should be marked done
        assert_eq!(app.pending_count, 1); // Only one pending item left

        // Items should be deselected after toggling
        assert!(!app.items[0].selected);
        assert!(!app.items[1].selected);
        assert!(!app.items[2].selected);
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

        assert!(!app.items[0].done); // First item should remain unchanged
        assert!(app.items[1].done); // Cursor item should be marked done
        assert_eq!(app.pending_count, 1); // One pending item left
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
        let mut app = App::new(items, NoOpEditor);

        // snooze 1d when no due date -> now + 1d (range-checked)
        let before1 = Utc::now();
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let after1 = Utc::now();
        let due1 = app.items[0].due_date.expect("due set after snooze day");
        assert!(due1 >= before1 + Duration::days(1) && due1 <= after1 + Duration::days(1));

        // postpone 7d when due in future -> due + 7d (exact)
        let prev = due1;
        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let due2 = app.items[0].due_date.expect("due set after postpone week");
        assert_eq!(due2, prev + Duration::days(7));

        // unsnooze 1d when due in future -> due - 1d (exact)
        let prev2 = due2;
        app.handle_key_event_internal(KeyEvent::new(KEY_UNSNOOZE_DAY, KeyModifiers::NONE));
        let due3 = app.items[0].due_date.expect("due set after unsnooze day");
        assert_eq!(due3, prev2 - Duration::days(1));

        // prepone 7d when due in future -> due - 7d (exact)
        let prev3 = due3;
        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        let due4 = app.items[0].due_date.expect("due set after prepone week");
        assert_eq!(due4, prev3 - Duration::days(7));
    }

    #[test]
    fn snooze_with_past_due_date() {
        let past_date = Utc::now() - Duration::days(2); // 2 days ago
        let items = vec![Todo {
            title: String::from("overdue task"),
            comment: None,
            expanded: false,
            done: false,
            selected: false,
            due_date: Some(past_date),
            google_task_id: None,
        }];
        let mut app = App::new(items, NoOpEditor);

        let before_snooze = Utc::now();
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let after_snooze = Utc::now();

        let new_due_date = app.items[0].due_date.unwrap();

        // Should be set to current time + 1 day, not past_date + 1 day
        let expected_min = before_snooze + Duration::days(1);
        let expected_max = after_snooze + Duration::days(1);

        assert!(new_due_date >= expected_min && new_due_date <= expected_max);
        assert!(new_due_date > Utc::now()); // Should be in the future
    }

    #[test]
    fn snooze_with_future_due_date() {
        let future_date = Utc::now() + Duration::days(3); // 3 days from now
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

        let new_due_date = app.items[0].due_date.unwrap();
        let expected_due_date = future_date + Duration::days(1);

        // Should add 1 day to the existing future due date
        let diff = (new_due_date - expected_due_date).num_seconds().abs();
        assert!(diff < 5); // Allow small timing differences
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
        let mut app = App::new(items, NoOpEditor);

        // Should set to current time + 1 day
        let before_snooze = Utc::now();
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let after_snooze = Utc::now();
        let new_due_date = app.items[0].due_date.unwrap();
        let expected_min = before_snooze + Duration::days(1);
        let expected_max = after_snooze + Duration::days(1);
        assert!(new_due_date >= expected_min && new_due_date <= expected_max);
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
        let mut app = App::new(items, NoOpEditor);

        let before = Utc::now();
        app.handle_key_event_internal(KeyEvent::new(KEY_POSTPONE_WEEK, KeyModifiers::NONE));
        let after = Utc::now();
        let new_due_date = app.items[0].due_date.unwrap();
        let expected_min = before + Duration::days(7);
        let expected_max = after + Duration::days(7);
        assert!(new_due_date >= expected_min && new_due_date <= expected_max);
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
        let mut app = App::new(items, NoOpEditor);

        let before = Utc::now();
        app.handle_key_event_internal(KeyEvent::new(KEY_PREPONE_WEEK, KeyModifiers::NONE));
        let after = Utc::now();
        let new_due_date = app.items[0].due_date.unwrap();
        let expected_min = before - Duration::days(7);
        let expected_max = after - Duration::days(7);
        assert!(new_due_date >= expected_min && new_due_date <= expected_max);
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
        let new_due_date = app.items[0].due_date.unwrap();
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
        let new_due_date = app.items[0].due_date.unwrap();
        assert_eq!(new_due_date, future_date - Duration::days(7));
    }

    #[test]
    fn snooze_multiple_selected_items_mixed_due_dates() {
        let past_date = Utc::now() - Duration::days(1); // 1 day ago
        let future_date = Utc::now() + Duration::days(2); // 2 days from now

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
        let mut app = App::new(items, NoOpEditor);

        let before_snooze = Utc::now();
        app.handle_key_event_internal(KeyEvent::new(KEY_SNOOZE_DAY, KeyModifiers::NONE));
        let after_snooze = Utc::now();

        // First item (overdue): should be set to current time + 1 day
        let new_due_date_1 = app.items[0].due_date.unwrap();
        let expected_min_1 = before_snooze + Duration::days(1);
        let expected_max_1 = after_snooze + Duration::days(1);
        assert!(new_due_date_1 >= expected_min_1 && new_due_date_1 <= expected_max_1);

        // Second item (future): should be original + 1 day
        let new_due_date_2 = app.items[1].due_date.unwrap();
        let expected_due_date_2 = future_date + Duration::days(1);
        let diff_2 = (new_due_date_2 - expected_due_date_2).num_seconds().abs();
        assert!(diff_2 < 5);

        // Third item (no due date): should be set to current time + 1 day
        let new_due_date_3 = app.items[2].due_date.unwrap();
        let expected_min_3 = before_snooze + Duration::days(1);
        let expected_max_3 = after_snooze + Duration::days(1);
        assert!(new_due_date_3 >= expected_min_3 && new_due_date_3 <= expected_max_3);

        // Fourth item (not selected): should remain unchanged
        assert_eq!(app.items[3].due_date, Some(past_date));

        // All selected items should be deselected after snoozing
        assert!(!app.items[0].selected);
        assert!(!app.items[1].selected);
        assert!(!app.items[2].selected);
        assert!(!app.items[3].selected); // Wasn't selected to begin with
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
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.pending_count, 1);
        assert_eq!(app.current_section, Section::Pending);
        assert_eq!(app.pending_index, 0);

        // Create new item
        app.create_new_item();

        // Verify new item was added
        assert_eq!(app.items.len(), 2);
        assert_eq!(app.items[1].title, "new task");
        assert_eq!(app.items[1].comment, Some(String::from("new comment")));
        assert!(!app.items[1].done);
        assert_eq!(app.pending_count, 2);

        // Verify cursor moved to new item
        assert_eq!(app.current_section, Section::Pending);
        assert_eq!(app.pending_index, 1);
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
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.pending_count, 1);

        // Attempt to create new item with empty title
        app.create_new_item();

        // Verify item was not added
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.pending_count, 1);
        assert_eq!(app.items[0].title, "existing task");
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
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.pending_count, 1);
        assert_eq!(app.current_section, Section::Pending);

        // Create new done item
        app.create_new_item();

        // Verify new item was added
        assert_eq!(app.items.len(), 2);
        assert_eq!(app.items[1].title, "completed task");
        assert!(app.items[1].done);
        assert_eq!(app.pending_count, 1); // Still only 1 pending item

        // Verify cursor moved to done section
        assert_eq!(app.current_section, Section::Done);
        assert_eq!(app.done_index, 0);
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
        assert_eq!(app.items.len(), 0);
        assert_eq!(app.pending_count, 0);

        // Create new item
        app.create_new_item();

        // Verify new item was added
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.items[0].title, "first task");
        assert!(!app.items[0].done);
        assert_eq!(app.pending_count, 1);

        // Verify cursor positioned correctly
        assert_eq!(app.current_section, Section::Pending);
        assert_eq!(app.pending_index, 0);
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
            "",
            " ",
            "s",
            "d",
            "+",
            "-",
            "+d",
            "-h",
            "5",
            "d5",
            "5x",
            "5days",
            "--5d",
            "++5d",
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
            "0s", "1s", "59s",
            "1m", "2m", "59m",
            "1h", "2h", "23h",
            "1d", "2d", "10d",
            "-1s", "-59s",
            "-1m", "-59m",
            "-1h", "-23h",
            "-1d", "-10d",
        ];

        for s in canonical { 
            let dur = parse_relative_duration(s).expect("parse canonical");
            let back = format_duration_compact(dur);
            assert_eq!(back, s, "round-trip failed for {s}");
        }
    }
}
