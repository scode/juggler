use env_logger::Builder;
use log::LevelFilter;
use std::io;

use log::{error, info};

mod store;
mod ui;
mod auth;

use clap::{Parser, Subcommand};
use store::{load_todos, store_todos, sync_to_tasks_with_oauth};
use ui::{App, ExternalEditor};

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
}

#[derive(Subcommand)]
enum SyncService {
    #[command(name = "google-tasks")]
    GoogleTasks {
        #[arg(long, help = "OAuth refresh token for Google Tasks API")]
        refresh_token: String,
        #[arg(long, help = "OAuth client ID")]
        client_id: String,
        #[arg(long, help = "OAuth client secret")]
        client_secret: String,
        #[arg(long, help = "Log actions without executing them")]
        dry_run: bool,
    },
    #[command(name = "google-tasks-legacy")]
    GoogleTasksLegacy {
        #[arg(long, help = "OAuth access token for Google Tasks API (legacy - will expire in ~1 hour)")]
        token: String,
        #[arg(long, help = "Log actions without executing them")]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut builder = Builder::from_default_env();
    builder.filter(None, LevelFilter::Info).init();

    let cli = Cli::parse();
    let todos_file = "TODOs.yaml";

    match cli.command {
        Some(Commands::Sync { service }) => {
            // CLI mode: handle sync commands
            match service {
                SyncService::GoogleTasks { refresh_token, client_id, client_secret, dry_run } => {
                    let mut todos = load_todos(todos_file)?;

                    info!("Syncing TODOs with Google Tasks using refresh token...");

                    let oauth_config = auth::GoogleOAuthConfig::new(client_id, client_secret, refresh_token);

                    if let Err(e) = sync_to_tasks_with_oauth(&mut todos, &oauth_config, dry_run).await {
                        error!("Error syncing with Google Tasks: {e}");
                        return Err(io::Error::other(e.to_string()));
                    }

                    // Save the updated todos with new google_task_ids
                    if let Err(e) = store_todos(&todos, todos_file) {
                        error!("Warning: Failed to save todos after sync: {e}");
                        return Err(e);
                    }

                    info!("Sync completed successfully!");
                }
                SyncService::GoogleTasksLegacy { token, dry_run } => {
                    let mut todos = load_todos(todos_file)?;

                    info!("Syncing TODOs with Google Tasks using legacy access token...");
                    info!("Warning: Access tokens expire in ~1 hour. Consider using refresh tokens instead.");

                    if let Err(e) = store::sync_to_tasks(&mut todos, &token, dry_run).await {
                        error!("Error syncing with Google Tasks: {e}");
                        return Err(io::Error::other(e.to_string()));
                    }

                    // Save the updated todos with new google_task_ids
                    if let Err(e) = store_todos(&todos, todos_file) {
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
            let items = load_todos(todos_file)?;
            let mut app = App::new(items, ExternalEditor);
            let app_result = app.run(&mut terminal);
            ratatui::restore();

            // Save todos when exiting
            if let Err(e) = store_todos(app.items(), todos_file) {
                error!("Warning: Failed to save todos: {e}");
            }

            return app_result;
        }
    }

    Ok(())
}
