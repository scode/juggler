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
- `--refresh-token <REFRESH_TOKEN>`: OAuth refresh token (recommended)
- `--client-id <CLIENT_ID>`: OAuth client ID (required with refresh token)
- `--client-secret <CLIENT_SECRET>`: OAuth client secret (required with refresh token)
- `--token <TOKEN>`: OAuth access token (deprecated, expires quickly)
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

**That's it!** For persistent use, set up OAuth refresh tokens (see below). For quick testing, you can use the OAuth 2.0 Playground to get a short-lived access token.

### Getting Your Google OAuth Credentials

To sync with Google Tasks, you need OAuth credentials from Google. There are two approaches: using refresh tokens (recommended for persistent use) or access tokens (quick but expires in 1 hour).

#### Method 1: OAuth Refresh Token (Recommended)

Refresh tokens provide long-term access without needing to re-authenticate frequently. You'll need to set up your own Google Cloud Project for this:

##### Step 1: Set up Google Cloud Project

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project (or use an existing one)
3. Enable the **Tasks API**:
   - Go to "APIs & Services" → "Library"
   - Search for "Tasks API" and enable it
4. Create OAuth 2.0 credentials:
   - Go to "APIs & Services" → "Credentials"
   - Click "Create Credentials" → "OAuth 2.0 Client IDs"
   - Choose "Desktop application" as the application type
   - Name it something like "Juggler TODO App"
   - Download the credentials JSON file

##### Step 2: Get Your Refresh Token

1. Go to [Google OAuth 2.0 Playground](https://developers.google.com/oauthplayground/)
2. Click the gear icon (⚙️) in the top right corner
3. Check **"Use your own OAuth credentials"**
4. Enter your **Client ID** and **Client Secret** from the credentials you just created
5. In the left panel, find **"Tasks API v1"** and expand it
6. Check the box for: `https://www.googleapis.com/auth/tasks`
7. Click **"Authorize APIs"**
8. Sign in to your Google account when prompted and click **"Allow"**
9. Click **"Exchange authorization code for tokens"**
10. Copy these three values:
    - **Refresh token** (starts with `1//`)
    - **Client ID** (from your Google Cloud project)
    - **Client Secret** (from your Google Cloud project)

##### Step 3: Sync with Refresh Token

```bash
# Sync using OAuth refresh token (recommended)
juggler sync google-tasks \
  --refresh-token "1//YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID.apps.googleusercontent.com" \
  --client-secret "YOUR_CLIENT_SECRET"
```

**Benefits of refresh tokens:**
- No need to re-authenticate frequently
- Automatic token refresh when access tokens expire
- Secure long-term access

#### Method 2: Quick Access Token (Legacy)

For quick testing or one-time use, you can get a short-lived access token:

1. Go to [Google OAuth 2.0 Playground](https://developers.google.com/oauthplayground/)
2. In the left panel, find **"Tasks API v1"** and expand it
3. Check the box for: `https://www.googleapis.com/auth/tasks`
4. Click **"Authorize APIs"**
5. Sign in to your Google account when prompted
6. Click **"Allow"** to grant access to your Google Tasks
7. Click **"Exchange authorization code for tokens"**
8. Copy the **"Access token"** value (starts with `ya29.`)

**Note:** This token expires in about 1 hour and is only recommended for quick testing.

### Synchronizing TODOs

Once you have your OAuth credentials, synchronize your TODOs with Google Tasks:

#### Using Refresh Token (Recommended)

```bash
# Sync with Google Tasks using refresh token
juggler sync google-tasks \
  --refresh-token "1//YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID.apps.googleusercontent.com" \
  --client-secret "YOUR_CLIENT_SECRET"
```

**Example:**
```bash
juggler sync google-tasks \
  --refresh-token "1//04xxxxx-xxxxxxxxxx" \
  --client-id "123456789.apps.googleusercontent.com" \
  --client-secret "GOCSPX-xxxxxxxxxxxxxxx"
```

#### Using Access Token (Legacy)

```bash
# Sync with Google Tasks using access token (deprecated)
juggler sync google-tasks --token "YOUR_ACCESS_TOKEN_HERE"
```

**Example:**
```bash
juggler sync google-tasks --token "ya29.a0AfH6SMBxxxxx..."
```

#### Dry-Run Mode

Test your sync operations without making actual changes:

```bash
# Dry-run mode with refresh token (recommended)
RUST_LOG=info juggler sync google-tasks \
  --refresh-token "1//YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID" \
  --client-secret "YOUR_CLIENT_SECRET" \
  --dry-run

# Dry-run mode with access token (legacy)
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
# With refresh token
RUST_LOG=info juggler sync google-tasks \
  --refresh-token "YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID" \
  --client-secret "YOUR_CLIENT_SECRET"

# With access token (legacy)
RUST_LOG=info juggler sync google-tasks --token "YOUR_TOKEN"
```

**Debug logging (verbose):**
```bash
# With refresh token
RUST_LOG=debug juggler sync google-tasks \
  --refresh-token "YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID" \
  --client-secret "YOUR_CLIENT_SECRET"
```

**Silent mode (errors only):**
```bash
# With refresh token
RUST_LOG=error juggler sync google-tasks \
  --refresh-token "YOUR_REFRESH_TOKEN" \
  --client-id "YOUR_CLIENT_ID" \
  --client-secret "YOUR_CLIENT_SECRET"
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

- **Never commit OAuth credentials** to version control
- **Use refresh tokens** for persistent access (recommended approach)
- **Access tokens expire** (typically 1 hour) but refresh tokens provide long-term access
- **Store credentials securely** and consider using environment variables:
  ```bash
  export JUGGLER_REFRESH_TOKEN="1//YOUR_REFRESH_TOKEN"
  export JUGGLER_CLIENT_ID="YOUR_CLIENT_ID"
  export JUGGLER_CLIENT_SECRET="YOUR_CLIENT_SECRET"
  
  juggler sync google-tasks \
    --refresh-token "$JUGGLER_REFRESH_TOKEN" \
    --client-id "$JUGGLER_CLIENT_ID" \
    --client-secret "$JUGGLER_CLIENT_SECRET"
  ```

### Troubleshooting

**"No 'juggler' task list found"**
- Create a task list named exactly **"juggler"** in [Google Tasks](https://tasks.google.com/)
- Make sure you're signed into the same Google account you used to get the token

**"Invalid token" or authentication errors**
- **Using refresh token**: Your OAuth credentials may be invalid or expired. Verify your client ID, client secret, and refresh token
- **Using access token**: Your access token has expired (they last ~1 hour) - get a new one from the [OAuth Playground](https://developers.google.com/oauthplayground/)
- Make sure you selected the `https://www.googleapis.com/auth/tasks` scope when getting your credentials
- For refresh tokens, check that you copied all three values: refresh token (starts with `1//`), client ID, and client secret

**Tasks not syncing properly**
- Check that your local TODOs.yaml file is valid YAML
- Use `--dry-run` mode first to see what would happen:
  ```bash
  # With refresh token
  RUST_LOG=info juggler sync google-tasks \
    --refresh-token "YOUR_REFRESH_TOKEN" \
    --client-id "YOUR_CLIENT_ID" \
    --client-secret "YOUR_CLIENT_SECRET" \
    --dry-run
  
  # With access token (legacy)
  RUST_LOG=info juggler sync google-tasks --token "YOUR_TOKEN" --dry-run
  ```
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
