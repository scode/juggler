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

use keyring::Entry;

/// Keychain/credential manager service name used for this application.
const KEYRING_SERVICE: &str = "juggler";

/// Account identifier for the stored refresh token credential.
const KEYRING_ACCOUNT_REFRESH_TOKEN: &str = "refresh-token";

fn keyring_entry() -> keyring::Result<Entry> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT_REFRESH_TOKEN)
}

/// Store the refresh token in the OS keychain.
pub fn store_refresh_token(refresh_token: &str) -> keyring::Result<()> {
    let entry = keyring_entry()?;
    entry.set_password(refresh_token)
}

/// Retrieve the refresh token from the OS keychain.
pub fn get_refresh_token() -> keyring::Result<String> {
    let entry = keyring_entry()?;
    entry.get_password()
}

/// Delete the stored refresh token from the OS keychain.
pub fn delete_refresh_token() -> keyring::Result<()> {
    let entry = keyring_entry()?;
    entry.delete_credential()
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyring::{mock, set_default_credential_builder};

    fn setup_mock_store() {
        // Use the in-memory mock credential store for deterministic tests
        set_default_credential_builder(mock::default_credential_builder());
    }

    #[test]
    fn test_store_and_get_refresh_token_with_mock_store() {
        setup_mock_store();

        let token = "test_refresh_token_123";
        store_refresh_token(token).expect("store should succeed");

        let got = get_refresh_token().expect("get should succeed");
        assert_eq!(got, token);
    }

    #[test]
    fn test_delete_refresh_token_with_mock_store() {
        setup_mock_store();

        store_refresh_token("tok").expect("store should succeed");
        delete_refresh_token().expect("delete should succeed");

        assert!(
            get_refresh_token().is_err(),
            "get after delete should error"
        );
    }

    #[test]
    fn test_get_missing_refresh_token_returns_err() {
        setup_mock_store();

        // With fresh mock store and no prior set, get should error
        assert!(get_refresh_token().is_err(), "get without set should error");
    }
}
