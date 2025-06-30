# juggler

A TODO juggler TUI application built with [Ratatui] that displays and manages TODO items from YAML files. Features a terminal user interface for managing tasks with due dates, comments, and Google Tasks synchronization.

[Ratatui]: https://ratatui.rs

## Features

- **Terminal User Interface**: Navigate and manage TODOs with keyboard shortcuts
- **YAML Storage**: TODOs stored in human-readable YAML format with comments and due dates
- **Due Date Support**: Automatic sorting with overdue items highlighted
- **External Editor Integration**: Edit TODOs in your preferred editor (via `$EDITOR`)
- **Google Tasks Sync**: One-way synchronization to Google Tasks (local YAML is authoritative)
- **Completion Tracking**: Mark items as done/undone
- **Snooze Functionality**: Postpone tasks by 1 day or 1 week

## Installation

```bash
cargo build --release
```

## Basic Usage

### Terminal UI Mode

Launch the interactive TUI (default behavior):

```bash
cargo run
# or
./target/release/juggler
```

**Keyboard Shortcuts:**
- `j/k` - Navigate up/down
- `o` - Toggle expand/collapse item
- `x` - Select/deselect item
- `e` - Toggle done status
- `E` - Edit item in external editor
- `s` - Snooze selected items by 1 day
- `S` - Snooze selected items by 1 week
- `q` - Quit and save

### Command Line Mode

View available commands:

```bash
juggler --help
juggler sync google-tasks --help
```

**Available sync options:**
- `--token <TOKEN>`: OAuth access token for Google Tasks API
- `--dry-run`: Log actions without executing them (safe testing mode)

## Google Tasks Synchronization

Juggler can synchronize your TODOs to Google Tasks, pushing your local YAML todos to Google's web/mobile interfaces. The local YAML file is the authoritative source - changes are pushed one-way to Google Tasks.

### Prerequisites

1. **Google Account**: You need a Google account with access to Google Tasks
2. **Juggler Task List**: Create a task list named "juggler" in Google Tasks

### Quick Setup

#### Create the "juggler" Task List

1. Open [Google Tasks](https://tasks.google.com/)
2. Create a new task list named exactly **"juggler"**
3. This is where all your TODO items will be synchronized

**That's it!** You can use the OAuth 2.0 Playground (see below) to get an access token without needing to set up your own Google Cloud Project.

### Getting Your Google Tasks Access Token

To sync with Google Tasks, you need to get an access token for your Google account. The easiest way is through your web browser:

#### Quick Token Access (Recommended)

1. Go to [Google OAuth 2.0 Playground](https://developers.google.com/oauthplayground/)
2. In the left panel, find **"Tasks API v1"** and expand it
3. Check the box for: `https://www.googleapis.com/auth/tasks`
4. Click **"Authorize APIs"**
5. Sign in to your Google account when prompted
6. Click **"Allow"** to grant access to your Google Tasks
7. Click **"Exchange authorization code for tokens"**
8. Copy the **"Access token"** value (starts with `ya29.`)

**Note:** This token expires in about 1 hour. For longer use, you can click "Refresh the token" to get a new one.

#### Using Your Own OAuth App (Optional)

If you prefer to use your own OAuth credentials instead of the playground's default ones:

1. Set up a Google Cloud Project:
   - Go to [Google Cloud Console](https://console.cloud.google.com/)
   - Create a new project and enable the "Tasks API"
   - Create OAuth 2.0 credentials (Desktop application type)
2. In the OAuth Playground:
   - Click the gear icon (⚙️) in the top right
   - Check **"Use your own OAuth credentials"**
   - Enter your **Client ID** and **Client Secret**
   - Follow steps 2-8 from the Quick Token Access method above

### Synchronizing TODOs

Once you have an access token, synchronize your TODOs with Google Tasks:

```bash
# Sync with Google Tasks
juggler sync google-tasks --token "YOUR_ACCESS_TOKEN_HERE"
```

**Example:**
```bash
juggler sync google-tasks --token "ya29.a0AfH6SMBxxxxx..."
```

#### Dry-Run Mode

Test your sync operations without making actual changes:

```bash
# Dry-run mode with logging
RUST_LOG=info juggler sync google-tasks --token "YOUR_TOKEN" --dry-run
```

In dry-run mode, all API actions are logged but not executed. This allows you to:
- Test your configuration safely
- Preview what changes would be made
- Debug sync issues without affecting your Google Tasks

#### Logging Configuration

Juggler uses Rust's standard logging infrastructure. Control logging output with the `RUST_LOG` environment variable:

**Basic logging (recommended):**
```bash
RUST_LOG=info juggler sync google-tasks --token "YOUR_TOKEN"
```

**Debug logging (verbose):**
```bash
RUST_LOG=debug juggler sync google-tasks --token "YOUR_TOKEN"
```

**Silent mode (errors only):**
```bash
RUST_LOG=error juggler sync google-tasks --token "YOUR_TOKEN"
```

**Log output includes:**
- Sync start/completion messages
- Task creation, updates, and deletions
- Clear `[DRY RUN]` prefixes when using `--dry-run`
- Error details for troubleshooting

### How Synchronization Works

The sync process pushes your local TODOs to Google Tasks (one-way sync):

1. **Creates new tasks** in Google Tasks for local TODOs without `google_task_id`
2. **Updates existing tasks** when title, notes, completion status, or due date changes in the local YAML
3. **Deletes orphaned tasks** in Google Tasks that no longer exist in your local YAML
4. **Maps task properties** from local to Google Tasks:
   - TODO `title` → Google Task `title`
   - TODO `comment` → Google Task `notes`
   - TODO `done` → Google Task `status` (completed/needsAction)
   - TODO `due_date` → Google Task `due`

After sync, each TODO item gets a `google_task_id` field linking it to the corresponding Google Task.

**Important**: Changes made directly in Google Tasks will be **overwritten** on the next sync. Always edit your TODOs in the local YAML file or through the juggler TUI.

### Security Notes

- **Never commit access tokens** to version control
- **Access tokens expire** (typically 1 hour) and need to be refreshed
- **Use refresh tokens** for production applications
- **Store tokens securely** and consider using environment variables

### Troubleshooting

**"No 'juggler' task list found"**
- Create a task list named exactly **"juggler"** in [Google Tasks](https://tasks.google.com/)
- Make sure you're signed into the same Google account you used to get the token

**"Invalid token" or authentication errors**
- Your access token has expired (they last ~1 hour) - get a new one from the [OAuth Playground](https://developers.google.com/oauthplayground/)
- Make sure you selected the `https://www.googleapis.com/auth/tasks` scope when getting your token
- Check that you copied the complete token (starts with `ya29.`)

**Tasks not syncing properly**
- Check that your local TODOs.yaml file is valid YAML
- Use `--dry-run` mode first to see what would happen: `RUST_LOG=info juggler sync google-tasks --token "YOUR_TOKEN" --dry-run`
- Try removing `google_task_id` fields from your YAML to force re-creation of tasks

## Data Format

TODOs are stored in `TODOs.yaml` with the following structure:

```yaml
- title: "Buy groceries"
  comment: |
    - Milk
    - Bread
    - Eggs
  done: false
  due_date: "2025-01-07T09:00:00Z"  # ISO 8601 format
  google_task_id: "task_abc123"     # Set after sync
- title: "Completed task"
  comment: null
  done: true
  due_date: null
  google_task_id: "task_def456"
```

## License

Copyright (c) Peter Schuller <peter.schuller@infidyne.com>

This project is licensed under the MIT license ([LICENSE] or <http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
