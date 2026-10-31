//! Browser sign-in: the authorization code flow with PKCE and a loopback redirect.
//!
//! [`prepare`] binds a one-shot listener on `127.0.0.1` with a free port and builds the
//! Microsoft sign-in URL for it. The user signs in in their own browser, which is sent
//! back to `http://localhost:<port>`; [`wait_for_code`] reads that single request, hands the
//! authorization code to [`exchange_code`], and [`run`] finishes the Xbox and Minecraft
//! stages exactly as the device code flow does.
//!
//! # What this opens up
//!
//! A listening socket, so it is kept as small as it can be:
//!
//! - loopback only, an ephemeral port, alive for one attempt and closed afterwards;
//! - a request counts only if it is `GET /?…` with a loopback `Host` and the `state` of this
//!   attempt (`aether_auth::auth_code::callback_query`, `parse_callback_query`); anything
//!   else gets a bare 404 or 400 and the wait goes on;
//! - at most 8 KiB are read per request, with a short timeout, so a stalled connection
//!   cannot hold the sign-in up for long;
//! - the code is useless without the PKCE verifier, which never leaves this process;
//! - the frontend never supplies a URL or a port: it only asks for the attempt to start.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use aether_auth::auth_code::{
    AuthCode, AuthCodeEndpoints, AuthorizationRequest, CallbackError, CodeExchangeRequest,
    MAX_REQUEST_HEAD_BYTES, PkceChallenge, PkceVerifier, callback_query, parse_callback_query,
    parse_exchange_response, random_state,
};
use aether_auth::device_code::DeviceCodeError;
use aether_auth::http::HttpTransport;
use aether_auth::secure_store::KeyringCredentialStore;
use aether_auth::token::MicrosoftTokens;

use super::{LoginEvent, LoginFailure, Session, UreqAuthTransport, finish_login};

/// How long the user has to finish signing in before the attempt is dropped.
pub(super) const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

const POLL_SLICE: Duration = Duration::from_millis(50);
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Everything one attempt needs, created before the browser is opened.
pub(super) struct Prepared {
    listener: TcpListener,
    port: u16,
    pkce: PkceChallenge,
    state: String,
    redirect_uri: String,
    /// The sign-in page the browser opens. Backend-built; the only URL the "open page"
    /// command will hand to the system.
    pub(super) url: String,
}

/// Binds the listener and builds the sign-in URL.
pub(super) fn prepare(client_id: &str) -> Result<Prepared, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|_| "cannot listen on the loopback interface for the sign-in".to_owned())?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "cannot configure the sign-in listener".to_owned())?;
    let port = listener
        .local_addr()
        .map_err(|_| "cannot read the sign-in listener address".to_owned())?
        .port();

    let pkce = PkceChallenge::generate();
    let state = random_state();
    // Microsoft matches `http://localhost` registrations on any port.
    let redirect_uri = format!("http://localhost:{port}");
    let url = AuthorizationRequest::new(client_id, &redirect_uri, &pkce, &state)
        .url(&AuthCodeEndpoints::microsoft());
    Ok(Prepared {
        listener,
        port,
        pkce,
        state,
        redirect_uri,
        url,
    })
}

/// How the wait for the redirect ended without a code.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BrowserEnd {
    Cancelled,
    TimedOut,
    Callback(CallbackError),
}

/// Waits for the one valid redirect and returns its authorization code.
pub(super) fn wait_for_code(
    listener: &TcpListener,
    port: u16,
    state: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<AuthCode, BrowserEnd> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(BrowserEnd::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(BrowserEnd::TimedOut);
        }
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => {
                std::thread::sleep(POLL_SLICE);
                continue;
            }
        };
        if stream.set_nonblocking(false).is_err()
            || stream.set_read_timeout(Some(READ_TIMEOUT)).is_err()
            || stream.set_write_timeout(Some(READ_TIMEOUT)).is_err()
        {
            continue;
        }
        let Some(query) = read_head(&mut stream).and_then(|head| callback_query(&head, port))
        else {
            respond(&mut stream, "404 Not Found", NOT_FOUND_PAGE);
            continue;
        };
        match parse_callback_query(&query, state) {
            Ok(code) => {
                respond(&mut stream, "200 OK", SUCCESS_PAGE);
                return Ok(code);
            }
            // Not ours: someone else's redirect. Refuse it and keep waiting for the real one.
            Err(CallbackError::StateMismatch | CallbackError::Malformed) => {
                respond(&mut stream, "400 Bad Request", NOT_FOUND_PAGE);
            }
            Err(error) => {
                respond(&mut stream, "200 OK", FAILURE_PAGE);
                return Err(BrowserEnd::Callback(error));
            }
        }
    }
}

/// Reads a request head (up to the blank line), at most [`MAX_REQUEST_HEAD_BYTES`].
fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        head.extend_from_slice(&chunk[..read]);
        if head.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if head.len() > MAX_REQUEST_HEAD_BYTES {
            return None;
        }
    }
    Some(String::from_utf8_lossy(&head).into_owned())
}

const NOT_FOUND_PAGE: &str =
    "<!doctype html><meta charset=\"utf-8\"><title>Aether Launcher</title><p>Not found.";
const SUCCESS_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>Aether Launcher</title>\
<style>body{font:16px system-ui,sans-serif;margin:3rem auto;max-width:32rem;padding:0 1rem}</style>\
<h1>Signed in</h1><p>You can close this tab and return to Aether Launcher.</p>\
<p>已登录。可以关闭这个标签页，回到 Aether Launcher。</p>";
const FAILURE_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>Aether Launcher</title>\
<style>body{font:16px system-ui,sans-serif;margin:3rem auto;max-width:32rem;padding:0 1rem}</style>\
<h1>Sign-in did not finish</h1><p>Return to Aether Launcher to see why and to try again.</p>\
<p>登录没有完成。回到 Aether Launcher 查看原因并重试。</p>";

/// Answers a request and closes the connection. The page loads nothing and the headers keep
/// the address (which carries the code) from being cached or sent anywhere.
fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Redeems the authorization code for Microsoft tokens.
pub(super) fn exchange_code(
    transport: &dyn HttpTransport,
    client_id: &str,
    code: &AuthCode,
    redirect_uri: &str,
    verifier: &PkceVerifier,
    now: SystemTime,
) -> Result<MicrosoftTokens, LoginFailure> {
    let request = CodeExchangeRequest::new(client_id, code, redirect_uri, verifier)
        .to_http(&AuthCodeEndpoints::microsoft());
    let response = transport.execute(&request).map_err(|_| LoginFailure {
        reason: "network",
        message: "the Microsoft sign-in service is unreachable".to_owned(),
    })?;
    parse_exchange_response(response.status, &response.body, now).map_err(|error| {
        let reason = match &error {
            // The code was already used or has expired: the attempt is over.
            DeviceCodeError::OAuth { code } if code == "invalid_grant" => "expired",
            _ => "service",
        };
        LoginFailure {
            reason,
            message: error.to_string(),
        }
    })
}

fn ended(end: BrowserEnd) -> LoginEvent {
    let (reason, message) = match end {
        BrowserEnd::Cancelled => return LoginEvent::Cancelled,
        BrowserEnd::TimedOut => ("expired", "the sign-in took too long".to_owned()),
        BrowserEnd::Callback(CallbackError::Denied) => {
            ("denied", "the sign-in was declined".to_owned())
        }
        BrowserEnd::Callback(error) => ("service", error.to_string()),
    };
    LoginEvent::Failed {
        failure: LoginFailure { reason, message },
    }
}

/// The worker: waits for the redirect, finishes the chain, reports one terminal event.
pub(super) fn run(
    client_id: &str,
    root: &Path,
    prepared: &Prepared,
    session: &Session,
    timeout: Duration,
) -> LoginEvent {
    let code = match wait_for_code(
        &prepared.listener,
        prepared.port,
        &prepared.state,
        &session.cancel,
        timeout,
    ) {
        Ok(code) => code,
        Err(end) => return ended(end),
    };
    let transport = UreqAuthTransport;
    let tokens = match exchange_code(
        &transport,
        client_id,
        &code,
        &prepared.redirect_uri,
        prepared.pkce.verifier(),
        SystemTime::now(),
    ) {
        Ok(tokens) => tokens,
        Err(failure) => return LoginEvent::Failed { failure },
    };
    let store = KeyringCredentialStore::new(root);
    match finish_login(&transport, &store, &tokens, SystemTime::now()) {
        Ok(account) => LoginEvent::SignedIn { account },
        Err(failure) => LoginEvent::Failed { failure },
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
