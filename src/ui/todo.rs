use chrono::{DateTime, Duration, Utc};
use ratatui::{
    style::{Color, Style},
    text::{Span, Text},
};

use crate::store::TodoItem;

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
                DueDateUrgency::DueSoon
            } else {
                DueDateUrgency::Normal
            }
        })
    }

    pub fn expanded_text(&self, now: DateTime<Utc>) -> Text<'_> {
        let mut first_line_spans = Vec::new();

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

pub fn parse_relative_duration(input: &str) -> Option<Duration> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    let (sign, rest) = match s.chars().next()? {
        '+' => (1i64, &s[1..]),
        '-' => (-1i64, &s[1..]),
        _ => (1i64, s),
    };

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

pub fn format_duration_compact(duration: Duration) -> String {
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
