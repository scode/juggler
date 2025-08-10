pub const GOOGLE_OAUTH_CLIENT_ID: &str =
    "427291927957-ahaf2g5gp42oo70chpt3c189d6i7bhl8.apps.googleusercontent.com";

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

pub fn get_oauth_client_id() -> String {
    std::env::var("JUGGLER_CLIENT_ID").unwrap_or_else(|_| GOOGLE_OAUTH_CLIENT_ID.to_string())
}
