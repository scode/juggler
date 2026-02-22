use env_logger::Env;

use log::{error, info};

mod config;
mod credential_storage;
mod error;
mod google_tasks;
mod oauth;
mod store;
mod time;
mod ui;

use error::{JugglerError, Result};

use clap::{Parser, Subcommand};
use config::{
    CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS, CREDENTIAL_KEYRING_SERVICE, GOOGLE_OAUTH_CLIENT_ID,
    get_todos_file_path,
};
use credential_storage::{CredentialStore, KeyringCredentialStore};
use google_tasks::{GoogleOAuthClient, GoogleOAuthCredentials, sync_to_tasks_with_oauth};
use oauth::run_oauth_flow;
use store::{load_todos, store_todos};
use ui::{App, ExternalEditor, Todo};

fn create_oauth_client_from_keychain(
    cred_store: &dyn CredentialStore,
    http_client: reqwest::Client,
) -> Result<GoogleOAuthClient> {
    let refresh_token = cred_store.get_refresh_token().map_err(|_| {
        JugglerError::config(
            "No refresh token found in keychain. Run `juggler login` to authenticate.",
        )
    })?;

    let credentials = GoogleOAuthCredentials {
        client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
        refresh_token,
    };

    Ok(GoogleOAuthClient::new(credentials, http_client))
}

fn maybe_persist_todos_after_sync(
    todos: &[Todo],
    todos_file: &std::path::Path,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        info!("Dry-run mode: skipping local TODO save after sync.");
        return Ok(());
    }
    store_todos(todos, todos_file)
}

fn save_todos_before_sync(todos: &[Todo], todos_file: &std::path::Path) -> Result<()> {
    store_todos(todos, todos_file)
}

#[derive(Parser)]
#[command(name = "juggler")]
#[command(about = "A TODO juggler TUI application")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Sync {
        #[command(subcommand)]
        service: SyncService,
    },
    Login {
        #[arg(long, default_value = "8080", help = "Local port for OAuth callback")]
        port: u16,
    },
    Logout,
}

#[derive(Subcommand)]
enum SyncService {
    #[command(name = "google-tasks")]
    GoogleTasks {
        #[arg(long, help = "Log actions without executing them")]
        dry_run: bool,
        #[arg(long, help = "Print keychain diagnostics for authentication")]
        debug_auth: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = Env::default().filter_or("RUST_LOG", "info");
    env_logger::Builder::from_env(env).init();

    let cli = Cli::parse();
    let todos_file = get_todos_file_path()?;

    let cred_store = KeyringCredentialStore::new();
    let http_client = reqwest::Client::new();

    match cli.command {
        Some(Commands::Login { port }) => {
            // OAuth browser login flow
            info!("Starting OAuth login flow...");

            match run_oauth_flow(GOOGLE_OAUTH_CLIENT_ID.to_string(), port).await {
                Ok(result) => {
                    println!("\n🎉 Authentication successful!");
                    match cred_store.store_refresh_token(&result.refresh_token) {
                        Ok(()) => {
                            println!(
                                "\nYour refresh token has been saved securely in your system keychain."
                            );
                            println!("You can now sync your TODOs with:");
                            println!();
                            println!("juggler sync google-tasks");
                            println!();
                            println!("Use --dry-run to preview changes:");
                            println!("juggler sync google-tasks --dry-run");
                        }
                        Err(e) => {
                            error!("Failed to store refresh token in keyring: {e}");
                            return Err(JugglerError::Credential(e));
                        }
                    }
                }
                Err(e) => {
                    error!("Authentication failed: {e}");
                    return Err(JugglerError::oauth(e.to_string()));
                }
            }
        }
        Some(Commands::Logout) => match cred_store.delete_refresh_token() {
            Ok(()) => {
                println!("Logged out: refresh token removed from keychain.");
            }
            Err(e) => {
                error!("Failed to delete refresh token from keychain: {e}");
                return Err(JugglerError::Credential(e));
            }
        },
        Some(Commands::Sync { service }) => {
            // CLI mode: handle sync commands
            match service {
                SyncService::GoogleTasks {
                    dry_run,
                    debug_auth,
                } => {
                    let mut todos = load_todos(&todos_file)?;

                    info!("Syncing TODOs with Google Tasks...");
                    if debug_auth {
                        info!("Auth diagnostics:");
                        info!("  platform: {}", std::env::consts::OS);
                        info!("  keychain service: {}", CREDENTIAL_KEYRING_SERVICE);
                        info!(
                            "  keychain account: {}",
                            CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS
                        );
                        match cred_store.get_refresh_token() {
                            Ok(t) => {
                                let len = t.len();
                                info!("  refresh token: [PRESENT] length={} chars", len);
                            }
                            Err(e) => {
                                error!("  refresh token: [ERROR] {}", e);
                            }
                        }
                    }

                    let oauth_client =
                        create_oauth_client_from_keychain(&cred_store, http_client.clone())?;

                    sync_to_tasks_with_oauth(&mut todos, oauth_client, dry_run).await?;

                    // Save the updated todos with new google_task_ids
                    if let Err(e) = maybe_persist_todos_after_sync(&todos, &todos_file, dry_run) {
                        error!("Warning: Failed to save todos after sync: {e}");
                        return Err(e);
                    }

                    info!("Sync completed successfully!");
                }
            }
        }
        None => {
            // TUI mode: original behavior
            let mut terminal = ratatui::init();
            let items = load_todos(&todos_file)?;
            let mut app = App::new(items, Box::new(ExternalEditor));
            let app_result = app.run(&mut terminal);
            ratatui::restore();

            if app.should_sync_on_exit() {
                let mut todos = app.items();

                // Always save local TODOs before attempting any sync. If the sync is slow
                // and the user kills the process or something, we want to make sure we don't
                // *locally* lose their changes.
                if let Err(e) = save_todos_before_sync(&todos, &todos_file) {
                    error!("Warning: Failed to save todos before sync: {e}");
                    return Err(e);
                }

                info!("Syncing TODOs with Google Tasks on exit...");

                match create_oauth_client_from_keychain(&cred_store, http_client) {
                    Ok(oauth_client) => {
                        let sync_result =
                            sync_to_tasks_with_oauth(&mut todos, oauth_client, false).await;
                        match sync_result {
                            Ok(()) => {
                                info!("Sync completed successfully!");
                                // Save again to persist any updated google_task_id values
                                if let Err(e) = store_todos(&todos, &todos_file) {
                                    error!("Warning: Failed to save todos after sync: {e}");
                                }
                            }
                            Err(e) => {
                                error!("Error syncing with Google Tasks: {e}");
                                // No additional save required here; we already saved before sync
                            }
                        }
                    }
                    Err(e) => {
                        error!("{}", e);
                        error!("Skipping sync. Todos were saved prior to sync attempt.");
                    }
                }
            } else if let Err(e) = store_todos(&app.items(), &todos_file) {
                error!("Warning: Failed to save todos: {e}");
            }

            return app_result;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_todo(title: &str) -> Todo {
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

    fn archive_file_count(dir: &std::path::Path) -> usize {
        fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with("TODOs_") && name.ends_with(".yaml")
            })
            .count()
    }

    #[test]
    fn dry_run_sync_does_not_rewrite_local_todos_or_create_archives() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let todos_file = temp_dir.path().join("TODOs.yaml");

        store_todos(&[make_todo("original")], &todos_file).expect("store initial todos");
        let before = fs::read_to_string(&todos_file).expect("read initial todos file");

        maybe_persist_todos_after_sync(&[make_todo("updated")], &todos_file, true)
            .expect("dry-run persist should succeed");

        let after = fs::read_to_string(&todos_file).expect("read todos file after dry-run");
        assert_eq!(before, after);
        assert_eq!(archive_file_count(temp_dir.path()), 0);
    }

    #[test]
    fn non_dry_run_sync_persists_local_todos_and_archives_previous_file() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let todos_file = temp_dir.path().join("TODOs.yaml");

        store_todos(&[make_todo("original")], &todos_file).expect("store initial todos");

        maybe_persist_todos_after_sync(&[make_todo("updated")], &todos_file, false)
            .expect("persist should succeed");

        let after = fs::read_to_string(&todos_file).expect("read updated todos file");
        assert!(after.contains("title: updated"));
        assert_eq!(archive_file_count(temp_dir.path()), 1);
    }

    #[test]
    fn save_todos_before_sync_persists_to_file_path() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let todos_file = temp_dir.path().join("TODOs.yaml");

        save_todos_before_sync(&[make_todo("saved-before-sync")], &todos_file)
            .expect("save should succeed");

        let content = fs::read_to_string(&todos_file).expect("read saved todos");
        assert!(content.contains("title: saved-before-sync"));
    }

    #[test]
    fn save_todos_before_sync_returns_error_for_directory_path() {
        let temp_dir = TempDir::new().expect("create temp dir");

        let result = save_todos_before_sync(&[make_todo("cannot-save")], temp_dir.path());
        assert!(result.is_err());
    }
}
