use std::io;

mod store;
mod ui;

use clap::{Parser, Subcommand};
use store::{load_todos, store_todos, sync_to_tasks};
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
        #[arg(long, help = "OAuth access token for Google Tasks API")]
        token: String,
        #[arg(long, help = "Log actions without executing them")]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    let todos_file = "TODOs.yaml";

    match cli.command {
        Some(Commands::Sync { service }) => {
            // CLI mode: handle sync commands
            match service {
                SyncService::GoogleTasks { token, dry_run } => {
                    let mut todos = load_todos(todos_file)?;

                    println!("Syncing TODOs with Google Tasks...");

                    if let Err(e) = sync_to_tasks(&mut todos, &token, dry_run).await {
                        eprintln!("Error syncing with Google Tasks: {e}");
                        return Err(io::Error::other(e.to_string()));
                    }

                    // Save the updated todos with new google_task_ids
                    if let Err(e) = store_todos(&todos, todos_file) {
                        eprintln!("Warning: Failed to save todos after sync: {e}");
                        return Err(e);
                    }

                    println!("Sync completed successfully!");
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
                eprintln!("Warning: Failed to save todos: {e}");
            }

            return app_result;
        }
    }

    Ok(())
}
