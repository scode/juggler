# Google Tasks Synchronization (Bare Bones)

Google Tasks sync currently exists in a very manual, bare-bones form.

The sync direction is one-way (`local TOML -> Google Tasks`). Your local `TODOs.toml` remains authoritative.

## Manual Setup Required

Before login/sync will work for your own environment, you must configure your own OAuth desktop client values in source code and rebuild.

1. Create an OAuth desktop client in Google Cloud Console.
2. Update these constants in `src/config.rs`:
   - `GOOGLE_OAUTH_CLIENT_ID`
   - `GOOGLE_OAUTH_CLIENT_SECRET`
3. Build juggler again.

```bash
cargo build --release
```

4. Create a Google Tasks list named exactly `juggler`.
5. Authenticate:

```bash
./target/release/juggler login
```

6. Sync:

```bash
./target/release/juggler sync google-tasks
```

## Commands

```bash
juggler login --help
juggler logout --help
juggler sync google-tasks --help
```

## Dry Run

Use `--dry-run` to preview actions without writing local changes or remote changes:

```bash
RUST_LOG=info juggler sync google-tasks --dry-run
```

## Logging

Juggler uses `env_logger`. If `RUST_LOG` is unset, default level is `info`.

```bash
# default / useful
RUST_LOG=info juggler sync google-tasks

# verbose
RUST_LOG=debug juggler sync google-tasks

# errors only
RUST_LOG=error juggler sync google-tasks

# module filter
RUST_LOG=juggler=debug,reqwest=warn juggler sync google-tasks
```

## Sync Behavior

Each sync reconciles Google Tasks to match local TOML state:

1. Create remote tasks for local todos without `google_task_id`.
2. Update remote tasks when local title/notes/status/due change.
3. Delete remote orphan tasks only when they carry juggler's ownership marker in notes.

Field mapping:

- local `title` -> Google Task `title`
- local `comment` -> Google Task `notes` (plus ownership marker metadata)
- local `done` -> Google Task `status`
- local `due_date` -> Google Task `due`

After sync, todos may gain `google_task_id` values linking them to remote tasks.

Changes made directly in Google Tasks are overwritten on the next sync.

## Limitations

Google Tasks API `due` is effectively date-only. Time precision is not preserved by the public API.

Juggler normalizes outgoing due dates to midnight UTC (`00:00:00Z`) and compares by calendar day to avoid spurious updates.

## Security Notes

- Do not commit OAuth credentials.
- Refresh tokens are stored in the OS keychain after `juggler login`.
- If credentials are stale or revoked, re-run `juggler login`.

## Troubleshooting

### No `juggler` task list found

Create a task list named exactly `juggler` in [Google Tasks](https://tasks.google.com/).

### Invalid token or authentication failures

- Run `juggler login` again.
- Confirm the OAuth scope includes `https://www.googleapis.com/auth/tasks`.

### `invalid_client` during login

Your local OAuth client ID/secret values are likely not configured correctly in `src/config.rs`.

### Sync result looks wrong

- Validate `TODOs.toml` syntax.
- Start with `--dry-run`.
- Remove specific `google_task_id` values if you need those items re-created remotely.
