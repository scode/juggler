use env_logger::Builder;
use log::LevelFilter;
use std::io;

use log::{error, info};

mod store;
mod ui;

use clap::{Parser, Subcommand};
use store::{
    GoogleOAuthClient, GoogleOAuthCredentials, load_todos, store_todos, sync_to_tasks,
    sync_to_tasks_with_oauth,
};
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
        #[arg(
            long,
            help = "OAuth access token for Google Tasks API (deprecated, use --refresh-token instead)"
        )]
        token: Option<String>,
        #[arg(long, help = "OAuth refresh token for Google Tasks API")]
        refresh_token: Option<String>,
        #[arg(long, help = "OAuth client ID for Google Tasks API")]
        client_id: Option<String>,
        #[arg(long, help = "OAuth client secret for Google Tasks API")]
        client_secret: Option<String>,
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
                SyncService::GoogleTasks {
                    token,
                    refresh_token,
                    client_id,
                    client_secret,
                    dry_run,
                } => {
                    let mut todos = load_todos(todos_file)?;

                    info!("Syncing TODOs with Google Tasks...");

                    // Determine authentication method
                    match (token, refresh_token, client_id, client_secret) {
                        // Legacy bearer token authentication
                        (Some(token), None, None, None) => {
                            info!("Using bearer token authentication (deprecated)");
                            if let Err(e) = sync_to_tasks(&mut todos, &token, dry_run).await {
                                error!("Error syncing with Google Tasks: {e}");
                                return Err(io::Error::other(e.to_string()));
                            }
                        }
                        // OAuth refresh token authentication
                        (None, Some(refresh_token), Some(client_id), Some(client_secret)) => {
                            info!("Using OAuth refresh token authentication");
                            let credentials = GoogleOAuthCredentials {
                                client_id,
                                client_secret,
                                refresh_token,
                            };
                            let oauth_client = GoogleOAuthClient::new(credentials);

                            if let Err(e) =
                                sync_to_tasks_with_oauth(&mut todos, oauth_client, dry_run).await
                            {
                                error!("Error syncing with Google Tasks: {e}");
                                return Err(io::Error::other(e.to_string()));
                            }
                        }
                        // Invalid combination of arguments
                        _ => {
                            error!("Invalid authentication configuration. Either provide:");
                            error!("  --token <TOKEN> (deprecated)");
                            error!("  OR");
                            error!(
                                "  --refresh-token <REFRESH_TOKEN> --client-id <CLIENT_ID> --client-secret <CLIENT_SECRET>"
                            );
                            return Err(io::Error::other("Invalid authentication configuration"));
                        }
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
