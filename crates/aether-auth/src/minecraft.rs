//! The third stage of the sign-in chain: Minecraft services.
//!
//! The XSTS token from [`crate::xbox`] is traded for a Minecraft access token
//! (`login_with_xbox`), then that token is used to answer two questions the launcher cares
//! about: does the account own the game ([`Entitlements`]) and what is the player called
//! ([`MinecraftProfile`])?
//!
//! Every function in this module is pure: the `build_*` helpers produce an
//! [`HttpRequest`], the `parse_*` helpers interpret an [`HttpResponse`], and the caller
//! owns the transport. That keeps the whole module testable offline.
//!
//! # Security
//!
//! [`MinecraftError`] never carries a token, a full URL or a response body. The only
//! variables that appear in error messages are status codes and static strings, so an
//! error can be logged verbatim.

use std::time::{Duration, SystemTime};

use serde::Deserialize;
use uuid::Uuid;

use crate::http::HttpRequest;
use crate::token::{MinecraftSession, MinecraftToken, TokenLifetime, XstsToken};

/// Trades an XSTS token for a Minecraft access token.
pub const LOGIN_WITH_XBOX_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
/// Lists the store purchases attached to an account.
pub const ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
/// Reports the player identity (UUID and name) attached to an account.
pub const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// Entitlement names that grant Java Edition access. Either one is sufficient.
const GAME_ENTITLEMENTS: [&str; 2] = ["product_minecraft", "game_minecraft"];

/// The prefix every `identityToken` must carry, exactly.
const XBL_PREFIX: &str = "XBL3.0 x=";

/// The head of the `authorization` header value.
const BEARER_PREFIX: &str = "Bearer ";

/// Failures reported by the Minecraft services stage.
#[derive(Debug, thiserror::Error)]
pub enum MinecraftError {
    #[error(
        "the Minecraft account is not entitled to the game; purchase it on a Microsoft account first"
    )]
    NotEntitled,
    #[error(
        "this account has no Minecraft profile yet; create a player name in the Minecraft launcher first"
    )]
    ProfileNotFound,
    #[error("Minecraft services rejected the token")]
    Unauthorized,
    #[error("Minecraft services returned an unexpected status {status}")]
    UnexpectedStatus { status: u16 },
    #[error("the Minecraft services response is missing required fields")]
    MalformedResponse,
}

impl MinecraftError {
    /// Maps an unsuccessful status to the error the flows report.
    ///
    /// `401` and `403` mean the token or the account behind it is not accepted, which is
    /// reported uniformly as [`MinecraftError::Unauthorized`]. Everything else keeps the
    /// status so the caller can tell a server fault from a client fault.
    #[must_use]
    fn from_status(status: u16) -> Self {
        match status {
            401 | 403 => Self::Unauthorized,
            other => Self::UnexpectedStatus { status: other },
        }
    }
}

// --------------------------------------------------------------------------------------
// Wire types
// --------------------------------------------------------------------------------------
//
// The token newtypes deliberately do not implement `serde`, so the JSON shapes are
// modelled with private structs that hold plain `String`s and are converted afterwards.

#[derive(Debug, Deserialize)]
struct LoginResponse {
    access_token: String,
    expires_in: u64,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EntitlementsResponse {
    #[serde(default)]
    items: Vec<EntitlementItem>,
}

#[derive(Debug, Deserialize)]
struct EntitlementItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    signature: String,
}

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    id: String,
    name: String,
}

// --------------------------------------------------------------------------------------
// Step 1: XSTS -> Minecraft access token
// --------------------------------------------------------------------------------------

/// Builds the `login_with_xbox` request.
///
/// The body is a single `identityToken` of the form `XBL3.0 x=<user_hash>;<xsts>`; the
/// separators (`x=`, `;`) are part of the protocol and must not be altered.
#[must_use]
pub fn build_login_request(user_hash: &str, xsts: &XstsToken) -> HttpRequest {
    let identity_token = format!("{XBL_PREFIX}{user_hash};{}", xsts.expose());
    let body = serde_json::json!({ "identityToken": identity_token }).to_string();
    HttpRequest::post_json(LOGIN_WITH_XBOX_URL, body)
}

/// Parses the `login_with_xbox` response into a [`MinecraftSession`].
///
/// `issued_at` is supplied by the caller instead of being read from the wall clock, so
/// the resulting [`TokenLifetime`] is deterministic under test.
///
/// # Errors
///
/// - `401`/`403` → [`MinecraftError::Unauthorized`];
/// - any other non-`2xx` status → [`MinecraftError::UnexpectedStatus`];
/// - a body without `access_token`/`expires_in`, whose token is empty, or whose
///   `expires_in` is zero → [`MinecraftError::MalformedResponse`].
pub fn parse_login_response(
    status: u16,
    body: &str,
    issued_at: SystemTime,
) -> Result<MinecraftSession, MinecraftError> {
    if !is_success(status) {
        return Err(MinecraftError::from_status(status));
    }
    let parsed: LoginResponse =
        serde_json::from_str(body).map_err(|_| MinecraftError::MalformedResponse)?;
    // A zero lifetime would hand the caller a session that is expired on arrival, which is
    // never what the login endpoint means; the device code flow rejects it the same way.
    if parsed.access_token.is_empty() || parsed.expires_in == 0 {
        return Err(MinecraftError::MalformedResponse);
    }
    let lifetime = TokenLifetime::new(issued_at, Duration::from_secs(parsed.expires_in));
    let username = parsed.username.filter(|name| !name.is_empty());
    Ok(MinecraftSession::new(
        MinecraftToken::new(parsed.access_token),
        lifetime,
        username,
    ))
}

// --------------------------------------------------------------------------------------
// Step 2: entitlements
// --------------------------------------------------------------------------------------

/// One entry of the `mcstore` entitlement list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entitlement {
    pub name: String,
    /// The opaque signature the service attaches to the entry. Preserved verbatim
    /// because the game client may pass it on; it is meaningless to the launcher.
    pub signature: String,
}

/// The entitlement list for an account.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entitlements {
    pub items: Vec<Entitlement>,
}

impl Entitlements {
    /// `true` when the account owns Java Edition, i.e. it holds `product_minecraft` or
    /// `game_minecraft`. `product_minecraft` is the regular purchase while
    /// `game_minecraft` is the older/alternate entitlement; either grants the game.
    #[must_use]
    pub fn owns_minecraft(&self) -> bool {
        self.items
            .iter()
            .any(|item| GAME_ENTITLEMENTS.contains(&item.name.as_str()))
    }

    /// `true` when the response listed no entries at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The names of the entitlements, useful for diagnostics that must stay free of
    /// signatures.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|item| item.name.as_str())
    }
}

/// Parses the `mcstore` response.
///
/// A missing or empty `items` array is a valid answer meaning "owns nothing"; only a
/// body that is not JSON at all is treated as malformed. A missing per-item `signature`
/// becomes an empty string.
///
/// # Errors
///
/// - `401`/`403` → [`MinecraftError::Unauthorized`];
/// - any other non-`2xx` status → [`MinecraftError::UnexpectedStatus`];
/// - a body that does not match the documented shape →
///   [`MinecraftError::MalformedResponse`].
pub fn parse_entitlements(status: u16, body: &str) -> Result<Entitlements, MinecraftError> {
    if !is_success(status) {
        return Err(MinecraftError::from_status(status));
    }
    let parsed: EntitlementsResponse =
        serde_json::from_str(body).map_err(|_| MinecraftError::MalformedResponse)?;
    Ok(Entitlements {
        items: parsed
            .items
            .into_iter()
            .map(|item| Entitlement {
                name: item.name,
                signature: item.signature,
            })
            .collect(),
    })
}

/// Builds the entitlement `GET`, authenticated with the Minecraft access token.
#[must_use]
pub fn build_entitlements_request(token: &MinecraftToken) -> HttpRequest {
    bearer_request(ENTITLEMENTS_URL, token)
}

// --------------------------------------------------------------------------------------
// Step 3: profile
// --------------------------------------------------------------------------------------

/// The player identity attached to a Minecraft account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftProfile {
    pub id: Uuid,
    pub name: String,
}

impl MinecraftProfile {
    /// The canonical hyphenated UUID form, e.g. `069a79f4-44e9-4726-a5be-fca90e38aaf5`.
    #[must_use]
    pub fn dashed_id(&self) -> String {
        self.id.to_string()
    }

    /// The 32-character form the game protocol uses, without hyphens.
    #[must_use]
    pub fn undashed_id(&self) -> String {
        self.id.simple().to_string()
    }
}

/// Parses the `minecraft/profile` response.
///
/// The service reports `id` as 32 hex digits without hyphens. The value is re-hyphenated
/// before parsing rather than relying on permissive UUID parsing, so the accepted shape
/// is explicit and testable.
///
/// # Errors
///
/// - `404` → [`MinecraftError::ProfileNotFound`], the normal answer for an account that
///   has never chosen a player name;
/// - `401`/`403` → [`MinecraftError::Unauthorized`];
/// - any other non-`2xx` status → [`MinecraftError::UnexpectedStatus`];
/// - a body without a well-formed `id` or `name` → [`MinecraftError::MalformedResponse`].
pub fn parse_profile(status: u16, body: &str) -> Result<MinecraftProfile, MinecraftError> {
    if status == 404 {
        return Err(MinecraftError::ProfileNotFound);
    }
    if !is_success(status) {
        return Err(MinecraftError::from_status(status));
    }
    let parsed: ProfileResponse =
        serde_json::from_str(body).map_err(|_| MinecraftError::MalformedResponse)?;
    if parsed.name.is_empty() {
        return Err(MinecraftError::MalformedResponse);
    }
    let id = parse_minecraft_uuid(&parsed.id).ok_or(MinecraftError::MalformedResponse)?;
    Ok(MinecraftProfile {
        id,
        name: parsed.name,
    })
}

/// Builds the profile `GET`, authenticated with the Minecraft access token.
#[must_use]
pub fn build_profile_request(token: &MinecraftToken) -> HttpRequest {
    bearer_request(PROFILE_URL, token)
}

// --------------------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------------------

/// `true` for `2xx`. The value comes from a transport, so a non-`2xx` answer is a
/// successful call with an unsuccessful status.
fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// A `GET` carrying `authorization: Bearer <token>`.
fn bearer_request(url: &str, token: &MinecraftToken) -> HttpRequest {
    HttpRequest::get(url).with_header(
        "authorization",
        format!("{BEARER_PREFIX}{}", token.expose()),
    )
}

/// Accepts the two id shapes the service is known to use: 32 bare hex digits and the
/// canonical hyphenated form.
fn parse_minecraft_uuid(raw: &str) -> Option<Uuid> {
    let trimmed = raw.trim();
    if trimmed.len() == 32 && trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let hyphenated = format!(
            "{}-{}-{}-{}-{}",
            &trimmed[0..8],
            &trimmed[8..12],
            &trimmed[12..16],
            &trimmed[16..20],
            &trimmed[20..32]
        );
        return Uuid::parse_str(&hyphenated).ok();
    }
    Uuid::parse_str(trimmed).ok()
}

#[cfg(test)]
#[path = "minecraft_tests.rs"]
mod tests;
