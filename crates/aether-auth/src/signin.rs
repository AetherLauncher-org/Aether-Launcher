//! Stages two to four glued together: from a Microsoft token to a Minecraft session.
//!
//! [`device_code`], [`xbox`] and [`minecraft`] each build requests and parse answers but
//! never talk to the network. This module is the small driver that runs them in order
//! through an injected [`HttpTransport`], so the whole chain is testable offline:
//!
//! 1. Microsoft access token → Xbox Live token ([`xbox::build_xbl_authenticate_request`]);
//! 2. Xbox Live token → XSTS token for the Minecraft relying party;
//! 3. XSTS token → Minecraft access token;
//! 4. entitlements (does the account own the game?) and the player profile.
//!
//! Errors never carry a response body, a token or a URL beyond its host, so a failure can
//! be logged verbatim.

use std::time::SystemTime;

use crate::device_code::{
    DeviceCodeEndpoints, DeviceCodeError, RefreshRequest, parse_refresh_response,
};
use crate::http::{HttpError, HttpRequest, HttpResponse, HttpTransport};
use crate::minecraft::{
    MinecraftError, MinecraftProfile, build_entitlements_request, build_login_request,
    build_profile_request, parse_entitlements, parse_login_response, parse_profile,
};
use crate::token::{MicrosoftTokens, MinecraftSession, RefreshToken};
use crate::xbox::{
    XboxError, build_xbl_authenticate_request, build_xsts_authorize_request, parse_xbl_response,
    parse_xsts_response,
};

/// Everything the launcher needs after a successful Minecraft sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftLogin {
    /// The access token the game is started with. Redacts itself.
    pub session: MinecraftSession,
    /// The player name and UUID.
    pub profile: MinecraftProfile,
    /// The Xbox user id from XSTS, stored on the account profile. XSTS leaves it out for the
    /// Minecraft relying party, so it is usually `None`.
    pub xuid: Option<String>,
}

/// Why a sign-in step failed.
#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error(transparent)]
    OAuth(#[from] DeviceCodeError),
    #[error(transparent)]
    Xbox(#[from] XboxError),
    #[error(transparent)]
    Minecraft(#[from] MinecraftError),
}

impl SignInError {
    /// `true` when retrying cannot help and the user has to go through the browser
    /// sign-in again: the refresh token was revoked or expired, or a freshly issued token
    /// was refused.
    #[must_use]
    pub fn requires_sign_in(&self) -> bool {
        match self {
            Self::OAuth(DeviceCodeError::OAuth { code }) => {
                matches!(code.as_str(), "invalid_grant" | "interaction_required")
            }
            Self::Minecraft(MinecraftError::Unauthorized) => true,
            _ => false,
        }
    }
}

fn send(transport: &dyn HttpTransport, request: &HttpRequest) -> Result<HttpResponse, SignInError> {
    Ok(transport.execute(request)?)
}

/// Runs the Xbox and Minecraft stages for a Microsoft access token.
///
/// `now` stamps the Minecraft token's lifetime; it is a parameter so tests do not depend
/// on the wall clock.
///
/// # Errors
///
/// - [`SignInError::Http`] when a request never produced a response;
/// - [`SignInError::Xbox`] for Xbox Live refusals (no Xbox profile, child account, region);
/// - [`SignInError::Minecraft`] with [`MinecraftError::NotEntitled`] when the account does
///   not own Java Edition, [`MinecraftError::ProfileNotFound`] when it has no player name
///   yet, and the other variants for service faults.
pub fn sign_in_to_minecraft(
    transport: &dyn HttpTransport,
    tokens: &MicrosoftTokens,
    now: SystemTime,
) -> Result<MinecraftLogin, SignInError> {
    let response = send(transport, &build_xbl_authenticate_request(&tokens.access))?;
    let xbl = parse_xbl_response(response.status, &response.body)?;

    let response = send(transport, &build_xsts_authorize_request(&xbl.token))?;
    let xsts = parse_xsts_response(response.status, &response.body)?;

    let response = send(
        transport,
        &build_login_request(&xsts.user_hash, &xsts.token),
    )?;
    let session = parse_login_response(response.status, &response.body, now)?;

    let response = send(transport, &build_entitlements_request(&session.access))?;
    let entitlements = parse_entitlements(response.status, &response.body)?;
    if !entitlements.owns_minecraft() {
        return Err(MinecraftError::NotEntitled.into());
    }

    let response = send(transport, &build_profile_request(&session.access))?;
    let profile = parse_profile(response.status, &response.body)?;

    Ok(MinecraftLogin {
        session,
        profile,
        xuid: xsts.xuid,
    })
}

/// Trades a stored refresh token for fresh Microsoft tokens.
///
/// The service rotates refresh tokens, so the caller must store `tokens.refresh` from the
/// result in place of the one it passed in.
///
/// # Errors
///
/// [`SignInError::OAuth`]; [`SignInError::requires_sign_in`] tells a revoked token from a
/// transient fault.
pub fn refresh_microsoft_tokens(
    transport: &dyn HttpTransport,
    endpoints: &DeviceCodeEndpoints,
    client_id: &str,
    refresh: &RefreshToken,
    now: SystemTime,
) -> Result<MicrosoftTokens, SignInError> {
    let request = RefreshRequest::new(client_id, refresh).to_http(endpoints);
    let response = send(transport, &request)?;
    Ok(parse_refresh_response(
        response.status,
        &response.body,
        now,
    )?)
}

#[cfg(test)]
#[path = "signin_tests.rs"]
mod tests;
