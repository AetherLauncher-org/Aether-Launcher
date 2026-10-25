//! Credential metadata persistence. Tokens must be protected by a platform vault.
//!
//! This file-backed implementation stores account IDs, display names, kinds and
//! XUIDs only. All refresh/access/Xbox/Minecraft tokens remain in memory. The
//! legacy `RefreshTokenPersistence::Allow` option is retained for API compatibility,
//! but attempting to persist a refresh token is refused, not written in plaintext.
//! Plaintext tokens in pre-existing credential files are also refused on load.
//! Use an OS-protected credential store before implementing remember-me behavior.
//!
//! Writes of metadata are atomic, not encrypted. Error messages never include
//! account secrets or their filesystem paths.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::token::RefreshToken;
use crate::{AccountId, AccountKind};

/// Persistence intent. This file store never writes plaintext credentials.
/// Until a platform credential vault is available, `Allow` fails closed for
/// accounts carrying refresh tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RefreshTokenPersistence {
    /// No token is ever written. Saving strips the refresh token and keeps the account
    /// metadata only.
    #[default]
    Deny,
    /// Reserved for a future protected credential store. File-backed token
    /// persistence with this policy returns `StoreError::InsecurePersistence`.
    Allow,
}

/// The on-disk shape of one account. **Never** contains an XSTS or Minecraft access
/// token: there is no field for one.
///
/// `Debug` is derived, and that is safe only because [`RefreshToken`] redacts itself
/// (`RefreshToken(****1234)`), so an accidental `{:?}` cannot reveal the secret.
///
/// `Serialize` is deliberately **not** implemented, not even derived. Deriving it would
/// make the storable shape identical to the in-memory shape and hand every holder of a
/// `StoredCredential` a one-liner that writes the clear-text token anywhere. Encoding
/// goes exclusively through the private `WireCredential` inside
/// [`FileCredentialStore::save`], which applies [`RefreshTokenPersistence`] first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCredential {
    pub account_id: AccountId,
    pub display_name: String,
    pub kind: AccountKind,
    /// In-memory refresh token; never persisted by this file store.
    pub refresh: Option<RefreshToken>,
    pub xuid: Option<String>,
}

impl StoredCredential {
    /// Builds a metadata-only credential; `refresh` starts out as `None`, which is what a
    /// denying store keeps.
    #[must_use]
    pub fn metadata_only(
        account_id: AccountId,
        display_name: impl Into<String>,
        kind: AccountKind,
    ) -> Self {
        Self {
            account_id,
            display_name: display_name.into(),
            kind,
            refresh: None,
            xuid: None,
        }
    }

    /// Attaches a refresh token in memory. Whether it survives a save is decided by the
    /// store, not by this builder.
    #[must_use]
    pub fn with_refresh(mut self, refresh: RefreshToken) -> Self {
        self.refresh = Some(refresh);
        self
    }

    /// Attaches the Xbox user id, if the profile has one.
    #[must_use]
    pub fn with_xuid(mut self, xuid: Option<String>) -> Self {
        self.xuid = xuid;
        self
    }
}

/// The persistence seam. Implementations must honour [`RefreshTokenPersistence`]; the
/// trait itself cannot enforce it, so every implementation documents how it does.
pub trait CredentialStore {
    /// Reads one account. A missing account is `Ok(None)`, not an error.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backend fails, when the document does not decode,
    /// or when it carries a token type this crate never persists.
    fn load(&self, account: AccountId) -> Result<Option<StoredCredential>, StoreError>;

    /// Saves (overwrites) one account, applying the implementation's persistence policy.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the credential cannot be encoded or the backend fails.
    fn save(&self, credential: &StoredCredential) -> Result<(), StoreError>;

    /// Removes one account. A missing account is not an error: removal is idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the backend fails for any reason other than absence.
    fn remove(&self, account: AccountId) -> Result<(), StoreError>;

    /// Lists every stored account id.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the directory itself cannot be listed.
    fn list(&self) -> Result<Vec<AccountId>, StoreError>;
}

/// A directory-backed store with one `<account_id>.json` document per account.
///
/// # Security
///
/// Only metadata is written; an opt-in token save fails closed without an OS vault.
pub struct FileCredentialStore {
    root: PathBuf,
    persistence: RefreshTokenPersistence,
}

impl FileCredentialStore {
    /// Creates a store rooted at `root`. Nothing is created until the first save, and
    /// nothing but account metadata is written until refresh persistence is opted into.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            persistence: RefreshTokenPersistence::Deny,
        }
    }

    /// Chooses whether refresh tokens may be written to the disk.
    #[must_use]
    pub fn with_refresh_persistence(mut self, persistence: RefreshTokenPersistence) -> Self {
        self.persistence = persistence;
        self
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn persistence(&self) -> RefreshTokenPersistence {
        self.persistence
    }

    /// The document path for `account`. Only the UUID is used, so a display name can
    /// never escape the root through a path separator.
    fn path_for(&self, account: AccountId) -> PathBuf {
        self.root.join(format!("{}.json", account.as_uuid()))
    }

    /// A sibling temporary path used for the atomic write. The process-local counter and
    /// the randomness in the UUID keep two concurrent writers from sharing it.
    fn temporary_path(&self, account: AccountId) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        self.root.join(format!(
            "{}.{}.{sequence}.tmp",
            account.as_uuid(),
            std::process::id()
        ))
    }

    /// Writes `bytes` to `target` via a flushed temporary file and a rename.
    ///
    /// The rename stays inside `root`, which makes it atomic on every supported platform.
    fn write_atomically(
        &self,
        target: &Path,
        temporary: &Path,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        let mut file = File::create(temporary).map_err(StoreError::Write)?;
        // On failure the temporary file is removed so a failed save leaves no debris
        // that a later crash-recovery pass might mistake for a credential.
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(temporary);
            return Err(StoreError::Write(error));
        }
        drop(file);
        if let Err(error) = fs::rename(temporary, target) {
            let _ = fs::remove_file(temporary);
            return Err(StoreError::Write(error));
        }
        Ok(())
    }
}

impl CredentialStore for FileCredentialStore {
    fn load(&self, account: AccountId) -> Result<Option<StoredCredential>, StoreError> {
        let bytes = match fs::read(self.path_for(account)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(StoreError::Read(error)),
        };

        // Two passes on purpose. `serde_json::from_slice` fails on malformed UTF-8 while
        // `from_str` is infallible for the same bytes, so scanning through `from_str`
        // guarantees the refusal below cannot be bypassed with odd bytes; the decode then
        // runs on the scanned text. The scan is cheap relative to any I/O.
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(value) = serde_json::from_str::<Value>(&text)
            && !document_is_trusted(&value)
        {
            return Err(StoreError::UntrustedContent);
        }
        let wire: WireCredential = serde_json::from_str(&text).map_err(StoreError::Decode)?;

        let stored = wire.account_id;
        if stored != account {
            // The document was renamed or copied over a different account's file. Trust
            // the filename that was asked for, not the claim inside the document.
            return Err(StoreError::UntrustedContent);
        }

        if wire.refresh_token.is_some() {
            return Err(StoreError::UntrustedContent);
        }
        Ok(Some(wire.into_credential(self.persistence)))
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), StoreError> {
        if self.persistence == RefreshTokenPersistence::Allow && credential.refresh.is_some() {
            return Err(StoreError::InsecurePersistence);
        }
        fs::create_dir_all(&self.root).map_err(StoreError::Write)?;

        let wire = WireCredential::from_credential(credential, self.persistence);
        let bytes = serde_json::to_vec_pretty(&wire).map_err(StoreError::Encode)?;

        self.write_atomically(
            &self.path_for(credential.account_id),
            &self.temporary_path(credential.account_id),
            &bytes,
        )
    }

    fn remove(&self, account: AccountId) -> Result<(), StoreError> {
        match fs::remove_file(self.path_for(account)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StoreError::Write(error)),
        }
    }

    fn list(&self) -> Result<Vec<AccountId>, StoreError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            // Nothing was ever saved, so the store is empty rather than broken.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Read(error)),
        };

        let mut accounts = Vec::new();
        for entry in entries {
            let entry = entry.map_err(StoreError::Read)?;
            // A leftover temporary file or an unrelated document is skipped, not an error.
            let Some(account) = account_from_file_name(&entry.file_name()) else {
                continue;
            };
            accounts.push(account);
        }
        accounts.sort_by_key(AccountId::as_uuid);
        accounts.dedup();
        Ok(accounts)
    }
}

/// Extracts the account id from a `<uuid>.json` file name. Anything else (temporary
/// files, notes, directories) yields `None`.
fn account_from_file_name(name: &std::ffi::OsStr) -> Option<AccountId> {
    let stem = Path::new(name).file_stem()?.to_str()?;
    Uuid::parse_str(stem).ok().map(AccountId::from_uuid)
}

/// The exact wire shape of a stored document.
///
/// This is the only type in the crate that is serialized from credential material, and it
/// is private: nothing outside this module can encode a `StoredCredential` behind the
/// store's back. The field list is the whitelist — there is no field for a Minecraft,
/// XSTS or XBL token, so none can be produced here.
///
/// It deliberately implements no `Debug`: its `refresh_token` field is clear text, so a
/// single `tracing::debug!("{wire:?}")` would print a live credential.
#[derive(Serialize, Deserialize)]
struct WireCredential {
    account_id: AccountId,
    display_name: String,
    kind: AccountKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    xuid: Option<String>,
    /// Legacy field: accepted by the decoder only to reject unsafe old documents.
    /// New metadata files always omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

impl WireCredential {
    /// File-backed persistence intentionally never serializes tokens.
    fn from_credential(credential: &StoredCredential, _policy: RefreshTokenPersistence) -> Self {
        Self {
            account_id: credential.account_id,
            display_name: credential.display_name.clone(),
            kind: credential.kind.clone(),
            xuid: credential.xuid.clone().filter(|xuid| !xuid.is_empty()),
            refresh_token: None,
        }
    }

    /// File-backed decoding never rehydrates a long-lived token.
    fn into_credential(self, _policy: RefreshTokenPersistence) -> StoredCredential {
        StoredCredential {
            account_id: self.account_id,
            display_name: self.display_name,
            kind: self.kind,
            refresh: None,
            xuid: self.xuid,
        }
    }
}

/// Field names that identify a token this crate never persists. Matched as a whole name,
/// case-insensitively, so `access_token`, `AccessToken`, `xsts_token`, `minecraft_token`
/// and `xbl_token` are all refused while `access_token_expires_at` simply remains an
/// unknown key that serde ignores.
const FORBIDDEN_TOKEN_KEYS: [&str; 4] =
    ["access_token", "xsts_token", "minecraft_token", "xbl_token"];

/// Recursively rejects a document that carries a forbidden token field.
///
/// The scan walks objects (and the objects inside arrays) so a token cannot hide in a
/// nested structure, and resolves one level of `\u` escapes in an object key so
/// `"access\u005ftoken"` cannot slip past the comparison.
fn document_is_trusted(value: &Value) -> bool {
    match value {
        Value::Object(object) => object_is_trusted(object),
        Value::Array(items) => items.iter().all(document_is_trusted),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
    }
}

fn object_is_trusted(object: &Map<String, Value>) -> bool {
    object.iter().all(|(key, value)| {
        let normalized = unescape_u16(key);
        let candidate = normalized.trim().to_ascii_lowercase();
        if FORBIDDEN_TOKEN_KEYS.contains(&candidate.as_str()) {
            return false;
        }
        document_is_trusted(value)
    })
}

/// Resolves JSON `\uXXXX` escapes in a key. Only escapes are touched; everything else is
/// copied verbatim, which is enough to compare against a fixed set of ASCII names.
fn unescape_u16(key: &str) -> String {
    fn unit_at(digits: &str) -> Option<u32> {
        u32::from_str_radix(digits, 16).ok()
    }

    let mut chars = key.chars().peekable();
    let mut out = String::with_capacity(key.len());
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let mut digits = String::with_capacity(4);
                for _ in 0..4 {
                    match chars.peek() {
                        Some(next) => {
                            digits.push(*next);
                            chars.next();
                        }
                        None => break,
                    }
                }
                match (digits.len(), unit_at(&digits)) {
                    (4, Some(unit)) => match char::from_u32(unit) {
                        Some(decoded) => out.push(decoded),
                        // Mirror serde_json: a lone surrogate is a replacement character.
                        None => out.push(char::REPLACEMENT_CHARACTER),
                    },
                    // Not a valid escape; keep the source text so the comparison still
                    // sees a literal backslash rather than silently dropping characters.
                    _ => {
                        out.push('\\');
                        out.push('u');
                        out.push_str(&digits);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Failures of the credential store.
///
/// Every message is a fixed string: no path, no account id and no token is ever
/// formatted into it. The underlying [`std::io::Error`] or [`serde_json::Error`] is kept
/// as the `#[source]` for diagnostics, where its position information is enough to debug
/// a malformed document without echoing its contents.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("failed to read the credential store")]
    Read(#[source] io::Error),
    #[error("failed to write the credential store")]
    Write(#[source] io::Error),
    #[error("failed to decode a stored credential")]
    Decode(#[source] serde_json::Error),
    #[error("failed to encode a stored credential")]
    Encode(#[source] serde_json::Error),
    #[error("the stored credential contains a token type this store never persists")]
    UntrustedContent,
    #[error("plaintext refresh token persistence requires an OS credential vault")]
    InsecurePersistence,
    #[error("the operating system credential store is unavailable")]
    ProtectedStoreUnavailable,
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
