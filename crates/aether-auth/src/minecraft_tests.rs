//! Offline tests for [`super`] (the Minecraft services stage).
//!
//! Nothing here touches the network: [`FakeTransport`] records the requests it is given
//! and replays canned responses, which lets the flow be exercised end to end.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use super::{
    ENTITLEMENTS_URL, Entitlement, Entitlements, LOGIN_WITH_XBOX_URL, MinecraftError, PROFILE_URL,
    build_entitlements_request, build_login_request, build_profile_request, parse_entitlements,
    parse_login_response, parse_profile,
};
use crate::http::{HttpError, HttpMethod, HttpRequest, HttpResponse, HttpTransport};
use crate::token::{MinecraftToken, XstsToken};

/// A secret-looking token used across the tests to prove it never reaches an error.
const SECRET: &str = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJzdGV2ZSJ9.c2lnbmF0dXJlLXBsYWNlaG9sZGVy";

/// An XSTS user hash with the shape the Xbox service really returns.
const USER_HASH: &str = "XUID_2600000000000001";

/// Replays queued responses and records every request it saw.
struct FakeTransport {
    responses: RefCell<VecDeque<Result<HttpResponse, HttpError>>>,
    seen: RefCell<Vec<HttpRequest>>,
}

impl FakeTransport {
    fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            responses: RefCell::new(responses.into_iter().map(Ok).collect()),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn with_error(error: HttpError) -> Self {
        Self {
            responses: RefCell::new(VecDeque::from([Err(error)])),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn last_request(&self) -> HttpRequest {
        self.seen
            .borrow()
            .last()
            .cloned()
            .expect("a request should have been sent")
    }

    fn requests(&self) -> std::cell::Ref<'_, Vec<HttpRequest>> {
        self.seen.borrow()
    }
}

impl HttpTransport for FakeTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.seen.borrow_mut().push(request.clone());
        self.responses
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| panic!("no canned response left for {}", request.url))
    }
}

fn issued_at() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn minecraft_token() -> MinecraftToken {
    MinecraftToken::new(SECRET)
}

fn login_body(response: &HttpRequest) -> serde_json::Value {
    let body = response.body().expect("login request must carry a body");
    serde_json::from_str(body).expect("login body must be JSON")
}

const LOGIN_JSON: &str = r#"{"username":"Steve","access_token":"eyJhbGciOiJub25lIn0.eyJzdWIiOiJzdGV2ZSJ9.c2lnbmF0dXJlLXBsYWNlaG9sZGVy","expires_in":86400,"token_type":"Bearer"}"#;

// --------------------------------------------------------------------------------------
// Step 1: login_with_xbox
// --------------------------------------------------------------------------------------

#[test]
fn login_request_targets_the_login_endpoint_with_json() {
    let request = build_login_request(USER_HASH, &XstsToken::new("xsts-secret-value-1234"));

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.url, LOGIN_WITH_XBOX_URL);
    assert_eq!(
        request.header("content-type"),
        Some("application/json"),
        "login_with_xbox expects a JSON body"
    );
}

#[test]
fn login_body_has_the_exact_identity_token_format() {
    let request = build_login_request(USER_HASH, &XstsToken::new("xsts-secret-value-1234"));
    let body = login_body(&request);
    let objects = body.as_object().expect("body is a JSON object");

    assert_eq!(objects.len(), 1, "only identityToken is sent");
    assert_eq!(
        objects["identityToken"],
        serde_json::json!(format!("XBL3.0 x={USER_HASH};xsts-secret-value-1234"))
    );
    assert_eq!(
        objects["identityToken"].as_str(),
        Some("XBL3.0 x=XUID_2600000000000001;xsts-secret-value-1234"),
        "prefix is 'XBL3.0 x=' and the separator before the token is ';'"
    );
}

#[test]
fn login_body_separator_is_a_semicolon_not_a_colon() {
    let request = build_login_request("uid", &XstsToken::new("tok"));
    let identity = login_body(&request)["identityToken"]
        .as_str()
        .expect("identityToken is a string")
        .to_owned();

    assert!(
        identity.contains(';'),
        "expected ';' separator in {identity}"
    );
    assert_eq!(identity, "XBL3.0 x=uid;tok");
}

#[test]
fn login_parse_extracts_token_lifetime_and_username() {
    let session = parse_login_response(200, LOGIN_JSON, issued_at()).expect("valid login body");

    assert_eq!(session.access.expose(), SECRET);
    assert_eq!(session.lifetime.issued_at(), issued_at());
    assert_eq!(session.lifetime.expires_in(), Duration::from_secs(86_400));
    assert_eq!(
        session.lifetime.expires_at(),
        issued_at() + Duration::from_secs(86_400)
    );
    assert_eq!(session.username.as_deref(), Some("Steve"));
}

#[test]
fn login_parse_without_username_yields_none() {
    let body = r#"{"access_token":"token-value-abcdef","expires_in":3600,"token_type":"Bearer"}"#;
    let session = parse_login_response(200, body, issued_at()).expect("username is optional");

    assert_eq!(session.access.expose(), "token-value-abcdef");
    assert_eq!(session.lifetime.expires_in(), Duration::from_secs(3_600));
    assert_eq!(session.username, None);
}

#[test]
fn login_parse_treats_an_empty_username_as_absent() {
    let body = r#"{"username":"","access_token":"token-value-abcdef","expires_in":60}"#;
    let session = parse_login_response(200, body, issued_at()).expect("username is optional");

    assert_eq!(session.username, None);
}

#[test]
fn login_parse_maps_401_and_403_to_unauthorized() {
    for status in [401, 403] {
        let error = parse_login_response(status, LOGIN_JSON, issued_at())
            .expect_err("auth failures must not parse");
        assert!(
            matches!(error, MinecraftError::Unauthorized),
            "status {status} should map to Unauthorized, got {error:?}"
        );
    }
}

#[test]
fn login_parse_keeps_other_failures_as_unexpected_status() {
    let error = parse_login_response(500, "{}", issued_at()).expect_err("5xx is not a success");

    assert!(matches!(
        error,
        MinecraftError::UnexpectedStatus { status: 500 }
    ));
}

#[test]
fn login_parse_rejects_a_body_without_access_token() {
    let body = r#"{"username":"Steve","expires_in":86400}"#;
    let error = parse_login_response(200, body, issued_at()).expect_err("access_token is required");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

#[test]
fn login_parse_rejects_a_body_without_expires_in() {
    let body = r#"{"username":"Steve","access_token":"token-value-abcdef"}"#;
    let error = parse_login_response(200, body, issued_at()).expect_err("expires_in is required");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

#[test]
fn login_parse_rejects_an_empty_access_token() {
    let body = r#"{"access_token":"","expires_in":86400}"#;
    let error = parse_login_response(200, body, issued_at()).expect_err("empty token is unusable");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

#[test]
fn login_parse_rejects_a_zero_lifetime() {
    // A session that is expired on arrival can only confuse the caller; the device code
    // flow rejects the same shape.
    let body = r#"{"username":"Steve","access_token":"token-value-abcdef","expires_in":0}"#;
    let error =
        parse_login_response(200, body, issued_at()).expect_err("zero lifetime is unusable");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

#[test]
fn login_parse_rejects_a_non_json_body() {
    let error = parse_login_response(200, "<html>nope</html>", issued_at())
        .expect_err("html is not a login response");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

// --------------------------------------------------------------------------------------
// Step 2: entitlements
// --------------------------------------------------------------------------------------

#[test]
fn entitlements_request_is_an_authenticated_get() {
    let request = build_entitlements_request(&minecraft_token());

    assert_eq!(request.method, HttpMethod::Get);
    assert_eq!(request.url, ENTITLEMENTS_URL);
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {SECRET}").as_str())
    );
    assert_eq!(request.body(), None, "a GET carries no body");
}

#[test]
fn owns_minecraft_for_product_minecraft() {
    let body = r#"{"items":[{"name":"product_minecraft","signature":"sig"}],"signature":"outer"}"#;
    let entitlements = parse_entitlements(200, body).expect("valid entitlements");

    assert!(entitlements.owns_minecraft());
    assert!(!entitlements.is_empty());
    assert_eq!(
        entitlements.items,
        vec![Entitlement {
            name: "product_minecraft".to_owned(),
            signature: "sig".to_owned(),
        }]
    );
}

#[test]
fn owns_minecraft_for_game_minecraft() {
    let body = r#"{"items":[{"name":"game_minecraft","signature":"sig"}]}"#;
    let entitlements = parse_entitlements(200, body).expect("valid entitlements");

    assert!(entitlements.owns_minecraft());
}

#[test]
fn other_entitlements_do_not_grant_the_game() {
    let body = r#"{"items":[
        {"name":"product_game_pass_ultimate","signature":"a"},
        {"name":"product_minecraft_demo","signature":"b"}
    ]}"#;
    let entitlements = parse_entitlements(200, body).expect("valid entitlements");

    assert!(!entitlements.owns_minecraft(), "names must match exactly");
    assert!(!entitlements.is_empty());
}

#[test]
fn empty_items_means_no_ownership_without_error() {
    let entitlements = parse_entitlements(200, r#"{"items":[]}"#).expect("empty list is valid");

    assert!(entitlements.is_empty());
    assert!(!entitlements.owns_minecraft());
    assert_eq!(entitlements, Entitlements::default());
}

#[test]
fn missing_items_means_no_ownership_without_error() {
    let entitlements =
        parse_entitlements(200, r#"{"signature":"outer"}"#).expect("missing items is valid");

    assert!(entitlements.is_empty());
    assert!(!entitlements.owns_minecraft());
}

#[test]
fn missing_item_signature_defaults_to_empty_string() {
    let body = r#"{"items":[{"name":"product_minecraft"}]}"#;
    let entitlements = parse_entitlements(200, body).expect("signature is optional");

    assert_eq!(entitlements.items[0].signature, "");
    assert!(entitlements.owns_minecraft());
}

#[test]
fn entitlements_reject_unauthenticated_statuses() {
    for status in [401, 403] {
        let error = parse_entitlements(status, "{}").expect_err("auth failure");
        assert!(matches!(error, MinecraftError::Unauthorized));
    }

    let error = parse_entitlements(502, "{}").expect_err("bad gateway");
    assert!(matches!(
        error,
        MinecraftError::UnexpectedStatus { status: 502 }
    ));
}

#[test]
fn entitlements_reject_a_non_json_body() {
    let error = parse_entitlements(200, "not json").expect_err("garbage body");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

#[test]
fn entitlements_reject_a_wrong_items_shape() {
    let error = parse_entitlements(200, r#"{"items":42}"#).expect_err("items must be an array");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

// --------------------------------------------------------------------------------------
// Step 3: profile
// --------------------------------------------------------------------------------------

#[test]
fn profile_request_is_an_authenticated_get() {
    let request = build_profile_request(&minecraft_token());

    assert_eq!(request.method, HttpMethod::Get);
    assert_eq!(request.url, PROFILE_URL);
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {SECRET}").as_str()),
        "profile requires 'authorization: Bearer <token>'"
    );
    assert_eq!(request.body(), None, "a GET carries no body");
}

#[test]
fn profile_parses_an_undashed_id() {
    let body = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[],"capes":[]}"#;
    let profile = parse_profile(200, body).expect("valid profile");

    assert_eq!(
        profile.id,
        Uuid::parse_str("069a79f4-44e9-4726-a5be-fca90e38aaf5").expect("hyphenated uuid")
    );
    assert_eq!(profile.name, "Notch");
    assert_eq!(profile.dashed_id(), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
}

#[test]
fn profile_tolerates_extra_fields_and_a_dashed_id() {
    let body = r#"{"id":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"Notch"}"#;
    let profile = parse_profile(200, body).expect("hyphenated ids are accepted too");

    assert_eq!(profile.undashed_id(), "069a79f444e94726a5befca90e38aaf5");
}

#[test]
fn profile_undashed_id_is_32_hex_digits_without_hyphens() {
    let body = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;
    let profile = parse_profile(200, body).expect("valid profile");
    let undashed = profile.undashed_id();

    assert_eq!(undashed.len(), 32, "the game protocol wants 32 characters");
    assert!(!undashed.contains('-'), "no hyphens in {undashed}");
    assert!(undashed.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(undashed, profile.dashed_id());
}

#[test]
fn profile_404_reports_a_missing_profile() {
    let error = parse_profile(404, r#"{"error":"NOT_FOUND"}"#).expect_err("404 is not a profile");

    assert!(matches!(error, MinecraftError::ProfileNotFound));
    let message = error.to_string();
    assert!(
        message.contains("profile"),
        "the message should explain the missing profile, got: {message}"
    );
    assert!(
        message.contains("player name"),
        "the message should tell the user how to fix it, got: {message}"
    );
    assert!(
        !message.contains(r#"{"error""#),
        "no response body in errors"
    );
}

#[test]
fn profile_maps_auth_failures_and_other_statuses_distinctly() {
    for status in [401, 403] {
        let error = parse_profile(status, "{}").expect_err("auth failure");
        assert!(matches!(error, MinecraftError::Unauthorized));
    }

    let error = parse_profile(500, "{}").expect_err("server error");
    assert!(matches!(
        error,
        MinecraftError::UnexpectedStatus { status: 500 }
    ));
}

#[test]
fn profile_rejects_a_malformed_id() {
    for id in [
        "",
        "not-a-uuid",
        "069a79f444e94726a5befca90e38aaf",
        "zzza79f444e94726a5befca90e38aaf5",
    ] {
        let body = format!(r#"{{"id":"{id}","name":"Notch"}}"#);
        let error = parse_profile(200, &body).expect_err("bad ids must not parse");
        assert!(
            matches!(error, MinecraftError::MalformedResponse),
            "id {id:?} should be rejected"
        );
    }
}

#[test]
fn profile_rejects_a_missing_name() {
    let error = parse_profile(200, r#"{"id":"069a79f444e94726a5befca90e38aaf5"}"#)
        .expect_err("name is required");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

#[test]
fn profile_rejects_a_non_json_body() {
    let error = parse_profile(200, "<html>nope</html>").expect_err("html is not a profile");

    assert!(matches!(error, MinecraftError::MalformedResponse));
}

// --------------------------------------------------------------------------------------
// Error redaction
// --------------------------------------------------------------------------------------

#[test]
fn errors_never_leak_the_token_url_or_response_body() {
    let token = minecraft_token();
    let request = build_profile_request(&token);
    let request_body_marker = "response-body-marker";

    let errors = [
        parse_login_response(401, LOGIN_JSON, issued_at()).expect_err("401"),
        parse_login_response(500, LOGIN_JSON, issued_at()).expect_err("500"),
        parse_login_response(200, "garbage", issued_at()).expect_err("garbage"),
        parse_entitlements(403, "{}").expect_err("403"),
        parse_entitlements(200, request_body_marker).expect_err("garbage"),
        parse_profile(404, request_body_marker).expect_err("404"),
        parse_profile(200, "garbage").expect_err("garbage"),
    ];

    for error in errors {
        for message in [error.to_string(), format!("{error:?}")] {
            assert!(
                !message.contains(SECRET),
                "error leaked the token: {message}"
            );
            assert!(
                !message.contains("api.minecraftservices.com"),
                "error leaked the URL: {message}"
            );
            assert!(
                !message.contains(request_body_marker),
                "error leaked the response body: {message}"
            );
            assert!(
                !message.contains("Bearer"),
                "error must not echo the auth header: {message}"
            );
        }
    }

    // The request itself does carry the token; that is expected, and its `Debug` output
    // must still stay quiet about the body.
    assert!(format!("{request:?}").contains("<redacted"));
}

// --------------------------------------------------------------------------------------
// End to end through a fake transport
// --------------------------------------------------------------------------------------

#[test]
fn full_flow_through_a_fake_transport() {
    let transport = FakeTransport::new(vec![
        HttpResponse::new(200, LOGIN_JSON),
        HttpResponse::new(
            200,
            r#"{"items":[{"name":"product_minecraft","signature":"sig"}],"signature":"outer"}"#,
        ),
        HttpResponse::new(
            200,
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#,
        ),
    ]);

    let xsts = XstsToken::new("xsts-token-value-abcdef");
    let session = transport
        .execute(&build_login_request(USER_HASH, &xsts))
        .and_then(|response| {
            parse_login_response(response.status, &response.body, issued_at()).map_err(|error| {
                HttpError::InvalidEndpoint {
                    reason: Box::leak(error.to_string().into_boxed_str()),
                }
            })
        })
        .expect("login step");
    assert_eq!(session.username.as_deref(), Some("Steve"));

    let entitlements_response = transport
        .execute(&build_entitlements_request(&session.access))
        .expect("entitlements transport");
    let entitlements =
        parse_entitlements(entitlements_response.status, &entitlements_response.body)
            .expect("entitlements step");
    assert!(entitlements.owns_minecraft());

    let profile_response = transport
        .execute(&build_profile_request(&session.access))
        .expect("profile transport");
    let profile = parse_profile(profile_response.status, &profile_response.body).expect("profile");

    assert_eq!(profile.name, "Notch");
    assert_eq!(profile.undashed_id(), "069a79f444e94726a5befca90e38aaf5");

    let requests = transport.requests();
    assert_eq!(requests.len(), 3, "one request per step");
    assert_eq!(requests[0].url, LOGIN_WITH_XBOX_URL);
    assert_eq!(
        login_body(&requests[0])["identityToken"],
        serde_json::json!(format!("XBL3.0 x={USER_HASH};xsts-token-value-abcdef"))
    );
    assert_eq!(requests[1].url, ENTITLEMENTS_URL);
    assert_eq!(requests[2].url, PROFILE_URL);
    for later in &requests[1..] {
        assert_eq!(
            later.header("authorization"),
            Some(format!("Bearer {SECRET}").as_str()),
            "{} must be authenticated",
            later.url
        );
    }
    assert_eq!(
        transport.last_request().url,
        PROFILE_URL,
        "the last request is the profile lookup"
    );
}

#[test]
fn transport_faults_surface_as_http_errors() {
    let transport = FakeTransport::with_error(HttpError::transport(PROFILE_URL));
    let error = transport
        .execute(&build_profile_request(&minecraft_token()))
        .expect_err("the transport is configured to fail");

    assert!(matches!(error, HttpError::Transport { .. }));
    assert_eq!(
        transport.requests().len(),
        1,
        "the request is still recorded for inspection"
    );
}
