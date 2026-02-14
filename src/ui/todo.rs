//! Runtime todo domain model shared by UI and sync flows.
//!
//! This module defines the in-memory task representation used during interactive
//! editing and synchronization. It extends persisted fields with UI-only state
//! (expanded/selected) so interface concerns do not pollute storage schema.
//!
//! It also parses compact relative-delay expressions (for example `5d`, `-2h`)
//! used by prompt-driven scheduling, and contains due-date formatting helpers.

use chrono::{DateTime, Duration, Utc};
use ratatui::style::Color;

use crate::config::DUE_SOON_THRESHOLD_SECS;
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
            } else if total_seconds <= DUE_SOON_THRESHOLD_SECS {
                DueDateUrgency::DueSoon
            } else {
                DueDateUrgency::Normal
            }
        })
    }

    pub fn has_comment(&self) -> bool {
        self.comment
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DueDateUrgency {
    Overdue,
    DueSoon,
    Normal,
}

impl DueDateUrgency {
    pub fn color(&self) -> Color {
        match self {
            DueDateUrgency::Overdue => Color::Red,
            DueDateUrgency::DueSoon => Color::Yellow,
            DueDateUrgency::Normal => Color::White,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
