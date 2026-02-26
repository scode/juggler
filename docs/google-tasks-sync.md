# Google Tasks Synchronization (Bare Bones)

Google Tasks sync currently exists in a very manual, bare-bones form.

The sync direction is one-way (`local TOML -> Google Tasks`). Your local `TODOs.toml` remains authoritative.

## Manual Setup Required

Before login/sync will work for your own environment, you must create your own Google OAuth desktop client and provide its credentials at runtime.

1. Create an OAuth desktop client in Google Cloud Console.
2. Build juggler.

```bash
cargo build --release
```

3. Store your desktop client values in shell variables (for convenience):

```bash
export GOOGLE_OAUTH_CLIENT_ID='your-client-id'
export GOOGLE_OAUTH_CLIENT_SECRET='your-client-secret'
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

### Why juggler does not ship OAuth client credentials

Important context for desktop/native OAuth clients:

- Google treats installed applications as public clients and explicitly documents that they cannot keep client secrets confidential.
- Evidence: [Google OAuth 2.0 for installed apps](https://developers.google.com/identity/protocols/oauth2/native-app), [RFC 8252 (OAuth 2.0 for Native Apps)](https://www.rfc-editor.org/rfc/rfc8252), and [RFC 7636 (PKCE)](https://www.rfc-editor.org/rfc/rfc7636).
- Consequence: anyone with the same client id/secret can present as this app during OAuth. That is expected for native clients; security is primarily user consent + PKCE, not secrecy of the client secret string.

In practice, automated Google compliance/abuse systems can still detect embedded client credentials in distributed binaries/source and open policy/case workflows against that OAuth client.

To avoid shipping baked-in values that trigger those automated cases, juggler requires runtime client credentials via flags (`--google-oauth-client-id`, `--google-oauth-client-secret`) or environment variables (`GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET`).

## Commands

```bash
juggler login --help
juggler logout --help
juggler sync google-tasks --help
```

`--google-oauth-client-id` and `--google-oauth-client-secret` are global CLI flags used by `login` and `sync`.

`GOOGLE_OAUTH_CLIENT_ID` and `GOOGLE_OAUTH_CLIENT_SECRET` provide clap env fallbacks for those flags. For `login` and `sync`, each value must be provided via flag or env var. `logout` ignores both.

If you use TUI mode and choose "save + sync on exit", provide both OAuth values via env vars (recommended) or global flags when launching `juggler`. If credentials are missing, juggler still saves local changes and skips sync with a diagnostic.

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

Your OAuth client id and/or secret (from flags or env vars) are likely incorrect or do not belong to the same OAuth desktop client.

### Sync result looks wrong

- Validate `TODOs.toml` syntax.
- Start with `--dry-run`.
- Remove specific `google_task_id` values if you need those items re-created remotely.
