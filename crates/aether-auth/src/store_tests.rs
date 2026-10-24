//! Tests for the credential store.
//!
//! Every test creates its own root under [`std::env::temp_dir`], named after a fresh
//! [`AccountId`], so parallel test threads never share a directory and nothing under a
//! real user profile is touched. The guard removes the directory when the test ends,
//! including on panic.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    CredentialStore, FileCredentialStore, RefreshTokenPersistence, StoreError, StoredCredential,
};
use crate::token::RefreshToken;
use crate::{AccountId, AccountKind};

const SECRET: &str = "M.R3_BAY.refresh-secret-value-1234";

/// A temporary root that deletes itself.
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether-auth-store-test-{}-{}",
            std::process::id(),
            AccountId::new().as_uuid()
        ));
        fs::create_dir_all(&path).expect("the temporary root should be creatable");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn store(&self) -> FileCredentialStore {
        FileCredentialStore::new(&self.path)
    }

    /// The raw document for `account`, exactly as it sits on the disk.
    fn raw_json(&self, account: AccountId) -> String {
        fs::read_to_string(self.path.join(format!("{}.json", account.as_uuid())))
            .expect("the document should exist")
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn microsoft_account(name: &str) -> StoredCredential {
    StoredCredential::metadata_only(AccountId::new(), name, AccountKind::Microsoft)
}

/// The document a `Deny` store produces for a credential that carries a token in memory.
fn denied_document(root: &TempRoot) -> (AccountId, String) {
    let store = root.store();
    let account = microsoft_account("Steve").with_refresh(RefreshToken::new(SECRET));
    store.save(&account).expect("the save should succeed");
    (account.account_id, root.raw_json(account.account_id))
}

#[test]
fn a_new_store_denies_refresh_token_persistence() {
    let root = TempRoot::new();
    let store = root.store();

    assert_eq!(store.persistence(), RefreshTokenPersistence::Deny);
    assert_eq!(
        RefreshTokenPersistence::default(),
        RefreshTokenPersistence::Deny
    );
    assert_eq!(store.root(), root.path());
}

#[test]
fn opting_in_keeps_the_chosen_policy() {
    let root = TempRoot::new();
    let store = root
        .store()
        .with_refresh_persistence(RefreshTokenPersistence::Allow);

    assert_eq!(store.persistence(), RefreshTokenPersistence::Allow);
    assert_eq!(store.root(), root.path());
}

#[test]
fn a_denied_save_writes_no_clear_text_token() {
    let root = TempRoot::new();
    let (account, document) = denied_document(&root);

    assert!(!document.contains(SECRET));
    assert!(!document.contains("refresh_token"));

    let loaded = root
        .store()
        .load(account)
        .expect("the load should succeed")
        .expect("the account should be present");
    assert_eq!(loaded.refresh, None);
    assert_eq!(loaded.display_name, "Steve");
}

#[test]
fn an_allowed_save_refuses_unprotected_refresh_tokens() {
    let root = TempRoot::new();
    let store = root
        .store()
        .with_refresh_persistence(RefreshTokenPersistence::Allow);
    let account = microsoft_account("Alex").with_refresh(RefreshToken::new(SECRET));

    assert!(matches!(
        store.save(&account),
        Err(StoreError::InsecurePersistence)
    ));
    assert!(!root.path().join(format!("{}.json", account.account_id.as_uuid())).exists());
}

#[test]
fn existing_plaintext_refresh_token_documents_are_refused() {
    let root = TempRoot::new();
    let account = microsoft_account("Steve");
    root.store().save(&account).unwrap();
    let raw = format!(
        r#"{{"account_id":"{}","display_name":"Steve","kind":"microsoft","refresh_token":"{}"}}"#,
        account.account_id.as_uuid(),
        SECRET
    );
    fs::write(root.path().join(format!("{}.json", account.account_id.as_uuid())), raw).unwrap();

    for policy in [RefreshTokenPersistence::Deny, RefreshTokenPersistence::Allow] {
        let store = root.store().with_refresh_persistence(policy);
        assert!(matches!(store.load(account.account_id), Err(StoreError::UntrustedContent)));
    }
}

#[test]
fn metadata_document_never_contains_any_token() {
    let root = TempRoot::new();
    let account = microsoft_account("Steve").with_refresh(RefreshToken::new(SECRET));
    root.store().save(&account).expect("metadata is permitted");
    let document = root.raw_json(account.account_id);

    for key in ["access_token", "xsts_token", "minecraft_token", "xbl_token", "refresh_token"] {
        assert!(!document.contains(key), "document must omit {key}");
    }
    for key in ["account_id", "display_name", "kind"] {
        assert!(document.contains(key), "document must contain {key}");
    }
}

#[test]
fn an_empty_xuid_is_omitted() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve").with_xuid(Some(String::new()));

    store.save(&account).expect("the save should succeed");
    let document = root.raw_json(account.account_id);

    assert!(!document.contains("xuid"));
    let loaded = store
        .load(account.account_id)
        .expect("the load should succeed")
        .expect("the account should be present");
    assert_eq!(loaded.xuid, None);
}

#[test]
fn an_atomic_save_leaves_no_temporary_file_behind() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");

    store.save(&account).expect("the save should succeed");
    store.save(&account).expect("the overwrite should succeed");

    let mut names: Vec<String> = fs::read_dir(root.path())
        .expect("the root should be listable")
        .map(|entry| {
            entry
                .expect("the entry should be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();

    assert_eq!(names, [format!("{}.json", account.account_id.as_uuid())]);
    assert!(!names.iter().any(|name| name.ends_with(".tmp")));
}

#[test]
fn a_document_with_an_access_token_is_refused() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");
    store.save(&account).expect("the save should succeed");

    let tampered = format!(
        r#"{{"account_id":"{}","display_name":"Steve","kind":"microsoft","access_token":"pl4nted-access-token-value"}}"#,
        account.account_id.as_uuid()
    );
    fs::write(
        root.path()
            .join(format!("{}.json", account.account_id.as_uuid())),
        tampered,
    )
    .expect("the tampered document should be writable");

    let error = store
        .load(account.account_id)
        .expect_err("the planted token must be refused");

    assert!(matches!(error, StoreError::UntrustedContent));
    assert_eq!(
        error.to_string(),
        "the stored credential contains a token type this store never persists"
    );
}

#[test]
fn every_forbidden_token_field_is_refused_including_nested_and_escaped_ones() {
    let cases = [
        r#""xsts_token":"planted-value""#,
        r#""minecraft_token":"planted-value""#,
        r#""xbl_token":"planted-value""#,
        r#""Access_Token":"planted-value""#,
        r#""access\u005ftoken":"planted-value""#,
        r#""extra":{"access_token":"planted-value"}"#,
        r#""extra":[{"xsts_token":"planted-value"}]"#,
    ];

    for case in cases {
        let root = TempRoot::new();
        let store = root.store();
        let account = microsoft_account("Steve");
        store.save(&account).expect("the save should succeed");

        let tampered = format!(
            r#"{{"account_id":"{}","display_name":"Steve","kind":"microsoft",{case}}}"#,
            account.account_id.as_uuid()
        );
        fs::write(
            root.path()
                .join(format!("{}.json", account.account_id.as_uuid())),
            tampered,
        )
        .expect("the tampered document should be writable");

        let error = store
            .load(account.account_id)
            .expect_err("the planted token must be refused");
        assert!(
            matches!(error, StoreError::UntrustedContent),
            "case {case} was not refused"
        );
    }
}

#[test]
fn a_similar_but_harmless_key_is_ignored() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");
    store.save(&account).expect("the save should succeed");

    let extended = format!(
        r#"{{"account_id":"{}","display_name":"Steve","kind":"microsoft","access_token_expires_at":12345,"future_field":true}}"#,
        account.account_id.as_uuid()
    );
    fs::write(
        root.path()
            .join(format!("{}.json", account.account_id.as_uuid())),
        extended,
    )
    .expect("the extended document should be writable");

    let loaded = store
        .load(account.account_id)
        .expect("unknown fields should be ignored")
        .expect("the account should be present");
    assert_eq!(loaded.display_name, "Steve");
    assert_eq!(loaded.refresh, None);
}

#[test]
fn a_document_claiming_another_account_is_refused() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");
    store.save(&account).expect("the save should succeed");

    let other = AccountId::new();
    let copied = format!(
        r#"{{"account_id":"{}","display_name":"Mallory","kind":"microsoft"}}"#,
        other.as_uuid()
    );
    fs::write(
        root.path()
            .join(format!("{}.json", account.account_id.as_uuid())),
        copied,
    )
    .expect("the copied document should be writable");

    let error = store
        .load(account.account_id)
        .expect_err("an identity mismatch must be refused");
    assert!(matches!(error, StoreError::UntrustedContent));
}

#[test]
fn a_malformed_document_is_a_decode_error() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");
    store.save(&account).expect("the save should succeed");
    fs::write(
        root.path()
            .join(format!("{}.json", account.account_id.as_uuid())),
        "{ not json",
    )
    .expect("the malformed document should be writable");

    let error = store
        .load(account.account_id)
        .expect_err("a malformed document must not load");
    assert!(matches!(error, StoreError::Decode(_)));
}

#[test]
fn loading_an_absent_account_returns_none() {
    let root = TempRoot::new();

    // Even before the root exists, a missing account is simply absent.
    let missing_root = TempRoot::new();
    let uncreated = missing_root.path().join("nested").join("store");
    let store = FileCredentialStore::new(&uncreated);
    assert_eq!(
        store
            .load(AccountId::new())
            .expect("absence is not an error"),
        None
    );

    let store = root.store();
    assert_eq!(
        store
            .load(AccountId::new())
            .expect("absence is not an error"),
        None
    );
}

#[test]
fn removing_an_absent_account_is_a_no_op() {
    let root = TempRoot::new();

    root.store()
        .remove(AccountId::new())
        .expect("removing an absent account is fine");

    // The root now exists but holds no matching document.
    root.store()
        .remove(AccountId::new())
        .expect("removing an absent account is fine");
}

#[test]
fn removing_a_stored_account_deletes_its_document() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");
    store.save(&account).expect("the save should succeed");

    store
        .remove(account.account_id)
        .expect("the removal should succeed");

    assert!(
        !root
            .path()
            .join(format!("{}.json", account.account_id.as_uuid()))
            .exists()
    );
    assert_eq!(
        store
            .load(account.account_id)
            .expect("absence is not an error"),
        None
    );
    assert!(store.list().expect("the list should succeed").is_empty());
}

#[test]
fn listing_reports_every_saved_account_and_skips_junk() {
    let root = TempRoot::new();
    let store = root.store();
    let first = microsoft_account("Steve");
    let second = microsoft_account("Alex");
    store.save(&first).expect("the save should succeed");
    store.save(&second).expect("the save should succeed");

    fs::write(root.path().join("notes.txt"), "hello").expect("the junk file should be writable");
    fs::write(root.path().join("not-a-uuid.json"), "{}").expect("the junk file should be writable");
    fs::write(root.path().join("leftover.tmp"), "{}").expect("the junk file should be writable");

    let mut listed = store.list().expect("junk must not fail the listing");
    listed.sort_by_key(AccountId::as_uuid);
    let mut expected = vec![first.account_id, second.account_id];
    expected.sort_by_key(AccountId::as_uuid);

    assert_eq!(listed, expected);
}

#[test]
fn saving_twice_overwrites_the_previous_document() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");

    store.save(&account).expect("the first save should succeed");
    let renamed = StoredCredential::metadata_only(account.account_id, "Alex", AccountKind::Offline)
        .with_xuid(Some("123".to_owned()));
    store
        .save(&renamed)
        .expect("the second save should succeed");

    let loaded = store
        .load(account.account_id)
        .expect("the load should succeed")
        .expect("the account should be present");
    assert_eq!(loaded, renamed);
    assert_eq!(loaded.display_name, "Alex");
    assert_eq!(
        loaded.kind,
        AccountKind::Offline,
        "the second save must win entirely, not merge"
    );
    assert_eq!(store.list().expect("the list should succeed").len(), 1);
}

#[test]
fn error_messages_never_reveal_the_store_location() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");
    store.save(&account).expect("the save should succeed");

    fs::write(
        root.path()
            .join(format!("{}.json", account.account_id.as_uuid())),
        r#"{"account_id":"","display_name":"Steve","kind":"microsoft"}"#,
    )
    .expect("the malformed document should be writable");

    let error = store
        .load(account.account_id)
        .expect_err("the document must not load");
    let rendered = error.to_string();
    let location = root.path().to_string_lossy().into_owned();

    assert!(!rendered.contains(&location));
    assert!(!rendered.contains(account.account_id.as_uuid().to_string().as_str()));
    assert!(!rendered.contains(SECRET));
}

#[test]
fn debug_of_a_stored_credential_does_not_leak_the_token() {
    let credential = microsoft_account("Steve").with_refresh(RefreshToken::new(SECRET));

    let rendered = format!("{credential:?}");

    assert!(!rendered.contains(SECRET));
    assert!(!rendered.contains("M.R3_BAY"));
    assert!(rendered.contains("RefreshToken(****1234)"));
    assert!(rendered.contains("Steve"));
}

#[test]
fn a_metadata_only_credential_starts_without_a_token() {
    let account = AccountId::new();
    let credential = StoredCredential::metadata_only(account, "Steve", AccountKind::Microsoft);

    assert_eq!(credential.account_id, account);
    assert_eq!(credential.refresh, None);
    assert_eq!(credential.xuid, None);
}

#[test]
fn the_document_is_named_after_the_account_uuid() {
    let root = TempRoot::new();
    let store = root.store();
    let account = microsoft_account("Steve");

    store.save(&account).expect("the save should succeed");

    let name = format!("{}.json", account.account_id.as_uuid());
    assert!(root.path().join(&name).is_file());
    assert_eq!(
        Path::new(OsStr::new(&name))
            .file_stem()
            .and_then(OsStr::to_str),
        Some(account.account_id.as_uuid().to_string().as_str())
    );
}
