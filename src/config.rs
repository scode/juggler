//! Shared configuration constants and path helpers.
//!
//! This module defines cross-cutting constants used by the CLI, TUI, OAuth
//! flow, credential storage, and Google Tasks sync.
//!
//! It also provides helpers for resolving juggler's data directory and TODO
//! file path, including CLI/env overrides.

pub const CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS: &str = "google-tasks";
pub const CREDENTIAL_KEYRING_SERVICE: &str = "juggler";

pub const DEFAULT_EDITOR: &str = "emacs";

pub const DEFAULT_TOKEN_EXPIRY_SECS: u64 = 3600;

/// Indentation for expanded comment lines: cursor (2) + checkbox (4) + time (5) = 11 chars.
pub const COMMENT_INDENT: &str = "           ";

pub const DUE_SOON_THRESHOLD_SECS: i64 = 172800;

pub const GOOGLE_OAUTH_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

pub const GOOGLE_OAUTH_CLIENT_ID: &str =
    "427291927957-9bon53siil65sgblb6hi846n53ddpte3.apps.googleusercontent.com";

/// Google OAuth client secret for the desktop (native) app client.
///
/// Important context for desktop/native apps:
/// - This value is not a confidential secret for installed applications. Google treats native apps as
///   public clients and expects the client secret to be embedded in the application binary/source.
/// - See Google documentation: https://developers.google.com/identity/protocols/oauth2/native-app
///   which explains that installed applications cannot keep secrets confidential and should use the
///   public client flow (PKCE). As a result, the "client secret" associated with a desktop client
///   is effectively public. Embedding it is acceptable and expected.
/// - Consequence: Anyone who has this value can impersonate this application during the
///   OAuth flow (they can present the same client id/secret). This is by design for native apps and
///   is mitigated by user consent and PKCE. Security relies on the user approving scopes for their
///   Google account, not on the secrecy of this string.
///
/// This application embeds the client secret below as required for native clients.
pub const GOOGLE_OAUTH_CLIENT_SECRET: &str = "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";

pub const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub const GOOGLE_TASKS_BASE_URL: &str = "https://tasks.googleapis.com";

pub const GOOGLE_TASKS_LIST_NAME: &str = "juggler";

pub const GOOGLE_TASK_TITLE_PREFIX: &str = "j:";

pub const GOOGLE_TASK_OWNERSHIP_MARKER: &str = "JUGGLER_META_OWNED_V1";

pub const GOOGLE_TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";

fn resolve_juggler_dir(
    cli_override: Option<&std::path::Path>,
    env_override: Option<&std::ffi::OsStr>,
    home_dir: Option<std::path::PathBuf>,
) -> std::io::Result<std::path::PathBuf> {
    if let Some(dir) = cli_override {
        return Ok(dir.to_path_buf());
    }

    if let Some(dir) = env_override {
        return Ok(std::path::PathBuf::from(dir));
    }

    home_dir
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Unable to find home directory",
            )
        })
        .map(|home| home.join(".juggler"))
}

pub fn get_juggler_dir(
    cli_override: Option<&std::path::Path>,
) -> std::io::Result<std::path::PathBuf> {
    let env_override = std::env::var_os("JUGGLER_DIR").filter(|value| !value.is_empty());
    resolve_juggler_dir(cli_override, env_override.as_deref(), dirs::home_dir())
}

pub fn get_todos_file_path(
    cli_override: Option<&std::path::Path>,
) -> std::io::Result<std::path::PathBuf> {
    get_juggler_dir(cli_override).map(|dir| dir.join("TODOs.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, path::PathBuf};

    #[test]
    fn resolve_juggler_dir_prefers_cli_override_over_env_and_home() {
        let resolved = resolve_juggler_dir(
            Some(std::path::Path::new("cli-dir")),
            Some(OsStr::new("env-dir")),
            Some(PathBuf::from("home-dir")),
        )
        .expect("resolve juggler dir");

        assert_eq!(resolved, PathBuf::from("cli-dir"));
    }

    #[test]
    fn resolve_juggler_dir_uses_env_when_cli_override_is_missing() {
        let resolved = resolve_juggler_dir(
            None,
            Some(OsStr::new("env-dir")),
            Some(PathBuf::from("home-dir")),
        )
        .expect("resolve juggler dir");

        assert_eq!(resolved, PathBuf::from("env-dir"));
    }

    #[test]
    fn resolve_juggler_dir_defaults_to_home_subdirectory() {
        let home = PathBuf::from("home-dir");

        let resolved =
            resolve_juggler_dir(None, None, Some(home.clone())).expect("resolve juggler dir");

        assert_eq!(resolved, home.join(".juggler"));
    }

    #[test]
    fn resolve_juggler_dir_errors_without_any_available_directory_source() {
        let err = resolve_juggler_dir(None, None, None).expect_err("missing directory source");

        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn get_todos_file_path_uses_cli_override_directory() {
        let todos_path = get_todos_file_path(Some(std::path::Path::new("cli-dir")))
            .expect("resolve todos file path");

        assert_eq!(todos_path, PathBuf::from("cli-dir").join("TODOs.toml"));
    }
}
