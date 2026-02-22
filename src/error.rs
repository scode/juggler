//! Application-wide error types and result alias.
//!
//! `JugglerError` collects failures from I/O, serialization, HTTP, OAuth,
//! Google Tasks operations, and credential storage into one enum.
//!
//! Modules return the shared `Result<T>` alias so command handlers and runtime
//! code can propagate errors through a consistent type.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum JugglerError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("TOML parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Google Tasks API error: {0}")]
    GoogleTasks(String),

    #[error("Credential error: {0}")]
    Credential(#[from] crate::credential_storage::CredentialError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}

impl JugglerError {
    #[cfg(test)]
    pub fn new<S: Into<String>>(msg: S) -> Self {
        JugglerError::Other(msg.into())
    }

    pub fn oauth<S: Into<String>>(msg: S) -> Self {
        JugglerError::OAuth(msg.into())
    }

    pub fn google_tasks<S: Into<String>>(msg: S) -> Self {
        JugglerError::GoogleTasks(msg.into())
    }

    pub fn config<S: Into<String>>(msg: S) -> Self {
        JugglerError::Config(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, JugglerError>;

impl From<String> for JugglerError {
    fn from(s: String) -> Self {
        JugglerError::Other(s)
    }
}

impl From<&str> for JugglerError {
    fn from(s: &str) -> Self {
        JugglerError::Other(s.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for JugglerError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        JugglerError::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = JugglerError::new("test error");
        assert_eq!(err.to_string(), "test error");
    }

    #[test]
    fn test_oauth_error() {
        let err = JugglerError::oauth("invalid token");
        assert_eq!(err.to_string(), "OAuth error: invalid token");
    }

    #[test]
    fn test_google_tasks_error() {
        let err = JugglerError::google_tasks("API rate limit");
        assert_eq!(err.to_string(), "Google Tasks API error: API rate limit");
    }

    #[test]
    fn test_config_error() {
        let err = JugglerError::config("missing file");
        assert_eq!(err.to_string(), "Configuration error: missing file");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err: JugglerError = io_err.into();
        assert!(matches!(err, JugglerError::Io(_)));
    }

    #[test]
    fn test_string_conversion() {
        let err: JugglerError = "string error".into();
        assert_eq!(err.to_string(), "string error");

        let err: JugglerError = String::from("owned string error").into();
        assert_eq!(err.to_string(), "owned string error");
    }
}
