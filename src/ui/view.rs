//! Stateless rendering layer for the TUI.
//!
//! This module turns immutable application state into terminal widgets using
//! ratatui. It owns layout composition, section framing, list item styling, and
//! footer rendering, but never mutates model state.
//!
//! Rendering is structured around the pending/done partition and cursor/focus
//! state in `AppModel`, with prompt mode rendered in the footer area.

use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, List, ListState, Paragraph},
};

use crate::config::COMMENT_INDENT;

use super::keymap::help_text;
use super::model::{AppMode, AppModel, Section};
use super::widgets::PromptWidget;

pub(super) fn draw(frame: &mut Frame, model: &AppModel, now: DateTime<Utc>) {
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

    let pending_widget = render_pending_section(model, now);
    let done_widget = render_done_section(model, now);

    match model.ui_state.current_section {
        Section::Pending => {
            let mut pending_state = ListState::default();
            pending_state.select(Some(model.ui_state.pending_index));
            frame.render_stateful_widget(pending_widget, sections[0], &mut pending_state);
            frame.render_widget(done_widget, sections[1]);
        }
        Section::Done => {
            frame.render_widget(pending_widget, sections[0]);
            let mut done_state = ListState::default();
            done_state.select(Some(model.ui_state.done_index));
            frame.render_stateful_widget(done_widget, sections[1], &mut done_state);
        }
    }

    render_help_or_prompt(frame, help_area, model);
}

fn render_pending_section(model: &AppModel, now: DateTime<Utc>) -> List<'_> {
    let pending_items: Vec<_> = model
        .items
        .pending_iter()
        .map(|(idx, _)| {
            ratatui::widgets::ListItem::new(display_text(model, Section::Pending, idx, now))
        })
        .collect();

    List::new(pending_items).block(Block::default().title("Pending").borders(Borders::ALL))
}

fn render_done_section(model: &AppModel, now: DateTime<Utc>) -> List<'_> {
    let done_items: Vec<_> = model
        .items
        .done_iter()
        .map(|(idx, _)| {
            let mut text = display_text(model, Section::Done, idx, now);
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

fn render_help_or_prompt(frame: &mut Frame, area: Rect, model: &AppModel) {
    match &model.mode {
        AppMode::Prompt(prompt) => {
            frame.render_widget(PromptWidget::new(&prompt.message, &prompt.buffer), area);
        }
        AppMode::Normal => {
            let help_widget =
                Paragraph::new(help_text()).block(Block::default().borders(Borders::TOP));
            frame.render_widget(help_widget, area);
        }
    }
}

pub(super) fn display_text(
    model: &AppModel,
    section: Section,
    index: usize,
    now: DateTime<Utc>,
) -> Text<'_> {
    let todo = model.items.get(section, index).expect("valid index");
    let is_cursored =
        section == model.ui_state.current_section && index == model.ui_state.current_index();

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
    if todo.expanded
        && has_comment
        && let Some(comment) = &todo.comment
    {
        for line in comment.lines() {
            lines.push(ratatui::text::Line::from(vec![
                Span::raw(COMMENT_INDENT),
                Span::raw(line),
            ]));
        }
    }

    Text::from(lines)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        text::{Span, Text},
    };

    use super::*;
    use crate::ui::model::AppModel;
    use crate::ui::todo::Todo;

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

    #[test]
    fn display_text_prefixes_cursor_and_comment_lines() {
        let first = todo("a");
        let mut second = todo("b");
        second.comment = Some("c1\nc2".to_string());
        second.expanded = true;
        let model = AppModel::new(vec![first, second]);

        assert_eq!(
            text_to_string(&display_text(&model, Section::Pending, 0, Utc::now())),
            "▶ [ ] a"
        );
        assert_eq!(
            text_to_string(&display_text(&model, Section::Pending, 1, Utc::now())),
            "  [ ] b >>>\n           c1\n           c2"
        );
    }

    #[test]
    fn display_text_shows_relative_due_time() {
        let base = Utc::now();
        let mut item = todo("future task");
        item.due_date = Some(base + Duration::hours(50));
        let model = AppModel::new(vec![item]);

        assert_eq!(
            text_to_string(&display_text(&model, Section::Pending, 0, base)),
            "▶ [ ]   2d future task"
        );
    }

    #[test]
    fn display_text_marks_selected_items() {
        let mut item = todo("first");
        item.selected = true;
        let model = AppModel::new(vec![item]);
        assert_eq!(
            text_to_string(&display_text(&model, Section::Pending, 0, Utc::now())),
            "▶ [x] first"
        );
    }

    #[test]
    fn draw_renders_help_footer_in_normal_mode() {
        let help = help_text();
        let width = (help.len() as u16).saturating_add(2);
        let backend = TestBackend::new(width, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let model = AppModel::new(Vec::new());

        terminal
            .draw(|frame| draw(frame, &model, Utc::now()))
            .unwrap();

        let buf = terminal.backend().buffer();
        let bottom_y = buf.area.bottom() - 1;
        let line: String = (0..buf.area.width)
            .map(|x| buf[(x, bottom_y)].symbol())
            .collect();
        assert_eq!(line.trim_end(), help);
    }

    #[test]
    fn draw_renders_prompt_footer_in_prompt_mode() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = AppModel::new(Vec::new());
        model.mode = AppMode::Prompt(super::super::model::PromptOverlay {
            message: "Delay: ".to_string(),
            buffer: "1d".to_string(),
            action: super::super::model::PromptAction::CustomDelay,
        });

        terminal
            .draw(|frame| draw(frame, &model, Utc::now()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let found = (0..buf.area.height).any(|y| {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            line.starts_with("Delay: 1d")
        });
        assert!(found);
    }
}
