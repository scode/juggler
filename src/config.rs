pub const GOOGLE_OAUTH_CLIENT_ID: &str =
    "427291927957-ahaf2g5gp42oo70chpt3c189d6i7bhl8.apps.googleusercontent.com";

pub const GOOGLE_TASKS_LIST_NAME: &str = "juggler";

pub const GOOGLE_OAUTH_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

pub const GOOGLE_TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";

pub const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub const GOOGLE_TASKS_BASE_URL: &str = "https://tasks.googleapis.com";

/// Google OAuth client secret for the installed (Desktop) application.
///
/// Important:
/// - For installed applications, this value is not a confidential secret.
/// - Google issues a `client_secret` even for Desktop clients, but it is not treated as
///   a secret because installed apps cannot keep secrets.
/// - Security for installed apps relies on user consent, PKCE (code verifier/challenge),
///   and loopback redirect URIs — not on a private client secret.
/// - Nevertheless, Google's token endpoint may still require the `client_secret` parameter
///   to be present during code exchange and refresh for some Desktop clients.
/// - Embedding the value here is therefore expected and acceptable for Desktop apps.
/// - This does allow others to impersonate this application (reuse this client_id/secret)
///   in a PKCE flow. That is expected: Desktop clients are public clients by design. The
///   practical impact is limited to branding and quota attribution; end-user account safety
///   still depends on the user's consent and PKCE proof-of-possession.
///
/// Reference: https://developers.google.com/identity/protocols/oauth2/native-app
pub const GOOGLE_OAUTH_CLIENT_SECRET: &str = "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";

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
