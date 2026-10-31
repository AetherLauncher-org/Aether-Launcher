use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::thread;

use aether_auth::auth_code::AuthorizationRequest;
use aether_auth::http::{HttpError, HttpRequest, HttpResponse};

use super::super::is_trusted_verification_uri;
use super::*;

const CLIENT_ID: &str = "00000000-0000-0000-0000-00000000dead";

/// Sends `request` to the listener and returns the status line and the body.
fn send(port: u16, request: &str) -> (String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(request.as_bytes()).expect("write");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    let status = answer.lines().next().unwrap_or("").to_owned();
    let body = answer.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, body)
}

fn get(port: u16, target: &str) -> (String, String) {
    send(
        port,
        &format!("GET {target} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"),
    )
}

/// Runs [`wait_for_code`] on a thread and returns its handle.
fn wait_in_background(
    prepared: Prepared,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) -> thread::JoinHandle<Result<AuthCode, BrowserEnd>> {
    thread::spawn(move || {
        wait_for_code(
            &prepared.listener,
            prepared.port,
            &prepared.state,
            &cancel,
            timeout,
        )
    })
}

#[test]
fn the_sign_in_url_points_at_microsoft_and_the_loopback_redirect() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    assert!(prepared.port > 0);
    assert_eq!(
        prepared.redirect_uri,
        format!("http://localhost:{}", prepared.port)
    );
    assert!(prepared.url.starts_with(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id="
    ));
    assert!(prepared.url.contains(&format!("state={}", prepared.state)));
    assert!(
        prepared
            .url
            .contains("redirect_uri=http%3A%2F%2Flocalhost%3A")
    );
    // The verifier stays here.
    assert!(!prepared.url.contains(prepared.pkce.verifier().expose()));
    // Opening it needs no further trust decision than the device code page gets.
    assert!(is_trusted_verification_uri(&prepared.url));
    let again = AuthorizationRequest::new(
        CLIENT_ID,
        &prepared.redirect_uri,
        &prepared.pkce,
        &prepared.state,
    )
    .url(&AuthCodeEndpoints::microsoft());
    assert_eq!(prepared.url, again);
}

#[test]
fn two_attempts_do_not_share_a_port_or_a_state() {
    let first = prepare(CLIENT_ID).expect("first");
    let second = prepare(CLIENT_ID).expect("second");
    assert_ne!(first.port, second.port);
    assert_ne!(first.state, second.state);
}

#[test]
fn the_listener_is_loopback_only() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    let address = prepared.listener.local_addr().expect("address");
    assert!(address.ip().is_loopback(), "{address}");
}

#[test]
fn only_the_real_redirect_completes_the_wait() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    let (port, state) = (prepared.port, prepared.state.clone());
    let handle = wait_in_background(
        prepared,
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(20),
    );

    // The browser asks for the icon: not the redirect.
    assert!(get(port, "/favicon.ico").0.contains("404"));
    // Another page, another attempt: refused, and the wait goes on.
    assert!(
        get(port, "/?code=stolen&state=someone-elses")
            .0
            .contains("400")
    );
    // A rebound page would carry its own host name.
    let rebound = send(
        port,
        &format!("GET /?code=x&state={state} HTTP/1.1\r\nHost: evil.example:{port}\r\n\r\n"),
    );
    assert!(rebound.0.contains("404"), "{rebound:?}");

    let (status, body) = get(port, &format!("/?code=M.C5%2Babc&state={state}"));
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("close this tab"), "{body}");

    let code = handle.join().expect("thread").expect("a code");
    assert_eq!(code.expose(), "M.C5+abc");
}

#[test]
fn a_declined_sign_in_ends_the_wait_with_the_reason() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    let (port, state) = (prepared.port, prepared.state.clone());
    let handle = wait_in_background(
        prepared,
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(20),
    );

    let (status, body) = get(port, &format!("/?error=access_denied&state={state}"));
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("did not finish"), "{body}");

    assert_eq!(
        handle.join().expect("thread"),
        Err(BrowserEnd::Callback(CallbackError::Denied))
    );
}

#[test]
fn cancelling_stops_the_wait_quickly() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = wait_in_background(prepared, Arc::clone(&cancel), Duration::from_secs(60));

    let started = Instant::now();
    cancel.store(true, Ordering::SeqCst);
    assert_eq!(handle.join().expect("thread"), Err(BrowserEnd::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn the_wait_gives_up_after_its_timeout() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    let handle = wait_in_background(
        prepared,
        Arc::new(AtomicBool::new(false)),
        Duration::from_millis(200),
    );
    assert_eq!(handle.join().expect("thread"), Err(BrowserEnd::TimedOut));
}

#[test]
fn a_connection_that_sends_nothing_does_not_block_the_real_redirect_for_long() {
    let prepared = prepare(CLIENT_ID).expect("prepare");
    let (port, state) = (prepared.port, prepared.state.clone());
    let handle = wait_in_background(
        prepared,
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(30),
    );

    let _stalled = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let started = Instant::now();
    let (status, _) = get(port, &format!("/?code=abc&state={state}"));
    assert!(status.contains("200"), "{status}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(handle.join().expect("thread").is_ok());
}

// ---- the code exchange ----------------------------------------------------------------

struct Script {
    answer: Result<HttpResponse, ()>,
    seen: Mutex<Vec<HttpRequest>>,
}

impl HttpTransport for Script {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.seen.lock().expect("lock").push(request.clone());
        self.answer
            .clone()
            .map_err(|()| HttpError::transport(&request.url))
    }
}

fn script(answer: Result<HttpResponse, ()>) -> Script {
    Script {
        answer,
        seen: Mutex::new(Vec::new()),
    }
}

fn a_code() -> AuthCode {
    aether_auth::auth_code::parse_callback_query("code=the-code&state=s", "s").expect("code")
}

#[test]
fn the_exchange_sends_the_code_with_the_verifier_and_returns_tokens() {
    let pkce = PkceChallenge::from_verifier("v".repeat(64));
    let transport = script(Ok(HttpResponse::new(
        200,
        r#"{"access_token":"at-123456789012","refresh_token":"rt-123456789012","expires_in":3600}"#
            .to_owned(),
    )));

    let tokens = exchange_code(
        &transport,
        CLIENT_ID,
        &a_code(),
        "http://localhost:5123",
        pkce.verifier(),
        SystemTime::UNIX_EPOCH,
    )
    .expect("tokens");
    assert!(tokens.refresh.is_some());

    let seen = transport.seen.lock().expect("lock");
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].url,
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
    );
    let body = seen[0].body().expect("body");
    assert!(body.contains("grant_type=authorization_code"));
    assert!(body.contains("code=the-code"));
    assert!(body.contains(&format!("code_verifier={}", "v".repeat(64))));
}

#[test]
fn exchange_failures_get_stable_reasons_and_no_secrets() {
    let pkce = PkceChallenge::from_verifier("v".repeat(64));
    let cases = [
        (Err(()), "network"),
        (
            Ok(HttpResponse::new(
                400,
                r#"{"error":"invalid_grant","error_description":"the-code"}"#.to_owned(),
            )),
            "expired",
        ),
        (
            Ok(HttpResponse::new(
                400,
                r#"{"error":"invalid_client"}"#.to_owned(),
            )),
            "service",
        ),
        (Ok(HttpResponse::new(500, "oops".to_owned())), "service"),
    ];
    for (answer, reason) in cases {
        let failure = exchange_code(
            &script(answer),
            CLIENT_ID,
            &a_code(),
            "http://localhost:5123",
            pkce.verifier(),
            SystemTime::UNIX_EPOCH,
        )
        .expect_err("must fail");
        assert_eq!(failure.reason, reason);
        assert!(!failure.message.contains("the-code"), "{}", failure.message);
        assert!(!failure.message.contains(&"v".repeat(64)));
    }
}

#[test]
fn ending_the_wait_maps_to_the_events_the_dialog_knows() {
    assert_eq!(ended(BrowserEnd::Cancelled), LoginEvent::Cancelled);
    let reason = |end| match ended(end) {
        LoginEvent::Failed { failure } => failure.reason,
        other => panic!("{other:?}"),
    };
    assert_eq!(reason(BrowserEnd::TimedOut), "expired");
    assert_eq!(
        reason(BrowserEnd::Callback(CallbackError::Denied)),
        "denied"
    );
    assert_eq!(
        reason(BrowserEnd::Callback(CallbackError::OAuth {
            code: "invalid_request".to_owned()
        })),
        "service"
    );
}
