//! The second stage of the sign-in chain: Xbox Live.
//!
//! A Microsoft OAuth access token is traded for an Xbox Live (XBL) token
//! ([`build_xbl_authenticate_request`] / [`parse_xbl_response`]), and the XBL token is
//! then traded for an XSTS token scoped to the Minecraft relying party
//! ([`build_xsts_authorize_request`] / [`parse_xsts_response`]). The XSTS token and the
//! user hash are exactly what [`crate::minecraft`] needs next.
//!
//! Both steps are pure: the `build_*` helpers produce an [`HttpRequest`], the `parse_*`
//! helpers interpret an [`HttpResponse`], and the caller owns the transport. Every test
//! therefore runs offline.
//!
//! # Security
//!
//! [`XboxError`] never carries a token, a full URL or a response body. The only values
//! that appear in an error message are status codes, the numeric `XErr` code Xbox Live
//! reports, and static strings, so an error can be logged verbatim. [`XblToken`] follows
//! the newtypes in [`crate::token`] and redacts itself in `Debug` and `Display`.

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::http::HttpRequest;
use crate::token::{AccessToken, XstsToken};

/// Exchanges a Microsoft OAuth access token for an Xbox Live token.
pub const XBL_AUTHENTICATE_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
/// Exchanges an Xbox Live token for an XSTS token.
pub const XSTS_AUTHORIZE_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
/// The relying party that grants access to the Minecraft services. It must be spelled
/// exactly like this; the service compares the value verbatim.
pub const MINECRAFT_RELYING_PARTY: &str = "rp://api.minecraftservices.com/";

/// Every XBL/XSTS call pins this protocol version in `x-xbl-contract-version`.
const XBL_CONTRACT_VERSION: &str = "1";
/// The `Properties.AuthMethod` the RPS flow uses.
const AUTH_METHOD: &str = "RPS";
/// The `Properties.SiteName` the RPS flow uses.
const SITE_NAME: &str = "user.auth.xboxlive.com";
/// The relying party of the first step: Xbox Live itself.
const XBOX_RELYING_PARTY: &str = "http://auth.xboxlive.com";
/// The only token type either endpoint issues.
const TOKEN_TYPE: &str = "JWT";
/// The sandbox every retail Minecraft player lives in.
const RETAIL_SANDBOX: &str = "RETAIL";
/// Microsoft requires the OAuth access token to be prefixed inside `RpsTicket`.
const RPS_TICKET_PREFIX: &str = "d=";

// Xbox Live reports account-state problems as numeric codes in the `XErr` field.
/// The account has not created an Xbox profile yet.
const XERR_NO_XBOX_PROFILE: u64 = 2_148_916_233;
/// Xbox Live is not offered in the account's region.
const XERR_REGION_UNSUPPORTED: u64 = 2_148_916_235;
/// The account belongs to a child and is not in a family group.
const XERR_CHILD_ACCOUNT: u64 = 2_148_916_238;

/// Formats a secret as `XblToken(****{tail})`, showing at most the last four
/// characters. Anything shorter than twelve characters is fully hidden.
fn redacted(secret: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let count = secret.chars().count();
    if count >= 12 {
        let tail: String = secret.chars().skip(count - 4).collect();
        write!(f, "XblToken(****{tail})")
    } else {
        write!(f, "XblToken(****)")
    }
}

/// An Xbox Live token.
///
/// Kept private to this module's API surface rather than folded into
/// [`crate::token::XstsToken`]: an XBL token is accepted only by the XSTS endpoint, while
/// an XSTS token is accepted only by the Minecraft endpoints. Mixing them up is a bug the
/// type system can catch.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct XblToken(String);

impl XblToken {
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The raw token. Only use it while building a request body; never log it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for XblToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        redacted(&self.0, f)
    }
}

impl fmt::Display for XblToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        redacted(&self.0, f)
    }
}

/// One `DisplayClaims.xui` entry. `uhs` (the user hash) is present on both steps. `xid` (the
/// Xbox user id) is **not** part of the answer for the Minecraft relying party, which is the
/// one the launcher asks for; it appears only for other relying parties.
#[derive(Debug, Deserialize)]
struct XuiClaim {
    uhs: String,
    #[serde(default)]
    xid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DisplayClaims {
    /// `xui` may legitimately be absent or empty; both mean "no claims", which is an
    /// error for this flow but must not abort deserialization.
    #[serde(default)]
    xui: Vec<XuiClaim>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(rename = "Token", default)]
    token: Option<String>,
    #[serde(rename = "DisplayClaims", default)]
    display_claims: Option<DisplayClaims>,
}

/// The result of the XBL step: a token plus the user hash that must accompany it later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XblSession {
    pub token: XblToken,
    pub user_hash: String,
}

/// The result of the XSTS step. `xuid` is the account's Xbox user id when the answer carries
/// one, which it does not for the Minecraft relying party; the launcher stores it on the
/// [`crate::AccountProfile`] only when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XstsSession {
    pub token: XstsToken,
    pub user_hash: String,
    pub xuid: Option<String>,
}

/// Failures reported by the Xbox Live stage.
#[derive(Debug, thiserror::Error)]
pub enum XboxError {
    #[error(
        "this Microsoft account does not have an Xbox profile yet; create one at xbox.com before signing in"
    )]
    NoXboxProfile,
    #[error("Xbox Live is not available in the region this account is registered in")]
    RegionUnsupported,
    #[error(
        "this is a child account; an adult must add it to a Microsoft family group before it can play online"
    )]
    ChildAccount,
    #[error("Xbox Live rejected the sign-in with error code {code}")]
    XstsCode { code: u64 },
    #[error("the Xbox service returned an unexpected status {status}")]
    UnexpectedStatus { status: u16 },
    #[error("the Xbox service response is missing required fields")]
    MalformedResponse,
}

/// Maps an `XErr` code to the user-facing error. Only the number survives: the body and
/// the redirect URL the service sends along with it are dropped.
fn from_xerr_code(code: u64) -> XboxError {
    match code {
        XERR_NO_XBOX_PROFILE => XboxError::NoXboxProfile,
        XERR_REGION_UNSUPPORTED => XboxError::RegionUnsupported,
        XERR_CHILD_ACCOUNT => XboxError::ChildAccount,
        other => XboxError::XstsCode { code: other },
    }
}

/// Extracts a numeric `XErr` from an error body.
///
/// Xbox Live has sent this field both as a JSON number and as a decimal string, so both
/// shapes are accepted. Anything else (including a body that is not JSON) yields `None`,
/// and the caller falls back to [`XboxError::UnexpectedStatus`].
fn xerr_code(body: &str) -> Option<u64> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    let raw = parsed.get("XErr")?;
    match raw {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// Turns an unsuccessful status into the error the flow reports.
fn error_for(status: u16, body: &str) -> XboxError {
    match xerr_code(body) {
        Some(code) => from_xerr_code(code),
        None => XboxError::UnexpectedStatus { status },
    }
}

/// `true` for the only status range either endpoint treats as an answer.
fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// Parses a successful token response and returns the token plus the first `xui` entry.
///
/// A body that is not JSON, lacks `Token`, or carries an empty/absent `xui` list is
/// [`XboxError::MalformedResponse`].
fn parse_token_response(status: u16, body: &str) -> Result<(String, XuiClaim), XboxError> {
    if !is_success(status) {
        return Err(error_for(status, body));
    }
    let parsed: TokenResponse =
        serde_json::from_str(body).map_err(|_| XboxError::MalformedResponse)?;
    let token = parsed
        .token
        .filter(|token| !token.is_empty())
        .ok_or(XboxError::MalformedResponse)?;
    let claim = parsed
        .display_claims
        .and_then(|claims| claims.xui.into_iter().next())
        .filter(|claim| !claim.uhs.is_empty())
        .ok_or(XboxError::MalformedResponse)?;
    Ok((token, claim))
}

// --------------------------------------------------------------------------------------
// Step 1: Microsoft access token -> XBL token
// --------------------------------------------------------------------------------------

/// Builds the `user/authenticate` request.
///
/// `Properties.RpsTicket` must carry the `d=` prefix in front of the OAuth access token;
/// Xbox Live rejects the bare token. The document is assembled with `serde_json` so the
/// token is escaped rather than concatenated.
#[must_use]
pub fn build_xbl_authenticate_request(access: &AccessToken) -> HttpRequest {
    let body = json!({
        "Properties": {
            "AuthMethod": AUTH_METHOD,
            "SiteName": SITE_NAME,
            "RpsTicket": format!("{RPS_TICKET_PREFIX}{}", access.expose()),
        },
        "RelyingParty": XBOX_RELYING_PARTY,
        "TokenType": TOKEN_TYPE,
    })
    .to_string();
    HttpRequest::post_json(XBL_AUTHENTICATE_URL, body)
        .with_header("x-xbl-contract-version", XBL_CONTRACT_VERSION)
}

/// Parses the `user/authenticate` response.
///
/// # Errors
///
/// - a non-`2xx` status carrying an `XErr` code → the matching [`XboxError`] variant;
/// - any other non-`2xx` status → [`XboxError::UnexpectedStatus`];
/// - a body without `Token` or with an empty/absent `DisplayClaims.xui` →
///   [`XboxError::MalformedResponse`].
pub fn parse_xbl_response(status: u16, body: &str) -> Result<XblSession, XboxError> {
    let (token, claim) = parse_token_response(status, body)?;
    Ok(XblSession {
        token: XblToken::new(token),
        user_hash: claim.uhs,
    })
}

// --------------------------------------------------------------------------------------
// Step 2: XBL token -> XSTS token
// --------------------------------------------------------------------------------------

/// Builds the `xsts/authorize` request.
///
/// The relying party is [`MINECRAFT_RELYING_PARTY`], which is what makes the resulting
/// XSTS token usable against the Minecraft services.
#[must_use]
pub fn build_xsts_authorize_request(xbl: &XblToken) -> HttpRequest {
    let body = json!({
        "Properties": {
            "SandboxId": RETAIL_SANDBOX,
            "UserTokens": [xbl.expose()],
        },
        "RelyingParty": MINECRAFT_RELYING_PARTY,
        "TokenType": TOKEN_TYPE,
    })
    .to_string();
    HttpRequest::post_json(XSTS_AUTHORIZE_URL, body)
        .with_header("x-xbl-contract-version", XBL_CONTRACT_VERSION)
}

/// Parses the `xsts/authorize` response.
///
/// # Errors
///
/// - a non-`2xx` status carrying a known `XErr` code → [`XboxError::NoXboxProfile`],
///   [`XboxError::RegionUnsupported`] or [`XboxError::ChildAccount`];
/// - a non-`2xx` status carrying an unknown code → [`XboxError::XstsCode`];
/// - any other non-`2xx` status → [`XboxError::UnexpectedStatus`];
/// - a body without `Token`, or with an empty/absent `DisplayClaims.xui` or `uhs` →
///   [`XboxError::MalformedResponse`]. A missing `xid` is not an error.
pub fn parse_xsts_response(status: u16, body: &str) -> Result<XstsSession, XboxError> {
    let (token, claim) = parse_token_response(status, body)?;
    let xuid = claim.xid.filter(|xuid| !xuid.is_empty());
    Ok(XstsSession {
        token: XstsToken::new(token),
        user_hash: claim.uhs,
        xuid,
    })
}

#[cfg(test)]
#[path = "xbox_tests.rs"]
mod tests;
