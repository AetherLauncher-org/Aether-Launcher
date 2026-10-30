//! Stage one of the sign-in chain, the other way: the OAuth 2.0 authorization code flow
//! with PKCE (RFC 7636), for a public client on the same computer as the browser.
//!
//! The launcher opens the Microsoft sign-in page in the user's browser, the browser is
//! redirected back to a one-shot listener on the loopback interface, and the code that
//! arrives there is traded for tokens together with a secret only the launcher knows (the
//! PKCE verifier). Nothing in this module touches the network or the clock:
//! [`AuthorizationRequest::url`] and [`CodeExchangeRequest::to_http`] build what is sent,
//! [`callback_query`] and [`parse_callback_query`] interpret what the browser sends back,
//! and [`parse_exchange_response`] reads the answer. The listener itself lives in the
//! desktop shell.
//!
//! # Security
//!
//! - The **verifier** and the **authorization code** are credentials: [`PkceVerifier`] and
//!   [`AuthCode`] redact themselves in `Debug` and `Display`, and no error carries one.
//! - The redirect is accepted only with the `state` the launcher generated for this
//!   attempt, so a request from another page or another process cannot complete or
//!   poison a sign-in ([`CallbackError::StateMismatch`]).
//! - The request that reaches the listener is untrusted input: [`callback_query`] bounds
//!   its size, requires `GET /`, and requires a loopback `Host` header with the right
//!   port, which closes the DNS-rebinding route.

use std::fmt;
use std::time::SystemTime;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::device_code::{DEFAULT_SCOPE, DeviceCodeError, TOKEN_URL, parse_refresh_response};
use crate::http::{HttpRequest, form_encode};
use crate::token::MicrosoftTokens;

/// The endpoint the browser is sent to.
pub const AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";

/// The longest request head [`callback_query`] looks at.
pub const MAX_REQUEST_HEAD_BYTES: usize = 8 * 1024;

/// The longest authorization code accepted from a redirect. Microsoft's are around a
/// thousand characters; anything far past that is not one.
const MAX_CODE_LEN: usize = 4096;

/// Where the authorization code flow sends its requests.
///
/// The fields are public so tests can point the flow at a recording transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCodeEndpoints {
    pub authorize_url: String,
    pub token_url: String,
}

impl AuthCodeEndpoints {
    /// The Microsoft identity platform endpoints for consumer accounts.
    #[must_use]
    pub fn microsoft() -> Self {
        Self {
            authorize_url: AUTHORIZE_URL.to_owned(),
            token_url: TOKEN_URL.to_owned(),
        }
    }
}

impl Default for AuthCodeEndpoints {
    fn default() -> Self {
        Self::microsoft()
    }
}

// --------------------------------------------------------------------------------------
// Secrets
// --------------------------------------------------------------------------------------

/// The PKCE code verifier: a random secret the token request must repeat.
///
/// Whoever intercepts the redirect still cannot redeem the code without it.
#[derive(Clone, PartialEq, Eq)]
pub struct PkceVerifier(String);

impl PkceVerifier {
    /// The raw verifier. Only use it while building the token request body.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PkceVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PkceVerifier(****)")
    }
}

impl fmt::Display for PkceVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// The one-time authorization code from the redirect. A credential until it is redeemed.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthCode(String);

impl AuthCode {
    /// The raw code. Only use it while building the token request body.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthCode(****)")
    }
}

impl fmt::Display for AuthCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// --------------------------------------------------------------------------------------
// PKCE
// --------------------------------------------------------------------------------------

/// A verifier and the challenge derived from it (`S256`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceChallenge {
    verifier: PkceVerifier,
    challenge: String,
}

impl PkceChallenge {
    /// A fresh random pair. The verifier is 64 hex characters, which is inside RFC 7636's
    /// 43 to 128 and carries 244 bits of randomness.
    #[must_use]
    pub fn generate() -> Self {
        Self::from_verifier(random_text())
    }

    /// Derives the challenge for a given verifier; used by [`generate`](Self::generate)
    /// and by tests that replay the RFC's example.
    #[must_use]
    pub fn from_verifier(verifier: impl Into<String>) -> Self {
        let verifier = verifier.into();
        let digest = Sha256::digest(verifier.as_bytes());
        Self {
            challenge: base64_url(&digest),
            verifier: PkceVerifier(verifier),
        }
    }

    /// The value sent in the authorization URL.
    #[must_use]
    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    /// The secret kept for the token request.
    #[must_use]
    pub fn verifier(&self) -> &PkceVerifier {
        &self.verifier
    }
}

/// A fresh unguessable `state` value for one attempt.
#[must_use]
pub fn random_state() -> String {
    random_text()
}

/// 64 lowercase hex characters from two version-4 UUIDs (the operating system's CSPRNG).
fn random_text() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Base64 with the URL-safe alphabet and no padding (RFC 4648 §5).
fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = chunk.get(1).copied().map_or(0, u32::from);
        let third = chunk.get(2).copied().map_or(0, u32::from);
        let group = (first << 16) | (second << 8) | third;
        let emit = chunk.len() + 1;
        for index in 0..emit {
            let sextet = (group >> (18 - 6 * index)) & 0x3f;
            out.push(char::from(ALPHABET[sextet as usize]));
        }
    }
    out
}

// --------------------------------------------------------------------------------------
// Step 1: the URL the browser opens
// --------------------------------------------------------------------------------------

/// The authorization request, as a URL for the user's browser.
#[derive(Debug, Clone, Copy)]
pub struct AuthorizationRequest<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub challenge: &'a str,
    pub state: &'a str,
    pub scope: &'a str,
}

impl<'a> AuthorizationRequest<'a> {
    /// A request carrying [`DEFAULT_SCOPE`].
    #[must_use]
    pub fn new(
        client_id: &'a str,
        redirect_uri: &'a str,
        challenge: &'a PkceChallenge,
        state: &'a str,
    ) -> Self {
        Self {
            client_id,
            redirect_uri,
            challenge: challenge.challenge(),
            state,
            scope: DEFAULT_SCOPE,
        }
    }

    /// The full authorization URL. `prompt=select_account` lets the user pick which
    /// Microsoft account to use instead of silently reusing the browser's session.
    #[must_use]
    pub fn url(&self, endpoints: &AuthCodeEndpoints) -> String {
        format!(
            "{}?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}\
             &state={}&code_challenge={}&code_challenge_method=S256&prompt=select_account",
            endpoints.authorize_url,
            form_encode(self.client_id),
            form_encode(self.redirect_uri),
            form_encode(self.scope),
            form_encode(self.state),
            form_encode(self.challenge),
        )
    }
}

// --------------------------------------------------------------------------------------
// Step 2: what the browser sends back
// --------------------------------------------------------------------------------------

/// Why a redirect did not yield an authorization code.
///
/// No variant carries the query string, which holds the code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallbackError {
    #[error("the sign-in was declined")]
    Denied,
    #[error("the authorization endpoint rejected the request with {code}")]
    OAuth { code: String },
    #[error("the redirect carried a different state than this sign-in expects")]
    StateMismatch,
    #[error("the redirect carried no authorization code")]
    MissingCode,
    #[error("the redirect is malformed")]
    Malformed,
}

/// Interprets the query string of the redirect.
///
/// The `state` is checked first, so a redirect that is not part of this attempt is
/// reported as [`CallbackError::StateMismatch`] whatever else it says.
///
/// # Errors
///
/// See [`CallbackError`].
pub fn parse_callback_query(query: &str, expected_state: &str) -> Result<AuthCode, CallbackError> {
    if query.len() > MAX_REQUEST_HEAD_BYTES {
        return Err(CallbackError::Malformed);
    }
    let mut state = None;
    let mut code = None;
    let mut error = None;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name).ok_or(CallbackError::Malformed)?;
        let value = percent_decode(value).ok_or(CallbackError::Malformed)?;
        match name.as_str() {
            "state" => state = Some(value),
            "code" => code = Some(value),
            "error" => error = Some(value),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Err(CallbackError::StateMismatch);
    }
    if let Some(error) = error {
        return Err(match error.as_str() {
            "access_denied" => CallbackError::Denied,
            // The code comes from a query string anyone can write to: keep it short and
            // plain before it reaches a message.
            other if is_plain_error_code(other) => CallbackError::OAuth {
                code: other.to_owned(),
            },
            _ => CallbackError::OAuth {
                code: "invalid_error".to_owned(),
            },
        });
    }
    match code {
        Some(code) if !code.is_empty() && code.len() <= MAX_CODE_LEN => Ok(AuthCode(code)),
        _ => Err(CallbackError::MissingCode),
    }
}

fn is_plain_error_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Percent-decodes a query component (`+` is a space). `None` for a bad escape or for
/// bytes that are not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hi = hex_value(*bytes.get(index + 1)?)?;
                let lo = hex_value(*bytes.get(index + 2)?)?;
                out.push(hi << 4 | lo);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(16)
        .and_then(|digit| u8::try_from(digit).ok())
}

/// Extracts the query of the one request the listener is waiting for.
///
/// `head` is what was read from the connection up to the blank line. It qualifies only if
/// it is `GET /?…` over HTTP/1.x with a `Host` of `localhost`, `127.0.0.1` or `[::1]` on
/// `port`. Everything else (the browser asking for `/favicon.ico`, another program poking
/// the port, a page that was rebound to the loopback address) is `None`, and the caller
/// answers it with a plain 404 and keeps waiting.
#[must_use]
pub fn callback_query(head: &str, port: u16) -> Option<String> {
    if head.len() > MAX_REQUEST_HEAD_BYTES {
        return None;
    }
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split(' ');
    let method = parts.next()?;
    let target = parts.next()?;
    let version = parts.next()?;
    if parts.next().is_some() || method != "GET" || !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return None;
    }
    let (path, query) = target.split_once('?')?;
    if path != "/" || query.is_empty() {
        return None;
    }
    let host = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("host").then(|| value.trim())
    })?;
    let allowed = [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ];
    allowed
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(host))
        .then(|| query.to_owned())
}

// --------------------------------------------------------------------------------------
// Step 3: trade the code for tokens
// --------------------------------------------------------------------------------------

/// The token request that redeems the authorization code.
#[derive(Debug, Clone, Copy)]
pub struct CodeExchangeRequest<'a> {
    pub client_id: &'a str,
    pub code: &'a AuthCode,
    pub redirect_uri: &'a str,
    pub verifier: &'a PkceVerifier,
    pub scope: &'a str,
}

impl<'a> CodeExchangeRequest<'a> {
    /// A request carrying [`DEFAULT_SCOPE`].
    #[must_use]
    pub fn new(
        client_id: &'a str,
        code: &'a AuthCode,
        redirect_uri: &'a str,
        verifier: &'a PkceVerifier,
    ) -> Self {
        Self {
            client_id,
            code,
            redirect_uri,
            verifier,
            scope: DEFAULT_SCOPE,
        }
    }

    /// Builds the form `POST` for the token endpoint.
    #[must_use]
    pub fn to_http(&self, endpoints: &AuthCodeEndpoints) -> HttpRequest {
        let body = format!(
            "grant_type=authorization_code&client_id={}&code={}&redirect_uri={}\
             &code_verifier={}&scope={}",
            form_encode(self.client_id),
            form_encode(self.code.expose()),
            form_encode(self.redirect_uri),
            form_encode(self.verifier.expose()),
            form_encode(self.scope),
        );
        HttpRequest::post_form(endpoints.token_url.as_str(), body)
    }
}

/// Interprets the answer to a [`CodeExchangeRequest`]: tokens, or an OAuth error such as
/// `invalid_grant` (the code was already used or has expired).
///
/// # Errors
///
/// Same as [`parse_refresh_response`].
pub fn parse_exchange_response(
    status: u16,
    body: &str,
    issued_at: SystemTime,
) -> Result<MicrosoftTokens, DeviceCodeError> {
    parse_refresh_response(status, body, issued_at)
}

#[cfg(test)]
#[path = "auth_code_tests.rs"]
mod tests;
