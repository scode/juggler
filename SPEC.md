# juggler Product Specification (Reverse-Engineered)

Purpose: Define what the product does and why, without prescribing implementation details.

## 1) Product Purpose

juggler is a local-first task manager for people who want keyboard-only task control in a terminal, with optional publishing to Google Tasks so the same tasks are visible in Google's ecosystem.

The product prioritizes:
- Fast, low-friction task triage from the terminal.
- Human-readable local ownership of data.
- Predictable synchronization where local state is authoritative.

## 2) Product Goals

1. Let users manage task state quickly without leaving the terminal.
2. Keep tasks durable and editable as plain YAML under user control.
3. Provide optional one-way synchronization to Google Tasks with minimal setup.
4. Make destructive remote effects explicit and predictable.

## 3) Non-Goals

1. Two-way merge/conflict resolution with Google Tasks.
2. Multi-user collaboration.
3. Cloud-hosted storage owned by juggler.
4. Rich project-management features (dependencies, boards, assignees, etc.).

## 4) Core User Outcomes

1. Users can capture, edit, complete, and schedule tasks from a TUI.
2. Users can apply operations to a focused task or a selected set.
3. Users can keep local data as the source of truth and push that state to Google Tasks when desired.
4. Users can safely preview sync effects before writing local or remote changes.

## 5) Modes and Entry Points

### Interactive Mode (Default)

Launching `juggler` opens a two-section task interface (pending + done) with keyboard navigation and editing actions.

Why: terminal users should manage tasks in a continuous flow without command churn.

### Command Mode

- `juggler login`: authorize against Google and store a long-lived refresh credential.
- `juggler logout`: remove stored refresh credential.
- `juggler sync google-tasks`: push local YAML state to Google Tasks.
- `--dry-run` on sync: preview operations with no local-file writes and no Google writes.

Why: auth/sync lifecycle should be scriptable and usable outside interactive sessions.

## 6) Task Model (Product Semantics)

Each task has:
- `title` (primary user-facing label)
- `comment` (optional details, multiline allowed)
- `done` (completion state)
- `due_date` (optional timestamp)
- `google_task_id` (optional linkage to a remote Google task)

Behavioral semantics:
- Titles are required and must be non-empty after trimming whitespace.
- Pending and done are separate sections in the UI.
- Completion state determines section membership.
- Due dates support urgency signaling and quick adjustments.
- `google_task_id` is an identity link used to reconcile local tasks with remote tasks.

Why: this is the minimal model needed for quick personal task control plus sync reconciliation.

## 7) UX Behavior Requirements

1. Keyboard-first operation is the default interaction model.
2. Batch actions operate on selected tasks; if none are selected, they operate on the focused task.
3. Editing and creation are performed in the user’s preferred external editor.
4. Due-date adjustments support quick fixed offsets and a custom relative offset prompt.
5. Custom relative delays are always interpreted relative to "now" (current time), not relative to an existing due date.
6. In-session ordering stability is currently preferred over continuous re-sorting; tasks may drift from strict due-date ordering until a later reload/session.
7. Google Task titles synced by juggler intentionally include the `j:` prefix.
8. Exiting can either save locally only or save + sync remotely.

Why: power users need fast repetitive operations and full-text editing with their existing tools.

## 8) Local Data Ownership and Safety Requirements

1. Local YAML is always the authoritative record.
2. Data is stored in a user-owned file under the user’s home directory.
3. Writes must prioritize durability and corruption resistance.
4. Previous versions are archived automatically to support rollback/recovery.
5. Durability strategy assumes users may rely on ordinary filesystem backup tools or cloud file synchronization without rewind/history; automated backups therefore create fresh archive files rather than mutating one backup in place.
6. Dry-run sync must not mutate local files.

Why: users should never lose control of their source data because of sync or transport failures.

## 9) Local Write Atomicity Requirements

1. Local TODO writes must use atomic replacement semantics.
2. The target content must be written to a temporary file first.
3. File contents must be flushed and synced to stable storage before replacement.
4. Replacement must happen via rename into place so readers see either old or new content, not partial data.
5. Atomic write behavior should preserve the recovery properties expected by local-first tooling.

Why: crash/power-loss scenarios should not leave partially-written primary task files.

## 10) Google Tasks Sync Requirements

1. Sync direction is one-way: local -> Google.
2. Remote list scope is fixed to a named Google task list (`juggler`).
3. Sync reconciles creates, updates, and deletes so remote state matches local state, subject to explicit ownership semantics that preserve safety without user friction.
4. Missing remote tasks referenced by local IDs are re-created from local state.
5. Dry-run reports intended effects without applying them.
6. OAuth should require minimal repeated user interaction after initial login.
7. Remote deletion policy should minimize user friction by making ownership effectively unambiguous in normal operation.
8. Ownership semantics should be explicit and machine-reliable (for example a dedicated ownership marker), not inferred from cosmetic title formatting.
9. If residual ambiguity exists, it must be narrow enough that deletion remains a highly reliable default for true juggler-owned tasks.

Why: users want predictable publication of local state, not bidirectional conflict management.

## 11) Security and Privacy Requirements

1. Authentication credentials are stored in OS keychain facilities, not in task YAML.
2. Logging should avoid exposing sensitive tokens.
3. OAuth flow must validate callback state to prevent callback forgery/cross-session injection.

Why: this is a local desktop tool; compromise of credentials should be minimized by default.

## 12) Error-Handling Requirements

1. User-facing failures should preserve local data and fail safely.
2. Sync/auth failures should be explicit and diagnosable via logs.
3. Recoverable workflows (login again, dry-run, local save) should remain available after failure.
4. In "save + sync" exits, remote sync must be blocked if local save fails.
5. Logout should be idempotent (successful even when no credential is currently stored).

Why: trust depends on transparent failures and no silent data corruption/loss.

## 13) Known Divergences (Observed Current Behavior vs Likely Intent)

These are intentionally documented here for product clarity; fixing them is separate work.

1. Empty titles are blocked on create but allowed on edit.
Reason this is likely a divergence: creation enforces non-empty title semantics, but the same invariant is not enforced when editing existing tasks.

2. "Quit with sync" can continue to remote sync even when the initial local save fails.
Reason this is likely a divergence: product intent suggests "save local first, then sync" as a safety guard, but the current flow may still sync remotely after a local-save error.

3. Remote deletion is strict reconciliation without explicit ownership semantics that make deletion confidence high by design.
Reason this is likely a divergence: intended behavior prefers low-friction deletion with minimal ambiguity, rather than relying on protective ambiguity handling.

4. Logout currently errors in some "already logged out" states.
Reason this is likely a divergence: intended behavior is idempotent logout.
