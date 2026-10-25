//! OS-backed refresh-token storage. Never writes token material to JSON.
//!
//! Refresh tokens live only in the OS credential vault. FileCredentialStore
//! persists metadata and strips all token fields. The two stores are not
//! transactional: a metadata-write failure may leave an unreferenced vault entry,
//! which can be replaced by retrying with the same account id. No unencrypted
//! fallback is attempted when vault operations fail.

use std::path::PathBuf;

use keyring::{Entry, Error as KeyringError};

use crate::store::{CredentialStore, FileCredentialStore, StoreError, StoredCredential};
use crate::{AccountId, AccountKind, token::RefreshToken};

const SERVICE_NAME: &str = "org.aetherlauncher.AetherLauncher.microsoft-refresh";

/// A narrow, mockable boundary for secure secret operations. It deliberately
/// does not expose keyring::Entry or any credentials in errors.
trait CredentialVault: Send + Sync {
    fn get(&self, account: AccountId) -> Result<Option<String>, StoreError>;
    fn set(&self, account: AccountId, token: &str) -> Result<(), StoreError>;
    fn delete(&self, account: AccountId) -> Result<(), StoreError>;
}

struct OsCredentialVault;

impl OsCredentialVault {
    fn entry(account: AccountId) -> Result<Entry, StoreError> {
        Entry::new(SERVICE_NAME, &account.as_uuid().to_string())
            .map_err(|_| StoreError::ProtectedStoreUnavailable)
    }
}

impl CredentialVault for OsCredentialVault {
    fn get(&self, account: AccountId) -> Result<Option<String>, StoreError> {
        match Self::entry(account)?
            .get_password()
        {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(StoreError::ProtectedStoreUnavailable),
        }
    }

    fn set(&self, account: AccountId, token: &str) -> Result<(), StoreError> {
        Self::entry(account)?
            .set_password(token)
            .map_err(|_| StoreError::ProtectedStoreUnavailable)
    }

    fn delete(&self, account: AccountId) -> Result<(), StoreError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(StoreError::ProtectedStoreUnavailable),
        }
    }
}

/// Persists nonsecret account metadata in JSON and Microsoft refresh tokens
/// exclusively in Windows Credential Manager, macOS Keychain or Linux Secret Service.
pub struct KeyringCredentialStore {
    metadata: FileCredentialStore,
    vault: Box<dyn CredentialVault>,
}

impl KeyringCredentialStore {
    /// Construct the production store with an operating-system credential vault.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_vault(root, OsCredentialVault)
    }

    fn with_vault(root: impl Into<PathBuf>, vault: impl CredentialVault + 'static) -> Self {
        Self {
            metadata: FileCredentialStore::new(root),
            vault: Box::new(vault),
        }
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn load(&self, account: AccountId) -> Result<Option<StoredCredential>, StoreError> {
        let Some(mut credential) = self.metadata.load(account)? else {
            return Ok(None);
        };
        if credential.kind == AccountKind::Microsoft {
            credential.refresh = self
                .vault
                .get(account)?
                .map(|token| RefreshToken::parse(token).map_err(|_| StoreError::CorruptedCredential))
                .transpose()?;
        }
        Ok(Some(credential))
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), StoreError> {
        if credential.kind == AccountKind::Microsoft {
            match credential.refresh.as_ref() {
                Some(refresh) => self.vault.set(credential.account_id, refresh.expose())?,
                None => self.vault.delete(credential.account_id)?,
            }
        }
        // The file encoder *always* strips tokens from persisted JSON.
        self.metadata.save(credential)
    }

    fn remove(&self, account: AccountId) -> Result<(), StoreError> {
        // Read trusted metadata first: offline accounts and absent accounts must
        // not require an unlocked OS vault. Never delete metadata if revoking a
        // known Microsoft credential fails, so removal can be retried safely.
        let previous = self.metadata.load(account)?;
        if previous.is_some_and(|credential| credential.kind == AccountKind::Microsoft) {
            self.vault.delete(account)?;
        }
        self.metadata.remove(account)
    }

    fn list(&self) -> Result<Vec<AccountId>, StoreError> {
        self.metadata.list()
    }
}

#[cfg(test)]
#[path = "secure_store_tests.rs"]
mod tests;
