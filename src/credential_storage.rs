//! Credential storage utilities for refresh tokens.
//!
//! This module defines `CredentialStore`, the interface used by login/logout
//! and sync code to persist refresh tokens.
//!
//! `KeyringCredentialStore` uses the platform keychain through the `keyring`
//! crate, and `InMemoryCredentialStore` is used in tests.

use keyring::Entry;
use log::debug;
use std::error::Error;
use std::fmt;

use crate::config::{CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS, CREDENTIAL_KEYRING_SERVICE};

/// Errors returned by `CredentialStore` implementations.
#[derive(Debug)]
pub enum CredentialError {
    /// The requested credential does not exist.
    NotFound,
    /// Backend/storage failure with message.
    Backend(String),
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialError::NotFound => write!(f, "credential not found"),
            CredentialError::Backend(msg) => write!(f, "credential backend error: {}", msg),
        }
    }
}

impl Error for CredentialError {}

/// OAuth credential storage trait so we can dependency inject it - allowing
/// testing without touching the real keyring.
pub trait CredentialStore: Send + Sync {
    fn store_refresh_token(&self, refresh_token: &str) -> Result<(), CredentialError>;
    fn get_refresh_token(&self) -> Result<String, CredentialError>;
    fn delete_refresh_token(&self) -> Result<(), CredentialError>;
}

/// Keyring-backed credential store.
pub struct KeyringCredentialStore;

impl KeyringCredentialStore {
    pub fn new() -> Self {
        Self
    }

    fn make_entry(&self) -> Result<Entry, CredentialError> {
        Entry::new(
            CREDENTIAL_KEYRING_SERVICE,
            CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS,
        )
        .map_err(|e| CredentialError::Backend(e.to_string()))
    }

    fn map_delete_error(error: keyring::Error) -> Result<(), CredentialError> {
        match error {
            keyring::Error::NoEntry => Ok(()),
            other => Err(CredentialError::Backend(other.to_string())),
        }
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn store_refresh_token(&self, refresh_token: &str) -> Result<(), CredentialError> {
        debug!(
            "Keyring: storing refresh token (service={}, account={})...",
            CREDENTIAL_KEYRING_SERVICE, CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS
        );
        let entry = self.make_entry()?;
        Entry::set_password(&entry, refresh_token)
            .map_err(|e| CredentialError::Backend(e.to_string()))
    }

    fn get_refresh_token(&self) -> Result<String, CredentialError> {
        debug!(
            "Keyring: retrieving refresh token (service={}, account={})...",
            CREDENTIAL_KEYRING_SERVICE, CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS
        );
        let entry = self.make_entry()?;
        match Entry::get_password(&entry) {
            Ok(s) => Ok(s),
            Err(keyring::Error::NoEntry) => Err(CredentialError::NotFound),
            Err(e) => Err(CredentialError::Backend(e.to_string())),
        }
    }

    fn delete_refresh_token(&self) -> Result<(), CredentialError> {
        debug!(
            "Keyring: deleting refresh token (service={}, account={})...",
            CREDENTIAL_KEYRING_SERVICE, CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS
        );
        let entry = self.make_entry()?;
        match Entry::delete_credential(&entry) {
            Ok(()) => Ok(()),
            Err(error) => Self::map_delete_error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// In-memory credential store for tests.
    #[derive(Default)]
    pub struct InMemoryCredentialStore {
        token: Mutex<Option<String>>,
    }

    impl InMemoryCredentialStore {
        pub fn new() -> Self {
            Self {
                token: Mutex::new(None),
            }
        }
    }

    impl CredentialStore for InMemoryCredentialStore {
        fn store_refresh_token(&self, refresh_token: &str) -> Result<(), CredentialError> {
            let mut guard = self.token.lock().unwrap();
            *guard = Some(refresh_token.to_string());
            Ok(())
        }

        fn get_refresh_token(&self) -> Result<String, CredentialError> {
            let guard = self.token.lock().unwrap();
            match &*guard {
                Some(s) => Ok(s.clone()),
                None => Err(CredentialError::NotFound),
            }
        }

        fn delete_refresh_token(&self) -> Result<(), CredentialError> {
            let mut guard = self.token.lock().unwrap();
            *guard = None;
            Ok(())
        }
    }

    #[test]
    fn test_store_and_get_refresh_token_in_memory() {
        let store = InMemoryCredentialStore::new();
        let token = "test_refresh_token_123";
        store
            .store_refresh_token(token)
            .expect("store should succeed");
        let got = store.get_refresh_token().expect("get should succeed");
        assert_eq!(got, token);
    }

    #[test]
    fn test_delete_refresh_token_in_memory() {
        let store = InMemoryCredentialStore::new();
        store
            .store_refresh_token("tok")
            .expect("store should succeed");
        store.delete_refresh_token().expect("delete should succeed");
        assert!(matches!(
            store.get_refresh_token(),
            Err(CredentialError::NotFound)
        ));
    }

    #[test]
    fn test_get_missing_refresh_token_returns_err() {
        let store = InMemoryCredentialStore::new();
        assert!(matches!(
            store.get_refresh_token(),
            Err(CredentialError::NotFound)
        ));
    }

    #[test]
    fn test_keyring_delete_no_entry_is_treated_as_success() {
        assert!(KeyringCredentialStore::map_delete_error(keyring::Error::NoEntry).is_ok());
    }

    #[test]
    fn test_keyring_delete_other_errors_are_preserved() {
        let platform_error =
            keyring::Error::PlatformFailure(Box::new(std::io::Error::other("backend failure")));
        let err = KeyringCredentialStore::map_delete_error(platform_error)
            .expect_err("non-no-entry errors should surface");
        assert!(matches!(err, CredentialError::Backend(_)));
    }
}
