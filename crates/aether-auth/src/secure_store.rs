//! OS-backed refresh-token storage. Never writes token material to JSON.
//!
//! Unlike the metadata-only FileCredentialStore, this implementation stores the
//! refresh token under a per-account key in Windows Credential Manager, macOS
//! Keychain, or the Linux Secret Service. It fails closed if the vault is locked
//! or unavailable: no fallback to an unencrypted file is permitted.
//!
//! The keyring and the metadata JSON are separate stores. They cannot be updated
//! transactionally; if the metadata write fails after the vault write succeeds,
//! a later retry with the same account id safely updates the same entry.

use std::path::PathBuf;

use keyring::{Entry, Error as KeyringError};

use crate::{AccountId, token::RefreshToken};
use crate::store::{CredentialStore, FileCredentialStore, StoreError, StoredCredential};

/// Dedicated namespace for Microsoft OAuth refresh credentials, not access tokens.
const SERVICE_NAME: &str = "org.aetherlauncher.AetherLauncher.microsoft-refresh";

/// Persists nonsecret account metadata in JSON and refresh tokens in the OS vault.
pub struct KeyringCredentialStore {
    metadata: FileCredentialStore,
}

impl KeyringCredentialStore {
    /// A secure store for account records rooted at the application's data directory.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            metadata: FileCredentialStore::new(root),
        }
    }

    fn entry(account: AccountId) -> Result<Entry, StoreError> {
        Entry::new(SERVICE_NAME, &account.as_uuid().to_string())
            .map_err(|_| StoreError::ProtectedStoreUnavailable)
    }

    fn delete_if_present(entry: &Entry) -> Result<(), StoreError> {
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(StoreError::ProtectedStoreUnavailable),
        }
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn load(&self, account: AccountId) -> Result<Option<StoredCredential>, StoreError> {
        let Some(mut credential) = self.metadata.load(account)? else {
            return Ok(None);
        };
        if credential.kind == crate::AccountKind::Microsoft {
            let entry = Self::entry(account)?;
            credential.refresh = match entry.get_password() {
                Ok(token) => Some(
                    RefreshToken::parse(token)
                        .map_err(|_| StoreError::ProtectedStoreUnavailable)?,
                ),
                Err(KeyringError::NoEntry) => None,
                Err(_) => return Err(StoreError::ProtectedStoreUnavailable),
            };
        }
        Ok(Some(credential))
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), StoreError> {
        if credential.kind == crate::AccountKind::Microsoft {
            let entry = Self::entry(credential.account_id)?;
            match credential.refresh.as_ref() {
                Some(refresh) => entry
                    .set_password(refresh.expose())
                    .map_err(|_| StoreError::ProtectedStoreUnavailable)?,
                None => Self::delete_if_present(&entry)?,
            }
        }
        // FileCredentialStore always strips secrets from the JSON representation.
        self.metadata.save(credential)
    }

    fn remove(&self, account: AccountId) -> Result<(), StoreError> {
        // Delete the vault entry first. If it fails, preserve metadata for retry.
        Self::delete_if_present(&Self::entry(account)?)?;
        self.metadata.remove(account)
    }

    fn list(&self) -> Result<Vec<AccountId>, StoreError> {
        self.metadata.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_secure_store_has_a_metadata_root() {
        let store = KeyringCredentialStore::new("aether-test-metadata");
        assert_eq!(store.metadata.root(), std::path::Path::new("aether-test-metadata"));
    }

    #[test]
    fn service_name_is_dedicated_to_refresh_tokens() {
        assert!(SERVICE_NAME.contains("AetherLauncher"));
        assert!(SERVICE_NAME.contains("refresh"));
    }
}
