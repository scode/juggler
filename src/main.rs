use env_logger::Builder;
use log::LevelFilter;
use std::io;

use log::{error, info};

mod config;
mod credential_storage;
mod google_tasks;
mod oauth;
mod store;
mod time;
mod ui;

use clap::{Parser, Subcommand};
use config::{GOOGLE_OAUTH_CLIENT_ID, get_todos_file_path};
use credential_storage::{
    CredentialStore, KEYRING_ACCOUNT_GOOGLE_TASKS, KEYRING_SERVICE, KeyringCredentialStore,
};
use google_tasks::{GoogleOAuthClient, GoogleOAuthCredentials, sync_to_tasks_with_oauth};
use oauth::run_oauth_flow;
use store::{load_todos, store_todos};
use ui::{App, ExternalEditor};

fn create_oauth_client_from_keychain(
    cred_store: &dyn CredentialStore,
) -> Result<GoogleOAuthClient, io::Error> {
    let refresh_token = cred_store.get_refresh_token().map_err(|_| {
        io::Error::other("No refresh token found in keychain. Run `juggler login` to authenticate.")
    })?;

    let credentials = GoogleOAuthCredentials {
        client_id: GOOGLE_OAUTH_CLIENT_ID.to_string(),
        refresh_token,
    };

    Ok(GoogleOAuthClient::new(credentials))
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
async fn main() -> io::Result<()> {
    let mut builder = Builder::from_default_env();
    builder.filter(None, LevelFilter::Info).init();

    let cli = Cli::parse();
    let todos_file = get_todos_file_path()?;

    let cred_store = KeyringCredentialStore::new();

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
                            return Err(io::Error::other(
                                "Failed to store refresh token in keyring",
                            ));
                        }
                    }
                }
                Err(e) => {
                    error!("Authentication failed: {e}");
                    return Err(io::Error::other(e.to_string()));
                }
            }
        }
        Some(Commands::Logout) => match cred_store.delete_refresh_token() {
            Ok(()) => {
                println!("Logged out: refresh token removed from keychain.");
            }
            Err(e) => {
                error!("Failed to delete refresh token from keychain: {e}");
                return Err(io::Error::other(
                    "Failed to delete refresh token from keychain",
                ));
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
                        info!("  keychain service: {}", KEYRING_SERVICE);
                        info!("  keychain account: {}", KEYRING_ACCOUNT_GOOGLE_TASKS);
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

                    let oauth_client = match create_oauth_client_from_keychain(&cred_store) {
                        Ok(client) => client,
                        Err(e) => {
                            error!("{}", e);
                            return Err(e);
                        }
                    };

                    if let Err(e) =
                        sync_to_tasks_with_oauth(&mut todos, oauth_client, dry_run).await
                    {
                        error!("Error syncing with Google Tasks: {e}");
                        return Err(io::Error::other(e.to_string()));
                    }

                    // Save the updated todos with new google_task_ids
                    if let Err(e) = store_todos(&todos, &todos_file) {
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
            let mut app = App::new(items, ExternalEditor);
            let app_result = app.run(&mut terminal);
            ratatui::restore();

            if app.should_sync_on_exit() {
                // Always save local TODOs before attempting any sync. If the sync is slow
                // and the user kills the process or something, we want to make sure we don't
                // *locally* lose their changes.
                if let Err(e) = store_todos(app.items(), &todos_file) {
                    error!("Warning: Failed to save todos before sync: {e}");
                }

                info!("Syncing TODOs with Google Tasks on exit...");

                match create_oauth_client_from_keychain(&cred_store) {
                    Ok(oauth_client) => {
                        let mut todos: Vec<_> = app.items().to_vec();

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
            } else if let Err(e) = store_todos(app.items(), &todos_file) {
                error!("Warning: Failed to save todos: {e}");
            }

            return app_result;
        }
    }

    Ok(())
}
