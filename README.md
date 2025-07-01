# juggler

A TODO juggler TUI application built with [Ratatui] that displays and manages TODO items from YAML files. Features a terminal user interface for managing tasks with due dates, comments, and Google Tasks synchronization.

[Ratatui]: https://ratatui.rs

## Features

- **Terminal User Interface**: Navigate and manage TODOs with keyboard shortcuts
- **YAML Storage**: TODOs stored in human-readable YAML format with comments and due dates
- **Due Date Support**: Automatic sorting with overdue items highlighted
- **External Editor Integration**: Edit TODOs in your preferred editor (via `$EDITOR`)
- **Google Tasks Sync**: One-way synchronization to Google Tasks (local YAML is authoritative)
- **Refresh Token Support**: Long-lived authentication with automatic token refresh
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

**Refresh Token Method (Recommended):**
- `--refresh-token <TOKEN>`: OAuth refresh token for Google Tasks API
- `--client-id <CLIENT_ID>`: OAuth client ID
- `--client-secret <CLIENT_SECRET>`: OAuth client secret
- `--dry-run`: Log actions without executing them (safe testing mode)

**Legacy Access Token Method:**
- `--token <TOKEN>`: OAuth access token for Google Tasks API (expires in ~1 hour)
- `--dry-run`: Log actions without executing them (safe testing mode)

## Google Tasks Synchronization

Juggler can synchronize your TODOs to Google Tasks, pushing your local YAML todos to Google's web/mobile interfaces. The local YAML file is the authoritative source - changes are pushed one-way to Google Tasks.

### Prerequisites

1. **Google Account**: You need a Google account with access to Google Tasks
2. **Juggler Task List**: Create a task list named "juggler" in Google Tasks
3. **OAuth Credentials**: Set up OAuth credentials for long-lived access

### Quick Setup

#### Create the "juggler" Task List

1. Open [Google Tasks](https://tasks.google.com/)
2. Create a new task list named exactly **"juggler"**
3. This is where all your TODO items will be synchronized

### Authentication Setup

#### Method 1: Refresh Token (Recommended)

For production use and long-term automation, use refresh tokens which don't expire:

**Step 1: Set up Google OAuth Application**

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project or select an existing one
3. Enable the "Google Tasks API"
4. Go to "Credentials" → "Create Credentials" → "OAuth 2.0 Client IDs"
5. Choose "Desktop application" as the application type
6. Note down your **Client ID** and **Client Secret**

**Step 2: Get Your Refresh Token**

1. Go to [Google OAuth 2.0 Playground](https://developers.google.com/oauthplayground/)
2. Click the gear icon (⚙️) in the top right
3. Check **"Use your own OAuth credentials"**
4. Enter your **Client ID** and **Client Secret** from Step 1
5. In the left panel, find **"Tasks API v1"** and expand it
6. Check the box for: `https://www.googleapis.com/auth/tasks`
7. Click **"Authorize APIs"**
8. Sign in to your Google account when prompted
9. Click **"Allow"** to grant access to your Google Tasks
10. Click **"Exchange authorization code for tokens"**
11. Copy the **"Refresh token"** value (starts with `1//`)

**Step 3: Sync with Refresh Token**

```bash
# Sync using refresh token (recommended)
juggler sync google-tasks \
  --refresh-token "1//your_refresh_token_here" \
  --client-id "your_client_id.apps.googleusercontent.com" \
  --client-secret "your_client_secret"
```

**Example:**
```bash
juggler sync google-tasks \
  --refresh-token "1//04xxxxx-xxxxxxxxxxxxxxxxxxxxxxxxx" \
  --client-id "123456789-abcdefghijklmnop.apps.googleusercontent.com" \
  --client-secret "GOCSPX-xxxxxxxxxxxxxxxxxxxxxxxx"
```

#### Method 2: Access Token (Legacy - Quick Testing)

For quick testing or one-time use, you can use short-lived access tokens:

1. Go to [Google OAuth 2.0 Playground](https://developers.google.com/oauthplayground/)
2. In the left panel, find **"Tasks API v1"** and expand it
3. Check the box for: `https://www.googleapis.com/auth/tasks`
4. Click **"Authorize APIs"**
5. Sign in to your Google account when prompted
6. Click **"Allow"** to grant access to your Google Tasks
7. Click **"Exchange authorization code for tokens"**
8. Copy the **"Access token"** value (starts with `ya29.`)

```bash
# Legacy method using access token (expires in ~1 hour)
juggler sync google-tasks-legacy --token "ya29.your_access_token_here"
```

**Note:** Access tokens expire in about 1 hour and need to be refreshed manually.

### Synchronizing TODOs

Once you have your credentials set up, synchronize your TODOs with Google Tasks:

```bash
# Using refresh token (recommended)
juggler sync google-tasks \
  --refresh-token "YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID" \
  --client-secret "YOUR_CLIENT_SECRET"

# Using legacy access token
juggler sync google-tasks-legacy --token "YOUR_ACCESS_TOKEN"
```

#### Dry-Run Mode

Test your sync operations without making actual changes:

```bash
# Dry-run mode with logging
RUST_LOG=info juggler sync google-tasks \
  --refresh-token "YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID" \
  --client-secret "YOUR_CLIENT_SECRET" \
  --dry-run
```

In dry-run mode, all API actions are logged but not executed. This allows you to:
- Test your configuration safely
- Preview what changes would be made
- Debug sync issues without affecting your Google Tasks

#### Logging Configuration

Juggler uses Rust's standard logging infrastructure. Control logging output with the `RUST_LOG` environment variable:

**Basic logging (recommended):**
```bash
RUST_LOG=info juggler sync google-tasks --refresh-token "..." --client-id "..." --client-secret "..."
```

**Debug logging (verbose):**
```bash
RUST_LOG=debug juggler sync google-tasks --refresh-token "..." --client-id "..." --client-secret "..."
```

**Silent mode (errors only):**
```bash
RUST_LOG=error juggler sync google-tasks --refresh-token "..." --client-id "..." --client-secret "..."
```

**Log output includes:**
- Sync start/completion messages
- Token refresh operations
- Task creation, updates, and deletions
- Clear `[DRY RUN]` prefixes when using `--dry-run`
- Error details for troubleshooting

### How Synchronization Works

The sync process pushes your local TODOs to Google Tasks (one-way sync):

1. **Refreshes access token** automatically using your refresh token (when using refresh token method)
2. **Creates new tasks** in Google Tasks for local TODOs without `google_task_id`
3. **Updates existing tasks** when title, notes, completion status, or due date changes in the local YAML
4. **Deletes orphaned tasks** in Google Tasks that no longer exist in your local YAML
5. **Maps task properties** from local to Google Tasks:
   - TODO `title` → Google Task `title` (prefixed with "j:")
   - TODO `comment` → Google Task `notes`
   - TODO `done` → Google Task `status` (completed/needsAction)
   - TODO `due_date` → Google Task `due`

After sync, each TODO item gets a `google_task_id` field linking it to the corresponding Google Task.

**Important**: Changes made directly in Google Tasks will be **overwritten** on the next sync. Always edit your TODOs in the local YAML file or through the juggler TUI.

### Security Notes

- **Never commit credentials** to version control
- **Refresh tokens don't expire** but can be revoked by the user
- **Access tokens expire** (typically 1 hour) and need to be refreshed
- **Store credentials securely** and consider using environment variables
- **Use refresh tokens** for production applications and automation

### Troubleshooting

**"No 'juggler' task list found"**
- Create a task list named exactly **"juggler"** in [Google Tasks](https://tasks.google.com/)
- Make sure you're signed into the same Google account you used to get the credentials

**"Invalid token" or authentication errors**
- For access tokens: Your token has expired (they last ~1 hour) - get a new one from the [OAuth Playground](https://developers.google.com/oauthplayground/)
- For refresh tokens: Check that your client ID and client secret are correct
- Make sure you selected the `https://www.googleapis.com/auth/tasks` scope when getting your token
- Check that you copied the complete token

**"OAuth error: invalid_grant"**
- Your refresh token may have expired or been revoked
- Re-authorize your application through the OAuth Playground to get a new refresh token

**Tasks not syncing properly**
- Check that your local TODOs.yaml file is valid YAML
- Use `--dry-run` mode first to see what would happen: `RUST_LOG=info juggler sync google-tasks --refresh-token "..." --client-id "..." --client-secret "..." --dry-run`
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
