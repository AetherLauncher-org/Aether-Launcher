//! Opaque token newtypes whose `Debug` and `Display` never print the secret.
//!
//! The wrapping types carry no `serde` traits on purpose: they enter the crate through
//! the JSON parsing helpers in [`crate::device_code`], [`crate::xbox`] and
//! [`crate::minecraft`], and they only leave it through `expose()` when a request body
//! is built. [`crate::store`] decides separately whether any of them may touch the disk.

use std::fmt;
use std::time::{Duration, SystemTime};

use thiserror::Error;

/// Formats a secret as `{label}(****{tail})`, showing at most the last four characters.
/// Short secrets are fully hidden.
fn redacted(label: &str, secret: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let count = secret.chars().count();
    if count >= 12 {
        let tail: String = secret.chars().skip(count - 4).collect();
        write!(f, "{label}(****{tail})")
    } else {
        write!(f, "{label}(****)")
    }
}

/// Generates a redacting token newtype with a uniform API.
macro_rules! secret_newtype {
    ($name:ident, $label:literal) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(secret: impl Into<String>) -> Self {
                Self(secret.into())
            }

            /// The raw secret. Only use it while building a request body; never log it.
            #[must_use]
            pub fn expose(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                redacted($label, &self.0, f)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                redacted($label, &self.0, f)
            }
        }
    };
}

secret_newtype!(AccessToken, "AccessToken");
secret_newtype!(RefreshToken, "RefreshToken");
secret_newtype!(XstsToken, "XstsToken");
secret_newtype!(MinecraftToken, "MinecraftToken");

impl AccessToken {
    /// Instantiation from the OAuth 2.0 token response, rejecting an empty value.
    pub fn parse(value: impl Into<String>) -> Result<Self, TokenError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TokenError::EmptySecret("access token"));
        }
        Ok(Self(value))
    }
}

impl RefreshToken {
    /// Instantiation from an OAuth 2.0 token response. A missing refresh token is not an
    /// error: some scopes and some policies omit it, and the caller decides whether that
    /// is acceptable.
    #[must_use]
    pub fn from_optional(value: Option<String>) -> Option<Self> {
        value.filter(|value| !value.is_empty()).map(Self)
    }

    /// Instantiation that rejects an empty value; used by the credential store.
    pub fn parse(value: impl Into<String>) -> Result<Self, TokenError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TokenError::EmptySecret("refresh token"));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("the {0} is empty")]
    EmptySecret(&'static str),
}

/// Captures when a token stops being valid. Expiry is expressed relative to an
/// injected `issued_at` so tests never depend on the wall clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenLifetime {
    issued_at: SystemTime,
    expires_in: Duration,
}

impl TokenLifetime {
    #[must_use]
    pub fn new(issued_at: SystemTime, expires_in: Duration) -> Self {
        Self {
            issued_at,
            expires_in,
        }
    }

    #[must_use]
    pub fn issued_at(&self) -> SystemTime {
        self.issued_at
    }

    #[must_use]
    pub fn expires_in(&self) -> Duration {
        self.expires_in
    }

    /// The exact moment the token expires.
    #[must_use]
    pub fn expires_at(&self) -> SystemTime {
        self.issued_at + self.expires_in
    }

    /// Whether the token is expired at `now`, applying `skew` as an early-expiry
    /// allowance that mimics clock drift between the client and the token service.
    #[must_use]
    pub fn is_expired_at(&self, now: SystemTime, skew: Duration) -> bool {
        self.expires_at()
            .checked_sub(skew)
            .is_none_or(|deadline| now >= deadline)
    }
}

/// The tokens that come back from the OAuth 2.0 token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrosoftTokens {
    pub access: AccessToken,
    pub refresh: Option<RefreshToken>,
    pub lifetime: TokenLifetime,
}

impl MicrosoftTokens {
    #[must_use]
    pub fn new(
        access: AccessToken,
        refresh: Option<RefreshToken>,
        lifetime: TokenLifetime,
    ) -> Self {
        Self {
            access,
            refresh,
            lifetime,
        }
    }

    /// `true` when the access token has expired and no refresh token is available to
    /// renew it, i.e. the user must sign in again.
    #[must_use]
    pub fn requires_sign_in_at(&self, now: SystemTime, skew: Duration) -> bool {
        self.refresh.is_none() && self.lifetime.is_expired_at(now, skew)
    }
}

/// The result of the Minecraft login step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftSession {
    pub access: MinecraftToken,
    pub lifetime: TokenLifetime,
    /// The player name as reported by the login endpoint; usually the name attached to
    /// the Xbox account, potentially changed later via `/minecraft/profile`.
    pub username: Option<String>,
}

impl MinecraftSession {
    #[must_use]
    pub fn new(access: MinecraftToken, lifetime: TokenLifetime, username: Option<String>) -> Self {
        Self {
            access,
            lifetime,
            username,
        }
    }
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
