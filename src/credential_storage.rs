//! Credential storage utilities for refresh tokens.
//!
//! This module centralizes the management of a refresh token using the
//! operating system's secure credential store via the `keyring` crate.
//!
//! Platform behavior:
//! - macOS/iOS: Uses the system Keychain
//! - Windows: Uses the Windows Credential Manager
//! - Linux/*nix: Uses the DBus Secret Service (synchronous API)
//!
//! Notes:
//! - Service and account names must be non-empty (macOS treats empty as wildcard).
//! - These identifiers should remain stable across app versions to allow retrieval.
//!
//! Testability:
//! - The public API accepts an optional `&Entry`. When `None`, a global singleton
//!   entry is used. Tests can pass a mock `Entry` explicitly without touching
//!   the global state or OS keychain.

use keyring::Entry;
use std::sync::{Mutex, OnceLock};

pub const KEYRING_SERVICE: &str = "juggler";
pub const KEYRING_ACCOUNT_GOOGLE_TASKS: &str = "google-tasks";

fn entry_singleton() -> &'static Mutex<Option<Entry>> {
    static SINGLETON: OnceLock<Mutex<Option<Entry>>> = OnceLock::new();
    SINGLETON.get_or_init(|| {
        // Entry::new returns Result; with constant non-empty identifiers,
        // initialization should succeed. If it fails, unwrap is acceptable.
        Mutex::new(Some(
            Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT_GOOGLE_TASKS).unwrap(),
        ))
    })
}

/// Store the refresh token in the OS keychain.
///
/// If `entry_opt` is `Some(&Entry)`, that entry is used; otherwise the
/// global singleton Entry is used.
pub fn store_refresh_token_with(
    entry_opt: Option<&Entry>,
    refresh_token: &str,
) -> keyring::Result<()> {
    with_entry(entry_opt, |e| e.set_password(refresh_token))
}

/// Convenience wrapper that uses the global singleton Entry.
pub fn store_refresh_token(refresh_token: &str) -> keyring::Result<()> {
    store_refresh_token_with(None, refresh_token)
}

/// Retrieve the refresh token from the OS keychain.
///
/// If `entry_opt` is `Some(&Entry)`, that entry is used; otherwise the
/// global singleton Entry is used.
pub fn get_refresh_token_with(entry_opt: Option<&Entry>) -> keyring::Result<String> {
    with_entry(entry_opt, |e| e.get_password())
}

/// Convenience wrapper that uses the global singleton Entry.
pub fn get_refresh_token() -> keyring::Result<String> {
    get_refresh_token_with(None)
}

/// Delete the stored refresh token from the OS keychain.
///
/// If `entry_opt` is `Some(&Entry)`, that entry is used; otherwise the
/// global singleton Entry is used.
pub fn delete_refresh_token_with(entry_opt: Option<&Entry>) -> keyring::Result<()> {
    with_entry(entry_opt, |e| e.delete_credential())
}

/// Convenience wrapper that uses the global singleton Entry.
pub fn delete_refresh_token() -> keyring::Result<()> {
    delete_refresh_token_with(None)
}

fn with_entry<T>(
    entry_opt: Option<&Entry>,
    f: impl FnOnce(&Entry) -> keyring::Result<T>,
) -> keyring::Result<T> {
    if let Some(e) = entry_opt {
        return f(e);
    }
    let guard = entry_singleton().lock().unwrap();
    let entry = guard.as_ref().expect("entry not initialized");
    f(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyring::mock;

    fn new_mock_entry() -> Entry {
        let builder = mock::default_credential_builder();
        let cred = builder
            .build(None, KEYRING_SERVICE, KEYRING_ACCOUNT_GOOGLE_TASKS)
            .expect("mock credential");
        Entry::new_with_credential(cred)
    }

    #[test]
    fn test_store_and_get_refresh_token_with_mock_store() {
        // Each test uses its own mock-backed Entry and does not touch global state
        let token = "test_refresh_token_123";
        let entry = new_mock_entry();
        store_refresh_token_with(Some(&entry), token).expect("store should succeed");

        let got = get_refresh_token_with(Some(&entry)).expect("get should succeed");
        assert_eq!(got, token);
    }

    #[test]
    fn test_delete_refresh_token_with_mock_store() {
        let entry = new_mock_entry();
        store_refresh_token_with(Some(&entry), "tok").expect("store should succeed");
        delete_refresh_token_with(Some(&entry)).expect("delete should succeed");

        assert!(
            get_refresh_token_with(Some(&entry)).is_err(),
            "get after delete should error"
        );
    }

    #[test]
    fn test_get_missing_refresh_token_returns_err() {
        // With fresh mock credential and no prior set, get should error
        let entry = new_mock_entry();
        assert!(
            get_refresh_token_with(Some(&entry)).is_err(),
            "get without set should error"
        );
    }
}
