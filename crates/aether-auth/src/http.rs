//! The HTTP surface the auth flows depend on.
//!
//! Every flow builds an [`HttpRequest`], hands it to an [`HttpTransport`], and parses
//! the [`HttpResponse`] with the same status-aware rules the real services use. No
//! implementation of [`HttpTransport`] lives in this crate: production code plugs in a
//! blocking client, tests plug in an in-memory recorder. That is what keeps the test
//! suite offline.

use std::fmt;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

/// One request handed to a transport.
///
/// The URL never carries credentials: tokens travel in the body or in headers. The
/// `Debug` implementation redacts the body anyway so an incidental log line cannot leak
/// a device code or a refresh token.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl HttpRequest {
    /// A `POST` that ships a JSON document.
    #[must_use]
    pub fn post_json(url: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("accept".to_owned(), "application/json".to_owned()),
            ],
            body: Some(body.into()),
        }
    }

    /// A `POST` that ships a form-encoded document, as the OAuth 2.0 endpoints expect.
    #[must_use]
    pub fn post_form(url: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: vec![
                (
                    "content-type".to_owned(),
                    "application/x-www-form-urlencoded".to_owned(),
                ),
                ("accept".to_owned(), "application/json".to_owned()),
            ],
            body: Some(body.into()),
        }
    }

    /// A plain `GET`, used by the Minecraft profile and entitlement endpoints.
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: vec![("accept".to_owned(), "application/json".to_owned())],
            body: None,
        }
    }

    /// Adds or replaces a header, keeping the header list free of duplicates.
    #[must_use]
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        let value = value.into();
        if let Some(existing) = self
            .headers
            .iter_mut()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            existing.1 = value;
        } else {
            self.headers.push((name.to_ascii_lowercase(), value));
        }
        self
    }

    /// Returns a header value case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The request body, if any.
    #[must_use]
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = self
            .body
            .as_ref()
            .map(|body| format!("<redacted:{} bytes>", body.len()));
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &redacted_headers(&self.headers))
            .field("body", &redacted)
            .finish()
    }
}

/// Header names whose values must never reach a log line. `Authorization: Bearer <token>`
/// is how the Minecraft endpoints carry credentials, so printing the header list verbatim
/// would defeat every other redaction in this crate.
const SENSITIVE_HEADERS: [&str; 5] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
];

/// Pairs every header with its printable value: names stay visible, secrets do not.
fn redacted_headers(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers
        .iter()
        .map(|(name, value)| {
            let printable = if SENSITIVE_HEADERS
                .iter()
                .any(|sensitive| name.eq_ignore_ascii_case(sensitive))
            {
                "<redacted>"
            } else {
                value.as_str()
            };
            (name.as_str(), printable)
        })
        .collect()
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    #[must_use]
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }

    /// `true` for `2xx`, the only range the flows treat as success.
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// A response handed back by a transport.
///
/// Successful token and login responses carry the credentials themselves, so the `Debug`
/// implementation reports only the status and the body length. Never log `body` for a
/// `2xx` from an authentication endpoint.
impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("body", &format!("<redacted:{} bytes>", self.body.len()))
            .finish()
    }
}

/// The transport boundary. Implementations are blocking, never retry on their own, and
/// map every I/O fault to [`HttpError::Transport`] without embedding the request body.
pub trait HttpTransport {
    /// Performs one request. Errors are transport-level only: a `4xx` or `5xx` answer is
    /// a successful transport call with an unsuccessful status.
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// The request never produced a response. Only the host is reported, never the full
    /// URL, so query strings cannot leak into logs.
    #[error("network failure while contacting {host}")]
    Transport { host: String },
    /// The caller built a request whose URL cannot be understood.
    #[error("invalid endpoint: {reason}")]
    InvalidEndpoint { reason: &'static str },
}

impl HttpError {
    /// Builds a transport fault for `url`, keeping only the host component.
    #[must_use]
    pub fn transport(url: &str) -> Self {
        Self::Transport {
            host: host_of(url).unwrap_or("<unknown host>").to_owned(),
        }
    }
}

/// Extracts the host from an absolute URL without touching the path or query.
#[must_use]
pub fn host_of(url: &str) -> Option<&str> {
    let (_, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    (!host.is_empty()).then_some(host)
}

/// Percent-encodes a form value. The OAuth 2.0 device code flow posts
/// `application/x-www-form-urlencoded`, and client ids, scopes and codes all contain
/// characters that must be escaped.
#[must_use]
pub fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            other => {
                let hi = char::from_digit(u32::from(other >> 4), 16).unwrap_or('0');
                let lo = char::from_digit(u32::from(other & 0x0f), 16).unwrap_or('0');
                encoded.push('%');
                encoded.push(hi.to_ascii_uppercase());
                encoded.push(lo.to_ascii_uppercase());
            }
        }
    }
    encoded
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
