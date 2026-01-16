use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, List, ListState, Paragraph},
};

use super::state::Section;
use super::widgets::{AppMode, PromptWidget};
use super::{App, HELP_TEXT};

impl App {
    pub(super) fn render_pending_section(&self) -> List<'_> {
        let pending_items: Vec<_> = self
            .items
            .pending_iter()
            .map(|(idx, _)| {
                ratatui::widgets::ListItem::new(self.display_text_internal(Section::Pending, idx))
            })
            .collect();

        List::new(pending_items).block(Block::default().title("Pending").borders(Borders::ALL))
    }

    pub(super) fn render_done_section(&self) -> List<'_> {
        let done_items: Vec<_> = self
            .items
            .done_iter()
            .map(|(idx, _)| {
                let mut text = self.display_text_internal(Section::Done, idx);
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

    pub(super) fn render_help_or_prompt(&self, area: Rect, frame: &mut Frame) {
        match &self.mode {
            AppMode::Prompt(prompt) => {
                frame.render_widget(PromptWidget::new(&prompt.message, &prompt.buffer), area);
            }
            AppMode::Normal => {
                let help_widget =
                    Paragraph::new(HELP_TEXT).block(Block::default().borders(Borders::TOP));
                frame.render_widget(help_widget, area);
            }
        }
    }

    pub(super) fn draw_internal(&mut self, frame: &mut Frame) {
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

    pub(super) fn display_text_internal(&self, section: Section, index: usize) -> Text<'_> {
        let todo = self.items.get(section, index).expect("valid index");
        let is_cursored =
            section == self.ui_state.current_section && index == self.ui_state.current_index();

        let cursor_prefix = if is_cursored { "▶ " } else { "  " };
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
            let color = todo
                .due_date_urgency(now)
                .map(|u| u.color())
                .unwrap_or(Color::White);
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

        if todo.expanded {
            let expanded_text = todo.expanded_text(now);
            for (i, line) in expanded_text.lines.iter().enumerate() {
                if i == 0 {
                    continue;
                }
                lines.push(line.clone());
            }
        }

        Text::from(lines)
    }
}
