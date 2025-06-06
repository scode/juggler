use chrono::{DateTime, Utc};

#[derive(Debug, serde::Deserialize)]
pub struct TodoItem {
    pub title: String,
    pub comment: Option<String>,
    #[serde(default)]
    pub done: bool,
    pub due_date: Option<DateTime<Utc>>,
}
