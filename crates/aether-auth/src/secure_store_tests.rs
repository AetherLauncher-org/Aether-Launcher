//! Deterministic keyring contract tests: no OS keyring or network access.
//! Each test owns a unique temporary metadata directory and an isolated mock vault.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::{CredentialVault, KeyringCredentialStore, SERVICE_NAME};
use crate::store::{CredentialStore, StoreError, StoredCredential};
use crate::token::RefreshToken;
use crate::{AccountId, AccountKind};

const SECRET: &str = "M.R3_BAY.local-refresh-secret-123456";

#[derive(Default)]
struct VaultState {
    entries: HashMap<AccountId, String>,
    calls: usize,
    fail_get: bool,
    fail_set: bool,
    fail_delete: bool,
}

#[derive(Clone, Default)]
struct MemoryVault(Arc<Mutex<VaultState>>);

impl MemoryVault {
    fn modify(&self, edit: impl FnOnce(&mut VaultState)) {
        edit(&mut self.0.lock().expect("mock vault lock"));
    }

    fn calls(&self) -> usize {
        self.0.lock().expect("mock vault lock").calls
    }

    fn contains(&self, account: AccountId) -> bool {
        self.0
            .lock()
            .expect("mock vault lock")
            .entries
            .contains_key(&account)
    }
}

impl CredentialVault for MemoryVault {
    fn get(&self, account: AccountId) -> Result<Option<String>, StoreError> {
        let mut state = self.0.lock().expect("mock vault lock");
        state.calls += 1;
        if state.fail_get {
            return Err(StoreError::ProtectedStoreUnavailable);
        }
        Ok(state.entries.get(&account).cloned())
    }

    fn set(&self, account: AccountId, token: &str) -> Result<(), StoreError> {
        let mut state = self.0.lock().expect("mock vault lock");
        state.calls += 1;
        if state.fail_set {
            return Err(StoreError::ProtectedStoreUnavailable);
        }
        state.entries.insert(account, token.to_owned());
        Ok(())
    }

    fn delete(&self, account: AccountId) -> Result<(), StoreError> {
        let mut state = self.0.lock().expect("mock vault lock");
        state.calls += 1;
        if state.fail_delete {
            return Err(StoreError::ProtectedStoreUnavailable);
        }
        state.entries.remove(&account);
        Ok(())
    }
}

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether-secure-store-test-{}-{}",
            std::process::id(),
            AccountId::new().as_uuid()
        ));
        fs::create_dir_all(&path).expect("create test root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn store(&self, vault: MemoryVault) -> KeyringCredentialStore {
        KeyringCredentialStore::with_vault(&self.0, vault)
    }

    fn json(&self, account: AccountId) -> String {
        fs::read_to_string(self.0.join(format!("{}.json", account.as_uuid())))
            .expect("saved account metadata")
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn microsoft() -> StoredCredential {
    StoredCredential::metadata_only(AccountId::new(), "Alex", AccountKind::Microsoft)
}

#[test]
fn new_secure_store_has_a_metadata_root() {
    let store = KeyringCredentialStore::new("aether-test-metadata");
    assert_eq!(store.metadata.root(), Path::new("aether-test-metadata"));
}

#[test]
fn service_name_is_dedicated_to_refresh_tokens() {
    assert!(SERVICE_NAME.contains("AetherLauncher"));
    assert!(SERVICE_NAME.contains("refresh"));
}

#[test]
fn microsoft_token_round_trip_is_vault_only() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    let store = root.store(vault.clone());
    let credential = microsoft().with_refresh(RefreshToken::parse(SECRET).unwrap());
    store.save(&credential).expect("vault save");
    assert!(vault.contains(credential.account_id));
    let json = root.json(credential.account_id);
    assert!(!json.contains(SECRET));
    assert!(!json.contains("refresh_token"));
    let loaded = store.load(credential.account_id).unwrap().unwrap();
    assert_eq!(loaded, credential);
    assert_eq!(store.list().unwrap(), vec![credential.account_id]);
}

#[test]
fn a_missing_vault_entry_is_not_an_error() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    let store = root.store(vault);
    let account = microsoft();
    store.save(&account).unwrap();
    let loaded = store.load(account.account_id).unwrap().unwrap();
    assert!(loaded.refresh.is_none());
}

#[test]
fn microsoft_save_without_token_revokes_an_existing_secret() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    let store = root.store(vault.clone());
    let account = microsoft();
    let with_token = account.clone().with_refresh(RefreshToken::parse(SECRET).unwrap());
    store.save(&with_token).unwrap();
    assert!(vault.contains(account.account_id));
    store.save(&account).unwrap();
    assert!(!vault.contains(account.account_id));
    assert_eq!(store.load(account.account_id).unwrap().unwrap(), account);
}

#[test]
fn failed_vault_write_does_not_fall_back_to_metadata_or_plaintext() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    vault.modify(|state| state.fail_set = true);
    let store = root.store(vault);
    let account = microsoft().with_refresh(RefreshToken::parse(SECRET).unwrap());
    assert!(matches!(
        store.save(&account),
        Err(StoreError::ProtectedStoreUnavailable)
    ));
    assert_eq!(store.load(account.account_id).unwrap(), None);
    assert!(!root.path().join(format!("{}.json", account.account_id.as_uuid())).exists());
}

#[test]
fn failed_vault_read_does_not_fall_back_to_metadata() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    let store = root.store(vault.clone());
    let account = microsoft().with_refresh(RefreshToken::parse(SECRET).unwrap());
    store.save(&account).unwrap();
    vault.modify(|state| state.fail_get = true);
    let error = store.load(account.account_id).unwrap_err();
    assert!(matches!(error, StoreError::ProtectedStoreUnavailable));
    assert!(!root.json(account.account_id).contains(SECRET));
    assert!(!error.to_string().contains(SECRET));
}

#[test]
fn failed_vault_delete_preserves_metadata_and_allows_retry() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    let store = root.store(vault.clone());
    let account = microsoft().with_refresh(RefreshToken::parse(SECRET).unwrap());
    store.save(&account).unwrap();
    vault.modify(|state| state.fail_delete = true);
    assert!(matches!(
        store.remove(account.account_id),
        Err(StoreError::ProtectedStoreUnavailable)
    ));
    assert!(root.path().join(format!("{}.json", account.account_id.as_uuid())).exists());
    assert!(vault.contains(account.account_id));
    vault.modify(|state| state.fail_delete = false);
    store.remove(account.account_id).expect("retry must succeed");
    assert!(!vault.contains(account.account_id));
    assert_eq!(store.load(account.account_id).unwrap(), None);
    store.remove(account.account_id).expect("absent removal is idempotent");
}

#[test]
fn offline_accounts_and_absent_accounts_never_touch_the_vault() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    vault.modify(|state| {
        state.fail_get = true;
        state.fail_set = true;
        state.fail_delete = true;
    });
    let store = root.store(vault.clone());
    let account = StoredCredential::metadata_only(AccountId::new(), "Player", AccountKind::Offline);
    store.save(&account).expect("save offline without vault");
    assert_eq!(store.load(account.account_id).unwrap(), Some(account.clone()));
    store.remove(account.account_id).expect("remove offline without vault");
    assert_eq!(store.load(account.account_id).unwrap(), None);
    store.remove(AccountId::new()).expect("remove unknown without vault");
    assert_eq!(vault.calls(), 0);
}

#[test]
fn corrupted_vault_secret_is_distinct_from_an_unavailable_vault() {
    let root = TestRoot::new();
    let vault = MemoryVault::default();
    let store = root.store(vault.clone());
    let account = microsoft();
    store.save(&account).unwrap();
    vault.modify(|state| {
        state.entries.insert(account.account_id, String::new());
    });
    let error = store.load(account.account_id).unwrap_err();
    assert!(matches!(error, StoreError::CorruptedCredential));
    assert_eq!(error.to_string(), "the protected credential is malformed");
}
