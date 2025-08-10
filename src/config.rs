pub const GOOGLE_OAUTH_CLIENT_ID: &str =
    "427291927957-ahaf2g5gp42oo70chpt3c189d6i7bhl8.apps.googleusercontent.com";

/// Google OAuth client secret for the desktop (native) app client.
///
/// Important context for desktop/native apps:
/// - This value is not a confidential secret for installed applications. Google treats native apps as
///   public clients and expects the client secret to be embedded in the application binary/source.
/// - See Google documentation: https://developers.google.com/identity/protocols/oauth2/native-app
///   which explains that installed applications cannot keep secrets confidential and should use the
///   public client flow (PKCE). As a result, the "client secret" associated with a desktop client
///   is effectively public. Embedding it is acceptable and expected.
/// - Consequence: Anyone who has this value can technically impersonate this application during the
///   OAuth flow (they can present the same client id/secret). This is by design for native apps and
///   is mitigated by user consent and PKCE. Security relies on the user approving scopes for their
///   Google account, not on the secrecy of this string.
///
/// If a concrete secret has been issued for this client id, set it here. Leaving it empty will fall
/// back to the PKCE public client flow without sending a client_secret.
pub const GOOGLE_OAUTH_CLIENT_SECRET: &str = "";

pub const GOOGLE_TASKS_LIST_NAME: &str = "juggler";

pub const GOOGLE_OAUTH_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

pub const GOOGLE_TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";

pub const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub const GOOGLE_TASKS_BASE_URL: &str = "https://tasks.googleapis.com";

#[cfg(test)]
pub const DEFAULT_TODOS_FILE: &str = "TODOs.yaml";

pub fn get_juggler_dir() -> std::io::Result<std::path::PathBuf> {
    dirs::home_dir()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Unable to find home directory",
            )
        })
        .map(|home| home.join(".juggler"))
}

pub fn get_todos_file_path() -> std::io::Result<std::path::PathBuf> {
    get_juggler_dir().map(|dir| dir.join("TODOs.yaml"))
}

pub const DEFAULT_EDITOR: &str = "emacs";
