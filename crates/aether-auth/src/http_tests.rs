use super::{
    HttpError, HttpMethod, HttpRequest, HttpResponse, HttpTransport, form_encode, host_of,
};

#[test]
fn post_json_carries_json_content_type() {
    let request = HttpRequest::post_json("https://example.invalid/token", "{\"a\":1}");

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(request.body(), Some("{\"a\":1}"));
}

#[test]
fn post_form_carries_form_content_type() {
    let request = HttpRequest::post_form("https://example.invalid/token", "grant_type=x");

    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
}

#[test]
fn get_requests_have_no_body() {
    let request = HttpRequest::get("https://example.invalid/profile");

    assert_eq!(request.method, HttpMethod::Get);
    assert_eq!(request.body(), None);
}

#[test]
fn with_header_replaces_a_case_insensitive_duplicate() {
    let request = HttpRequest::post_json("https://example.invalid/x", "{}")
        .with_header("CONTENT-TYPE", "text/plain");

    let occurrences = request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .count();

    assert_eq!(occurrences, 1);
    assert_eq!(request.header("Content-Type"), Some("text/plain"));
}

#[test]
fn debug_redacts_the_body() {
    let request = HttpRequest::post_form(
        "https://login.example.invalid/token",
        "grant_type=refresh_token&refresh_token=super-secret-value",
    );

    let rendered = format!("{request:?}");

    assert!(!rendered.contains("super-secret-value"));
    assert!(rendered.contains("<redacted:"));
    assert!(rendered.contains("login.example.invalid"));
}

#[test]
fn debug_redacts_sensitive_headers() {
    let token = "eyJhbGciOiJub25lIn0.mc-access-token-1234";
    let request = HttpRequest::get("https://api.example.invalid/minecraft/profile")
        .with_header("Authorization", format!("Bearer {token}"))
        .with_header("Cookie", "session=abc");

    let rendered = format!("{request:?}");

    assert!(!rendered.contains(token), "authorization value leaked");
    assert!(!rendered.contains("session=abc"), "cookie value leaked");
    assert!(rendered.contains("<redacted>"));
    // The header names stay visible so the log stays useful.
    assert!(rendered.contains("authorization"));
}

#[test]
fn debug_keeps_harmless_headers_readable() {
    let request = HttpRequest::post_json("https://example.invalid/x", "{}");

    let rendered = format!("{request:?}");

    assert!(rendered.contains("application/json"));
}

#[test]
fn response_debug_does_not_print_the_body() {
    let body = r#"{"access_token":"minecraft-token-value-9876","expires_in":86400}"#;
    let response = HttpResponse::new(200, body);

    let rendered = format!("{response:?}");

    assert!(!rendered.contains("minecraft-token-value-9876"));
    assert!(!rendered.contains("access_token"));
    assert!(rendered.contains("200"));
    assert!(rendered.contains("<redacted:"));
}

#[test]
fn success_only_covers_2xx() {
    assert!(HttpResponse::new(200, "{}").is_success());
    assert!(HttpResponse::new(204, "").is_success());
    assert!(!HttpResponse::new(301, "").is_success());
    assert!(!HttpResponse::new(400, "{}").is_success());
    assert!(!HttpResponse::new(503, "").is_success());
}

#[test]
fn host_extraction_ignores_path_and_query() {
    assert_eq!(
        host_of("https://example.invalid/a/b?c=d"),
        Some("example.invalid")
    );
    assert_eq!(host_of("https://example.invalid"), Some("example.invalid"));
    assert_eq!(
        host_of("https://example.invalid?x=1"),
        Some("example.invalid")
    );
    assert_eq!(host_of("not a url"), None);
    assert_eq!(host_of("https:///path"), None);
}

#[test]
fn transport_errors_only_mention_the_host() {
    let error = HttpError::transport("https://user.auth.xboxlive.com/user/authenticate?token=abc");

    assert_eq!(
        error.to_string(),
        "network failure while contacting user.auth.xboxlive.com"
    );
    assert!(!error.to_string().contains("token=abc"));
}

#[test]
fn transport_errors_without_a_host_stay_opaque() {
    let error = HttpError::transport("garbage");

    assert_eq!(
        error.to_string(),
        "network failure while contacting <unknown host>"
    );
}

#[test]
fn form_encoding_escapes_reserved_characters() {
    assert_eq!(
        form_encode("XboxLive.signin offline_access"),
        "XboxLive.signin+offline_access"
    );
    assert_eq!(form_encode("a/b:c"), "a%2Fb%3Ac");
    assert_eq!(form_encode("ü"), "%C3%BC");
    assert_eq!(form_encode("Aa0-_.~"), "Aa0-_.~");
    assert_eq!(form_encode("100%"), "100%25");
}

/// The transport is a trait, so a test double is a plain struct with a recorder. This is
/// the only kind of transport any test in this crate is allowed to use.
#[derive(Default)]
struct Recorder {
    seen: std::cell::RefCell<Vec<String>>,
}

impl HttpTransport for Recorder {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.seen.borrow_mut().push(request.url.clone());
        Ok(HttpResponse::new(200, "{}"))
    }
}

#[test]
fn the_trait_can_be_implemented_by_an_in_memory_double() {
    let transport = Recorder::default();

    let response = transport
        .execute(&HttpRequest::get("https://example.invalid/x"))
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(
        transport.seen.borrow().as_slice(),
        ["https://example.invalid/x"]
    );
}
