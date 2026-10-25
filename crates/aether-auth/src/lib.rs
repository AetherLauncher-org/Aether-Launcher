//! Microsoft account authentication and credential handling for Aether Launcher.
//!
//! The crate keeps its core logic free of network, filesystem and timer dependencies:
//! HTTP goes through [`http::HttpTransport`], persistence through
//! [`store::CredentialStore`], and time is expressed as [`std::time::Duration`] or
//! [`std::time::SystemTime`] values supplied by the caller. Every test therefore runs
//! offline and deterministically.
//!
//! The sign-in chain is split into four stages that mirror the endpoints the real
//! services expose:
//!
//! 1. [`device_code`] polls the Microsoft identity platform for an OAuth 2.0 token.
//! 2. [`xbox`] trades that token for an Xbox Live (XBL) token and an XSTS token.
//! 3. [`minecraft`] trades the XSTS token for a Minecraft access token and profile.
//! 4. [`store`] decides whether anything is allowed to touch the disk.
//!
//! [`offline`] covers the disconnected account path and [`token`] holds the redacting
//! newtypes that make accidental token logging hard.

pub mod device_code;
pub mod http;
pub mod minecraft;
pub mod offline;
pub mod secure_store;
pub mod store;
pub mod token;
pub mod xbox;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(Uuid);

impl AccountId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wraps an existing UUID. Offline accounts derive their identity from the
    /// player name, so the value is not random there.
    #[must_use]
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Microsoft,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountProfile {
    pub id: AccountId,
    pub display_name: String,
    pub kind: AccountKind,
    /// Xbox user id reported by XSTS. Absent for offline accounts and for profiles
    /// serialized before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xuid: Option<String>,
}

impl AccountProfile {
    /// Builds an offline profile from an already derived identity.
    #[must_use]
    pub fn offline(id: AccountId, display_name: impl Into<String>) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            kind: AccountKind::Offline,
            xuid: None,
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
