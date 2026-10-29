//! Stage one of the sign-in chain: the OAuth 2.0 device authorization grant.
//!
//! The user opens a verification page on another device (a phone, usually), types a short
//! user code, and the launcher polls the token endpoint until the account owner finishes
//! the sign-in. Nothing in this module touches the network: [`DeviceCodeRequest`] and
//! [`TokenPollRequest`] build an [`HttpRequest`], [`DeviceCodeAuthorization`] and
//! [`parse_token_response`] interpret the answer, and [`DeviceCodePoll`] is a clock-free
//! state machine that tells the caller how long to sleep between polls.
//!
//! # Security
//!
//! Two values are credentials here:
//!
//! - the `device_code`, which is exchanged for tokens and is modelled by [`DeviceCode`],
//!   a newtype whose `Debug` and `Display` print only the last four characters;
//! - the tokens that come back from [`parse_token_response`], which are [`AccessToken`]
//!   and [`RefreshToken`] and redact themselves.
//!
//! [`DeviceCodeError`] never carries a response body, a token or a device code. The only
//! data that reaches an error message is a status code or an OAuth error code, so an
//! error can be logged verbatim.

use std::fmt;
use std::time::{Duration, SystemTime};

use serde::Deserialize;

use crate::http::{HttpRequest, form_encode};
use crate::token::{AccessToken, MicrosoftTokens, RefreshToken, TokenLifetime};

/// The endpoint that hands out the user code and the device code.
pub const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
/// The endpoint that is polled until the sign-in completes, and that finally returns the
/// tokens.
pub const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
/// The scopes the launcher needs: `XboxLive.signin` for the Xbox stages that follow and
/// `offline_access` so a refresh token comes back and the user signs in only once.
pub const DEFAULT_SCOPE: &str = "XboxLive.signin offline_access";

/// The grant type of the device code flow, verbatim from RFC 8628.
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// The interval the flow falls back to when the service sends none, and the interval the
/// first poll uses when the service is silent about its preference.
const DEFAULT_INTERVAL_SECS: u64 = 5;

/// RFC 8628 forbids polling faster than once a second, so every interval is clamped up to
/// this floor.
const MIN_INTERVAL_SECS: u64 = 1;

/// The penalty RFC 8628 §3.5 attaches to `slow_down`.
const SLOW_DOWN_PENALTY: Duration = Duration::from_secs(5);

// --------------------------------------------------------------------------------------
// Endpoints
// --------------------------------------------------------------------------------------

/// Where the device code flow sends its two requests.
///
/// The Microsoft URLs are the production defaults; the fields are public so tests can
/// point the flow at a recording transport without punning on the real hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodeEndpoints {
    pub device_code_url: String,
    pub token_url: String,
}

impl DeviceCodeEndpoints {
    /// The Microsoft identity platform endpoints for consumer accounts.
    #[must_use]
    pub fn microsoft() -> Self {
        Self {
            device_code_url: DEVICE_CODE_URL.to_owned(),
            token_url: TOKEN_URL.to_owned(),
        }
    }
}

impl Default for DeviceCodeEndpoints {
    fn default() -> Self {
        Self::microsoft()
    }
}

// --------------------------------------------------------------------------------------
// The device code
// --------------------------------------------------------------------------------------

/// The opaque code the service returns from the device code endpoint.
///
/// It is a credential: anyone holding it and the client id can collect the tokens once
/// the user approves the sign-in. It therefore redacts itself like the token newtypes do,
/// and [`expose`](Self::expose) is only called while building a poll body.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceCode(String);

impl DeviceCode {
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The raw device code. Only use it while building a request body; never log it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Mirrors the redaction the token newtypes use: `Label(****1234)` for secrets long
/// enough to spare a tail, `Label(****)` otherwise.
impl fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.0.chars().count();
        if count >= 12 {
            let tail: String = self.0.chars().skip(count - 4).collect();
            write!(f, "DeviceCode(****{tail})")
        } else {
            f.write_str("DeviceCode(****)")
        }
    }
}

impl fmt::Display for DeviceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// --------------------------------------------------------------------------------------
// Step 1: request a device code
// --------------------------------------------------------------------------------------

/// The form request that asks the service for a device code and a user code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCodeRequest<'a> {
    pub client_id: &'a str,
    pub scope: &'a str,
}

impl<'a> DeviceCodeRequest<'a> {
    /// A request carrying [`DEFAULT_SCOPE`].
    #[must_use]
    pub fn new(client_id: &'a str) -> Self {
        Self {
            client_id,
            scope: DEFAULT_SCOPE,
        }
    }

    /// Replaces the scope list.
    #[must_use]
    pub fn with_scope(mut self, scope: &'a str) -> Self {
        self.scope = scope;
        self
    }

    /// Builds the form `POST` for the device code endpoint.
    #[must_use]
    pub fn to_http(&self, endpoints: &DeviceCodeEndpoints) -> HttpRequest {
        let body = format!(
            "client_id={}&scope={}",
            form_encode(self.client_id),
            form_encode(self.scope)
        );
        HttpRequest::post_form(endpoints.device_code_url.as_str(), body)
    }
}

// --------------------------------------------------------------------------------------
// Wire types
// --------------------------------------------------------------------------------------
//
// `DeviceCode` deliberately does not implement `serde`, so the JSON shapes are modelled
// with private structs holding plain `String`s and converted afterwards.

#[derive(Debug, Deserialize)]
struct AuthorizationResponse {
    user_code: String,
    verification_uri: String,
    device_code: String,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
}

/// What the user is shown while the launcher polls in the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodeAuthorization {
    /// The short code the user types on the verification page, e.g. `ABCD1234`.
    pub user_code: String,
    /// The page the user opens, e.g. `https://microsoft.com/devicelogin`.
    pub verification_uri: String,
    /// The credential the poll request carries. Redacts itself.
    pub device_code: DeviceCode,
    /// How long the pair of codes stays valid.
    pub expires_in: Duration,
    /// How often the service wants to be polled, never below one second.
    pub interval: Duration,
}

impl DeviceCodeAuthorization {
    /// Parses the device code endpoint response.
    ///
    /// `interval` is optional in the protocol; a missing or zero value becomes five
    /// seconds, and the result is clamped to at least one second either way.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceCodeError::MalformedResponse`] when the body is not JSON of the
    /// documented shape or when one of the required strings is empty. The error never
    /// carries the body, which contains the device code.
    pub fn from_json(body: &str) -> Result<Self, DeviceCodeError> {
        let parsed: AuthorizationResponse =
            serde_json::from_str(body).map_err(|_| DeviceCodeError::MalformedResponse)?;
        if parsed.user_code.is_empty()
            || parsed.verification_uri.is_empty()
            || parsed.device_code.is_empty()
        {
            return Err(DeviceCodeError::MalformedResponse);
        }
        if parsed.expires_in == 0 {
            return Err(DeviceCodeError::MalformedResponse);
        }
        let interval = parsed.interval.unwrap_or(DEFAULT_INTERVAL_SECS);
        Ok(Self {
            user_code: parsed.user_code,
            verification_uri: parsed.verification_uri,
            device_code: DeviceCode::new(parsed.device_code),
            expires_in: Duration::from_secs(parsed.expires_in),
            interval: clamp_interval(Duration::from_secs(interval)),
        })
    }
}

/// Raises an interval to the RFC 8628 floor of one second.
fn clamp_interval(interval: Duration) -> Duration {
    interval.max(Duration::from_secs(MIN_INTERVAL_SECS))
}

// --------------------------------------------------------------------------------------
// Step 2: poll the token endpoint
// --------------------------------------------------------------------------------------

/// One poll of the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPollRequest {
    pub client_id: String,
    pub device_code: DeviceCode,
}

impl TokenPollRequest {
    #[must_use]
    pub fn new(client_id: impl Into<String>, device_code: DeviceCode) -> Self {
        Self {
            client_id: client_id.into(),
            device_code,
        }
    }

    /// Builds the form `POST` for the token endpoint.
    #[must_use]
    pub fn to_http(&self, endpoints: &DeviceCodeEndpoints) -> HttpRequest {
        let body = format!(
            "grant_type={}&client_id={}&device_code={}",
            form_encode(DEVICE_CODE_GRANT_TYPE),
            form_encode(&self.client_id),
            form_encode(self.device_code.expose())
        );
        HttpRequest::post_form(endpoints.token_url.as_str(), body)
    }
}

// --------------------------------------------------------------------------------------
// Renewing a sign-in
// --------------------------------------------------------------------------------------

/// Exchanges a stored refresh token for a fresh access token, so the user signs in once
/// and later launches need no browser.
#[derive(Debug, Clone, Copy)]
pub struct RefreshRequest<'a> {
    pub client_id: &'a str,
    pub refresh: &'a RefreshToken,
    pub scope: &'a str,
}

impl<'a> RefreshRequest<'a> {
    /// A request carrying [`DEFAULT_SCOPE`].
    #[must_use]
    pub fn new(client_id: &'a str, refresh: &'a RefreshToken) -> Self {
        Self {
            client_id,
            refresh,
            scope: DEFAULT_SCOPE,
        }
    }

    /// Builds the form `POST` for the token endpoint.
    #[must_use]
    pub fn to_http(&self, endpoints: &DeviceCodeEndpoints) -> HttpRequest {
        let body = format!(
            "grant_type=refresh_token&client_id={}&refresh_token={}&scope={}",
            form_encode(self.client_id),
            form_encode(self.refresh.expose()),
            form_encode(self.scope)
        );
        HttpRequest::post_form(endpoints.token_url.as_str(), body)
    }
}

/// Interprets the answer to a [`RefreshRequest`].
///
/// A refresh answer is either tokens or an OAuth error; `invalid_grant` (the refresh
/// token was revoked or expired) arrives as [`DeviceCodeError::OAuth`], which the caller
/// treats as "sign in again". The device-flow outcomes (`authorization_pending` and
/// friends) make no sense here and are reported the same way.
///
/// # Errors
///
/// Same as [`parse_token_response`], plus [`DeviceCodeError::OAuth`] for any outcome other
/// than tokens.
pub fn parse_refresh_response(
    status: u16,
    body: &str,
    issued_at: SystemTime,
) -> Result<MicrosoftTokens, DeviceCodeError> {
    match parse_token_response(status, body, issued_at)? {
        PollOutcome::Authorized(tokens) => Ok(tokens),
        _ => Err(DeviceCodeError::OAuth {
            code: "unexpected_refresh_outcome".to_owned(),
        }),
    }
}

// --------------------------------------------------------------------------------------
// Step 3: interpret the poll answer
// --------------------------------------------------------------------------------------

/// What one poll of the token endpoint means for the state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// The account owner approved the sign-in; the tokens are the result of the flow.
    Authorized(MicrosoftTokens),
    /// The account owner has not finished yet.
    Pending,
    /// The launcher is polling too fast; RFC 8628 asks for a five second penalty.
    SlowDown,
    /// The device code is past its lifetime; start over with a new device code.
    Expired,
    /// The account owner refused the sign-in.
    Denied,
}

/// Interprets one answer from the token endpoint.
///
/// `issued_at` is supplied by the caller instead of being read from the wall clock, so the
/// resulting [`TokenLifetime`] stays deterministic under test.
///
/// # Errors
///
/// - `2xx` whose body lacks `access_token`/`expires_in`, or is not JSON →
///   [`DeviceCodeError::MalformedResponse`];
/// - `400`, `401` or `403` whose body names no `error` string →
///   [`DeviceCodeError::MalformedResponse`];
/// - `400`, `401` or `403` with an `error` value other than the four RFC 8628 codes →
///   [`DeviceCodeError::OAuth`];
/// - any other status, including `404` and `5xx` →
///   [`DeviceCodeError::UnexpectedStatus`].
pub fn parse_token_response(
    status: u16,
    body: &str,
    issued_at: SystemTime,
) -> Result<PollOutcome, DeviceCodeError> {
    if (200..300).contains(&status) {
        let parsed: TokenSuccessResponse =
            serde_json::from_str(body).map_err(|_| DeviceCodeError::MalformedResponse)?;
        if parsed.access_token.is_empty() || parsed.expires_in == 0 {
            return Err(DeviceCodeError::MalformedResponse);
        }
        let lifetime = TokenLifetime::new(issued_at, Duration::from_secs(parsed.expires_in));
        let refresh = RefreshToken::from_optional(parsed.refresh_token);
        return Ok(PollOutcome::Authorized(MicrosoftTokens::new(
            AccessToken::new(parsed.access_token),
            refresh,
            lifetime,
        )));
    }
    // A body that names an `error` code is an OAuth rejection whatever the status is: the
    // token endpoint normally answers `400` (RFC 6749 §5.2, RFC 8628 §3.5), but Microsoft
    // also uses `401` for an expired device code, so the mapping must not depend on which
    // `4xx` arrived.
    if let Some(outcome) = oauth_rejection(body) {
        return outcome;
    }
    // No error code was named. `400` is still the endpoint's error channel, so a `400` that
    // carries no usable code is a malformed token response. Every other status — `403`,
    // `404`, `429`, `5xx` — means the endpoint or the service itself is wrong, and the
    // status is the whole report: the body says nothing about the sign-in.
    if status == 400 {
        return Err(DeviceCodeError::MalformedResponse);
    }
    Err(DeviceCodeError::UnexpectedStatus { status })
}

/// Interprets a non-`2xx` body as an OAuth rejection, if it names an `error` code.
///
/// Returns `None` when the body is not a JSON object with a string `error` field, leaving
/// the decision to the caller.
fn oauth_rejection(body: &str) -> Option<Result<PollOutcome, DeviceCodeError>> {
    let parsed: TokenErrorResponse = serde_json::from_str(body).ok()?;
    // Only the error code is kept: the rest of the body is untrusted and may echo request
    // parameters, including the device code.
    Some(match parsed.error.as_str() {
        "authorization_pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::SlowDown),
        "expired_token" => Ok(PollOutcome::Expired),
        "authorization_declined" => Ok(PollOutcome::Denied),
        _ => Err(DeviceCodeError::OAuth { code: parsed.error }),
    })
}

// --------------------------------------------------------------------------------------
// Step 4: the clock-free polling state machine
// --------------------------------------------------------------------------------------

/// Tracks how long the launcher has been polling a device code.
///
/// The caller owns the sleeping: [`advance`](Self::advance) returns the delay to wait
/// before the next poll, so no timer is hidden inside the struct and the whole loop can be
/// replayed in a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodePoll {
    interval: Duration,
    expires_in: Duration,
    elapsed: Duration,
}

impl DeviceCodePoll {
    /// Seeds the state machine from a freshly issued authorization.
    #[must_use]
    pub fn from_challenge(challenge: &DeviceCodeAuthorization) -> Self {
        Self {
            interval: clamp_interval(challenge.interval),
            expires_in: challenge.expires_in,
            elapsed: Duration::ZERO,
        }
    }

    /// The current polling interval, which grows by five seconds after every `slow_down`.
    #[must_use]
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// The polling time accounted for so far.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Feeds one poll result in and reports what the caller should do next.
    ///
    /// Every call first charges one current interval to the elapsed time, including the
    /// terminating ones: a failed attempt consumed at least as long as the interval it was
    /// scheduled with.
    ///
    /// - `SlowDown` raises the interval by five seconds before charging it, which is the
    ///   penalty RFC 8628 §3.5 prescribes;
    /// - `Pending` and `SlowDown` answer [`PollAction::Wait`] until the elapsed time
    ///   reaches the advertised lifetime, then [`PollAction::Expired`];
    /// - `Denied` ends the attempt with [`PollAction::Expired`] as well, because the
    ///   device code cannot be reused for a second approval;
    /// - `Authorized` answers [`PollAction::Done`] regardless of the elapsed time.
    pub fn advance(&mut self, outcome: PollOutcome) -> PollAction {
        if let PollOutcome::Authorized(tokens) = outcome {
            self.elapsed = self.elapsed.saturating_add(self.interval);
            return PollAction::Done(tokens);
        }
        match outcome {
            PollOutcome::SlowDown => {
                self.interval = self.interval.saturating_add(SLOW_DOWN_PENALTY);
            }
            PollOutcome::Expired | PollOutcome::Denied => {
                self.elapsed = self.elapsed.saturating_add(self.interval);
                return PollAction::Expired;
            }
            PollOutcome::Pending | PollOutcome::Authorized(_) => {}
        }
        self.elapsed = self.elapsed.saturating_add(self.interval);
        if self.elapsed >= self.expires_in {
            PollAction::Expired
        } else {
            PollAction::Wait(self.interval)
        }
    }
}

/// What the caller does after one poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollAction {
    /// The flow succeeded; keep these tokens and stop polling.
    Done(MicrosoftTokens),
    /// Sleep this long, then poll again.
    Wait(Duration),
    /// The attempt is over: the device code expired, or the user declined. The caller must
    /// request a new device code and show a new user code.
    Expired,
}

// --------------------------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------------------------

/// Failures reported by the device code flow.
///
/// [`DeviceCodeError::MalformedResponse`] carries no payload on purpose: a `serde_json`
/// error would describe the offending document, which contains a device code or a token.
#[derive(Debug, thiserror::Error)]
pub enum DeviceCodeError {
    #[error("the device code response is missing required fields")]
    MalformedResponse,
    #[error("the OAuth endpoint returned an unexpected status {status}")]
    UnexpectedStatus { status: u16 },
    #[error("the OAuth endpoint rejected the request with {code}")]
    OAuth { code: String },
    #[error("the device code has expired")]
    Expired,
}

#[cfg(test)]
#[path = "device_code_tests.rs"]
mod tests;
