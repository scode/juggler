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
- **Snooze/Prepone**: Quickly adjust due dates by ±1 day or ±7 days, plus custom delays

## Quick Start

1. **Build the application:**
   ```bash
   cargo build --release
   ```

2. **Create a "juggler" task list in [Google Tasks](https://tasks.google.com/)**

3. **Set up Google OAuth credentials** (see detailed instructions below)

4. **Authenticate with Google:**
   ```bash
   ./target/release/juggler login
   ```

5. **Sync your TODOs:**
   ```bash
   ./target/release/juggler sync google-tasks
   ```
6. Optional: **Log out** by removing the stored refresh token:
   ```bash
   ./target/release/juggler logout
   ```

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
- `j/k` - Move cursor down/up
- `o` - Toggle expand/collapse on the cursored item
- `x` - Select/deselect the cursored item
- `e` - Toggle done on selected items; if none selected, acts on the cursored item
- `E` - Edit the cursored item in external editor
- `s` - Snooze selected items by 1 day; if none selected, snooze the cursored item
- `S` - Unsnooze (minus 1 day) for selected items; if none selected, unsnooze the cursored item
- `p` - Snooze by 7 days for selected items; if none selected, snooze the cursored item
- `P` - Prepone by 7 days for selected items; if none selected, prepone the cursored item
- `t` - Custom delay prompt (e.g., 5d, -2h)
- `q` - Quit and save

Note: Actions operate on all selected items. If no items are selected, they apply to the item under the cursor.

### Command Line Mode

View available commands:

```bash
juggler --help
juggler login --help
juggler logout --help
juggler sync google-tasks --help
```

**Available commands:**
- `juggler` - Launch interactive TUI mode
- `juggler login` - Browser-based OAuth authentication
- `juggler sync google-tasks` - Sync TODOs with Google Tasks

**Login options:**
- `--port <PORT>`: Local callback port (default: 8080)

**Sync options:**
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

**That's it!** Now you can authenticate easily with `juggler login` (see below) or manually set up OAuth credentials.

### Getting Your Google OAuth Credentials

To sync with Google Tasks, you need OAuth credentials from Google. There are two approaches: using refresh tokens (recommended for persistent use) or access tokens (quick but expires in 1 hour).

#### Method 1: Browser Login (Recommended)

The simplest way to authenticate is using the built-in browser login flow:

##### Step 1: Run Browser Authentication

```bash
# Launch browser-based authentication
juggler login
```

This will:
1. Start a local web server on port 8080 (customizable with `--port`)
2. Open your default browser to Google's authentication page
3. Guide you through the OAuth consent flow
4. Display the credentials you need for syncing

**Example output:**
```bash
🎉 Authentication successful!

You can now sync your TODOs with Google Tasks using:

juggler sync google-tasks
```

**Benefits:**
- Secure PKCE OAuth flow
- Automatic browser integration
- One-time setup per machine
- Long-term refresh tokens
- No Google Cloud Console setup required

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

#### Sync

```bash
# Sync your TODOs with Google Tasks (uses refresh token from your system keychain)
juggler sync google-tasks
```

#### Using Access Token (Legacy)

```bash
# Legacy access token flow has been removed. Please use:
juggler sync google-tasks
```

**Example:**
```bash
juggler sync google-tasks
```

#### Dry-Run Mode

Test your sync operations without making actual changes:

```bash
# Dry-run mode
RUST_LOG=info juggler sync google-tasks --dry-run
```

In dry-run mode, all API actions are logged but not executed. This allows you to:
- Test your configuration safely
- Preview what changes would be made
- Debug sync issues without affecting your Google Tasks

#### Logging Configuration

Juggler uses Rust's standard logging infrastructure. Control logging output with the `RUST_LOG` environment variable:

**Basic logging (recommended):**
```bash
RUST_LOG=info juggler sync google-tasks
```

**Debug logging (verbose):**
```bash
RUST_LOG=debug juggler sync google-tasks
```

**Silent mode (errors only):**
```bash
RUST_LOG=error juggler sync google-tasks
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

## Limitations

- **Google Tasks due time precision**: The Google Tasks API stores `due` as a date-only field. The time component is discarded when setting or reading via the public API. The UI may display a time, but that precision is not exposed through the public API. See the official docs: https://developers.google.com/workspace/tasks/reference/rest/v1/tasks (field `due`).
  - Impact in juggler during task syncing: We normalize outgoing due dates to midnight UTC (00:00:00Z) and compare by calendar day with a very small tolerance to avoid spurious updates.

## Security Notes

- **Never commit OAuth credentials** to version control
- **Use refresh tokens** for persistent access (recommended approach)
- **Access tokens expire** (typically 1 hour) but refresh tokens provide long-term access
- **Storage**: Refresh tokens are stored securely in your system keychain via the keyring crate after running `juggler login`. To refresh or reset, run `juggler login` again.

### Troubleshooting

**"No 'juggler' task list found"**
- Create a task list named exactly **"juggler"** in [Google Tasks](https://tasks.google.com/)
- Make sure you're signed into the same Google account you used to get the token

**"Invalid token" or authentication errors**
- If you see authentication errors, run `juggler login` again to refresh the stored credentials. You can also revoke the app's access at https://myaccount.google.com/permissions and then run `juggler login` again.
- Make sure you selected the `https://www.googleapis.com/auth/tasks` scope when getting your credentials
- For refresh tokens, check that you copied the refresh token value (starts with `1//`)

**"Error 401: invalid_client" or authentication errors during login**
- This should not occur with the latest version as the OAuth client is properly configured
- If you encounter persistent errors, please file an issue at the project repository

**Tasks not syncing properly**
- Check that your local TODOs.yaml file is valid YAML
- Use `--dry-run` mode first to see what would happen:
  ```bash
  RUST_LOG=info juggler sync google-tasks --dry-run
  ```
- Try removing `google_task_id` fields from your YAML to force re-creation of tasks

## Data Format

By default, TODOs are stored at `~/.juggler/TODOs.yaml`. Each save creates a timestamped backup of the previous file in the same directory (e.g., `TODOs_2025-01-07T09-00-00.yaml`).

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
