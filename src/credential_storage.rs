//! Credential storage utilities for refresh tokens.
//!
//! This module defines a `CredentialStore` trait and provides two implementations:
//! - `KeyringCredentialStore`: uses the OS keychain via the `keyring` crate
//! - `InMemoryCredentialStore`: a simple, thread-safe in-memory store for tests
//!
//! Platform behavior for the keyring-backed implementation:
//! - macOS/iOS: Uses the system Keychain
//! - Windows: Uses the Windows Credential Manager
//! - Linux/*nix: Uses the DBus Secret Service (synchronous API)
//!
//! Notes:
//! - Service and account names must be non-empty (macOS treats empty as wildcard).
//! - These identifiers should remain stable across app versions to allow retrieval.

use keyring::Entry;
use log::info;
use std::error::Error;
use std::fmt;
use std::sync::Mutex;

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
}

impl CredentialStore for KeyringCredentialStore {
    fn store_refresh_token(&self, refresh_token: &str) -> Result<(), CredentialError> {
        info!(
            "Keyring: storing refresh token (service={}, account={})...",
            CREDENTIAL_KEYRING_SERVICE, CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS
        );
        let entry = self.make_entry()?;
        Entry::set_password(&entry, refresh_token)
            .map_err(|e| CredentialError::Backend(e.to_string()))
    }

    fn get_refresh_token(&self) -> Result<String, CredentialError> {
        info!(
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
        info!(
            "Keyring: deleting refresh token (service={}, account={})...",
            CREDENTIAL_KEYRING_SERVICE, CREDENTIAL_KEYRING_ACCOUNT_GOOGLE_TASKS
        );
        let entry = self.make_entry()?;
        Entry::delete_credential(&entry).map_err(|e| CredentialError::Backend(e.to_string()))
    }
}

/// In-memory credential store for tests.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Default)]
pub struct InMemoryCredentialStore {
    token: Mutex<Option<String>>,
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
