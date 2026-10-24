use std::cell::RefCell;
use std::collections::VecDeque;

use serde_json::Value;

use super::{
    MINECRAFT_RELYING_PARTY, XBL_AUTHENTICATE_URL, XSTS_AUTHORIZE_URL, XblSession, XblToken,
    XboxError, XstsSession, build_xbl_authenticate_request, build_xsts_authorize_request,
    parse_xbl_response, parse_xsts_response,
};
use crate::http::{HttpError, HttpMethod, HttpRequest, HttpResponse, HttpTransport};

const ACCESS_TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.access-payload.signature-1234";
const XBL_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IlJQIn0.xbl-payload.xbl-signature-5678";
const XSTS_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IlhTVFMifQ.xsts-payload.xsts-signature-9012";
const USER_HASH: &str = "2535400000000000";
const XUID: &str = "2535400000000001";

/// The transport is a trait, so tests hand in a scripted double that records every
/// request and replays pre-baked replies. Nothing in this file touches the network.
struct FakeTransport {
    responses: RefCell<VecDeque<Result<HttpResponse, HttpError>>>,
    seen: RefCell<Vec<HttpRequest>>,
}

impl FakeTransport {
    fn new(responses: Vec<Result<HttpResponse, HttpError>>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn seen(&self) -> std::cell::Ref<'_, Vec<HttpRequest>> {
        self.seen.borrow()
    }
}

impl HttpTransport for FakeTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.seen.borrow_mut().push(request.clone());
        self.responses
            .borrow_mut()
            .pop_front()
            .unwrap_or(Err(HttpError::Transport {
                host: "unscripted".to_owned(),
            }))
    }
}

fn xbl_body(token: &str, uhs: &str) -> String {
    format!(r#"{{"Token":"{token}","DisplayClaims":{{"xui":[{{"uhs":"{uhs}"}}]}}}}"#)
}

fn xsts_body(token: &str, uhs: &str, xid: &str) -> String {
    format!(r#"{{"Token":"{token}","DisplayClaims":{{"xui":[{{"uhs":"{uhs}","xid":"{xid}"}}]}}}}"#)
}

/// The body the real XSTS endpoint sends for an account-state problem.
fn xsts_error_body(code: &str) -> String {
    format!(
        r#"{{"Identity":"0","XErr":{code},"Message":"","Redirect":"https://start.ui.xboxlive.com/CreateAccount"}}"#
    )
}

fn parse_body(body: &str) -> Value {
    serde_json::from_str(body).expect("request body must be JSON")
}

// --------------------------------------------------------------------------------------
// Step 1: XBL authenticate request
// --------------------------------------------------------------------------------------

#[test]
fn xbl_request_posts_json_to_the_authenticate_endpoint() {
    let request = build_xbl_authenticate_request(&crate::token::AccessToken::new(ACCESS_TOKEN));

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.url, XBL_AUTHENTICATE_URL);
    assert_eq!(
        request.url,
        "https://user.auth.xboxlive.com/user/authenticate"
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("x-xbl-contract-version"), Some("1"));
}

#[test]
fn xbl_request_body_matches_the_rps_document() {
    let request = build_xbl_authenticate_request(&crate::token::AccessToken::new(ACCESS_TOKEN));

    let body = parse_body(request.body().expect("xbl request has a body"));

    assert_eq!(body["Properties"]["AuthMethod"], "RPS");
    assert_eq!(body["Properties"]["SiteName"], "user.auth.xboxlive.com");
    assert_eq!(body["RelyingParty"], "http://auth.xboxlive.com");
    assert_eq!(body["TokenType"], "JWT");
    assert_eq!(body["Properties"]["RpsTicket"], format!("d={ACCESS_TOKEN}"));
}

#[test]
fn xbl_request_prefixes_the_ticket_with_d_equals() {
    let request = build_xbl_authenticate_request(&crate::token::AccessToken::new("plain-token"));

    let body = request.body().expect("xbl request has a body");
    let ticket = body
        .split("\"RpsTicket\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next());

    assert_eq!(ticket, Some("d=plain-token"));
}

#[test]
fn xbl_request_escapes_the_token_instead_of_interpolating_it() {
    let hostile = "quote\"brace}back\\slash";
    let request = build_xbl_authenticate_request(&crate::token::AccessToken::new(hostile));

    let body = request.body().expect("xbl request has a body");
    let parsed = parse_body(body);

    assert_eq!(parsed["Properties"]["RpsTicket"], format!("d={hostile}"));
    assert!(!body.contains("\"RpsTicket\":\"d=quote\""));
}

// --------------------------------------------------------------------------------------
// Step 1: XBL response
// --------------------------------------------------------------------------------------

#[test]
fn xbl_response_yields_the_token_and_the_user_hash() {
    let session = parse_xbl_response(200, &xbl_body(XBL_TOKEN, USER_HASH)).expect("valid response");

    assert_eq!(session.token.expose(), XBL_TOKEN);
    assert_eq!(session.user_hash, USER_HASH);
    assert!(!session.token.is_empty());
}

#[test]
fn xbl_response_with_an_empty_xui_list_is_malformed() {
    let body = r#"{"Token":"whatever-token-value","DisplayClaims":{"xui":[]}}"#;

    assert!(matches!(
        parse_xbl_response(200, body),
        Err(XboxError::MalformedResponse)
    ));
}

#[test]
fn xbl_response_without_display_claims_is_malformed() {
    let body = r#"{"Token":"whatever-token-value"}"#;

    assert!(matches!(
        parse_xbl_response(200, body),
        Err(XboxError::MalformedResponse)
    ));
}

#[test]
fn xbl_response_without_a_token_is_malformed() {
    let body = r#"{"DisplayClaims":{"xui":[{"uhs":"2535400000000000"}]}}"#;

    assert!(matches!(
        parse_xbl_response(200, body),
        Err(XboxError::MalformedResponse)
    ));
}

#[test]
fn a_non_json_success_body_is_malformed() {
    assert!(matches!(
        parse_xbl_response(200, "<html>nope</html>"),
        Err(XboxError::MalformedResponse)
    ));
    assert!(matches!(
        parse_xsts_response(200, ""),
        Err(XboxError::MalformedResponse)
    ));
}

// --------------------------------------------------------------------------------------
// Step 2: XSTS authorize request
// --------------------------------------------------------------------------------------

#[test]
fn xsts_request_posts_json_to_the_authorize_endpoint() {
    let request = build_xsts_authorize_request(&XblToken::new(XBL_TOKEN));

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.url, XSTS_AUTHORIZE_URL);
    assert_eq!(request.url, "https://xsts.auth.xboxlive.com/xsts/authorize");
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("x-xbl-contract-version"), Some("1"));
}

#[test]
fn xsts_request_uses_the_exact_minecraft_relying_party() {
    let request = build_xsts_authorize_request(&XblToken::new(XBL_TOKEN));

    let body = parse_body(request.body().expect("xsts request has a body"));

    assert_eq!(MINECRAFT_RELYING_PARTY, "rp://api.minecraftservices.com/");
    assert_eq!(body["RelyingParty"], MINECRAFT_RELYING_PARTY);
    assert_eq!(body["RelyingParty"], "rp://api.minecraftservices.com/");
}

#[test]
fn xsts_request_asks_for_the_retail_sandbox_and_carries_the_user_token() {
    let request = build_xsts_authorize_request(&XblToken::new(XBL_TOKEN));

    let body = parse_body(request.body().expect("xsts request has a body"));

    assert_eq!(body["Properties"]["SandboxId"], "RETAIL");
    assert_eq!(body["TokenType"], "JWT");
    assert_eq!(
        body["Properties"]["UserTokens"],
        Value::Array(vec![Value::from(XBL_TOKEN)])
    );
}

// --------------------------------------------------------------------------------------
// Step 2: XSTS response
// --------------------------------------------------------------------------------------

#[test]
fn xsts_response_yields_the_token_the_user_hash_and_the_xuid() {
    let session =
        parse_xsts_response(200, &xsts_body(XSTS_TOKEN, USER_HASH, XUID)).expect("valid response");

    assert_eq!(session.token.expose(), XSTS_TOKEN);
    assert_eq!(session.user_hash, USER_HASH);
    assert_eq!(session.xuid, XUID);
}

#[test]
fn xsts_response_without_xid_is_malformed() {
    let body = xbl_body(XSTS_TOKEN, USER_HASH);

    assert!(matches!(
        parse_xsts_response(200, &body),
        Err(XboxError::MalformedResponse)
    ));
}

// --------------------------------------------------------------------------------------
// XErr translation
// --------------------------------------------------------------------------------------

#[test]
fn a_missing_xbox_profile_has_a_specific_message() {
    let error = parse_xsts_response(401, &xsts_error_body("2148916233")).unwrap_err();

    assert!(matches!(error, XboxError::NoXboxProfile));
    assert_eq!(
        error.to_string(),
        "this Microsoft account does not have an Xbox profile yet; create one at xbox.com before signing in"
    );
}

#[test]
fn an_unsupported_region_has_a_specific_message() {
    let error = parse_xsts_response(401, &xsts_error_body("2148916235")).unwrap_err();

    assert!(matches!(error, XboxError::RegionUnsupported));
    assert_eq!(
        error.to_string(),
        "Xbox Live is not available in the region this account is registered in"
    );
}

#[test]
fn a_child_account_has_a_specific_message() {
    let error = parse_xsts_response(401, &xsts_error_body("2148916238")).unwrap_err();

    assert!(matches!(error, XboxError::ChildAccount));
    assert_eq!(
        error.to_string(),
        "this is a child account; an adult must add it to a Microsoft family group before it can play online"
    );
}

#[test]
fn xerr_is_accepted_as_a_string_too() {
    let body = xsts_error_body("\"2148916238\"");

    assert!(matches!(
        parse_xsts_response(401, &body),
        Err(XboxError::ChildAccount)
    ));
}

#[test]
fn an_unknown_xerr_code_keeps_the_number() {
    let error = parse_xsts_response(401, &xsts_error_body("2148916236")).unwrap_err();

    assert!(matches!(
        error,
        XboxError::XstsCode {
            code: 2_148_916_236
        }
    ));
    assert_eq!(
        error.to_string(),
        "Xbox Live rejected the sign-in with error code 2148916236"
    );
}

#[test]
fn an_unknown_xerr_code_arrives_as_a_string_as_well() {
    let error = parse_xsts_response(401, &xsts_error_body("\"2148916236\"")).unwrap_err();

    assert!(matches!(
        error,
        XboxError::XstsCode {
            code: 2_148_916_236
        }
    ));
}

#[test]
fn xbl_errors_go_through_the_same_translation() {
    let error = parse_xbl_response(401, &xsts_error_body("2148916233")).unwrap_err();

    assert!(matches!(error, XboxError::NoXboxProfile));
}

#[test]
fn an_unsuccessful_status_without_xerr_keeps_the_status() {
    let error = parse_xsts_response(500, "upstream failure").unwrap_err();

    assert!(matches!(error, XboxError::UnexpectedStatus { status: 500 }));
    assert_eq!(
        error.to_string(),
        "the Xbox service returned an unexpected status 500"
    );

    let xbl = parse_xbl_response(403, r#"{"Message":"forbidden"}"#).unwrap_err();
    assert!(matches!(xbl, XboxError::UnexpectedStatus { status: 403 }));
}

#[test]
fn a_non_numeric_xerr_falls_back_to_the_status() {
    let error = parse_xsts_response(401, r#"{"XErr":"not-a-number"}"#).unwrap_err();

    assert!(matches!(error, XboxError::UnexpectedStatus { status: 401 }));
}

#[test]
fn an_error_status_beats_a_well_formed_body() {
    // The token must not be surfaced just because the body happened to be parseable.
    let error = parse_xsts_response(401, &xsts_body(XSTS_TOKEN, USER_HASH, XUID)).unwrap_err();

    assert!(matches!(error, XboxError::UnexpectedStatus { status: 401 }));
}

// --------------------------------------------------------------------------------------
// Token redaction
// --------------------------------------------------------------------------------------

#[test]
fn xbl_token_debug_reveals_only_the_last_four_characters() {
    let token = XblToken::new(XBL_TOKEN);

    let rendered = format!("{token:?}");

    assert_eq!(rendered, "XblToken(****5678)");
    assert!(!rendered.contains("xbl-payload"));
    assert!(!rendered.contains(XBL_TOKEN));
    assert_eq!(format!("{token}"), "XblToken(****5678)");
}

#[test]
fn short_xbl_tokens_are_fully_hidden() {
    let token = XblToken::new("short1234");

    assert_eq!(format!("{token:?}"), "XblToken(****)");
    assert!(!format!("{token:?}").contains("1234"));
    assert_eq!(format!("{token}"), "XblToken(****)");
}

#[test]
fn session_debug_is_redacted() {
    let body = xsts_body(XSTS_TOKEN, USER_HASH, XUID);
    let session = parse_xsts_response(200, &body).unwrap();

    let rendered = format!("{session:?}");

    assert!(rendered.contains("XstsToken(****9012)"));
    assert!(!rendered.contains(XSTS_TOKEN));
    assert!(!rendered.contains("xsts-payload"));
}

// --------------------------------------------------------------------------------------
// No token ever reaches an error message
// --------------------------------------------------------------------------------------

#[test]
fn error_messages_never_echo_the_response_body() {
    let replies = [
        (401, xsts_error_body("2148916233")),
        (401, xsts_error_body("2148916235")),
        (401, xsts_error_body("2148916238")),
        (401, xsts_error_body("999999999")),
        (401, xsts_body(XSTS_TOKEN, USER_HASH, XUID)),
        (500, xsts_body(XSTS_TOKEN, USER_HASH, XUID)),
        (200, "not json at all".to_string()),
    ];

    for (status, body) in replies {
        for error in [
            parse_xbl_response(status, &body).unwrap_err(),
            parse_xsts_response(status, &body).unwrap_err(),
        ] {
            let message = error.to_string();
            assert!(!message.contains(XSTS_TOKEN), "leaked the token: {message}");
            assert!(
                !message.contains("xsts-payload"),
                "leaked the token: {message}"
            );
            assert!(
                !message.contains("start.ui.xboxlive.com"),
                "leaked the redirect: {message}"
            );
            assert!(!message.contains("https://"), "leaked a URL: {message}");
            assert!(!message.contains('<'), "leaked the body: {message}");
        }
    }
}

#[test]
fn request_debug_does_not_leak_the_access_token() {
    let request = build_xbl_authenticate_request(&crate::token::AccessToken::new(ACCESS_TOKEN));

    let rendered = format!("{request:?}");

    assert!(!rendered.contains(ACCESS_TOKEN));
    assert!(!rendered.contains("access-payload"));
}

// --------------------------------------------------------------------------------------
// Whole-chain test through the transport boundary
// --------------------------------------------------------------------------------------

#[test]
fn the_two_steps_chain_through_a_transport() {
    let transport = FakeTransport::new(vec![
        Ok(HttpResponse::new(200, xbl_body(XBL_TOKEN, USER_HASH))),
        Ok(HttpResponse::new(
            200,
            xsts_body(XSTS_TOKEN, USER_HASH, XUID),
        )),
    ]);

    // Step 1: Microsoft access token -> XBL session.
    let access = crate::token::AccessToken::new(ACCESS_TOKEN);
    let xbl_request = build_xbl_authenticate_request(&access);
    let xbl_response = transport.execute(&xbl_request).expect("scripted reply");
    let xbl_session: XblSession =
        parse_xbl_response(xbl_response.status, &xbl_response.body).expect("xbl parses");

    // Step 2: XBL session -> XSTS session.
    let xsts_request = build_xsts_authorize_request(&xbl_session.token);
    let xsts_response = transport.execute(&xsts_request).expect("scripted reply");
    let xsts_session: XstsSession =
        parse_xsts_response(xsts_response.status, &xsts_response.body).expect("xsts parses");

    let seen = transport.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].url, XBL_AUTHENTICATE_URL);
    assert_eq!(seen[1].url, XSTS_AUTHORIZE_URL);
    assert_eq!(seen[1].header("x-xbl-contract-version"), Some("1"));
    assert_eq!(
        parse_body(seen[1].body().expect("xsts body"))["Properties"]["UserTokens"][0],
        XBL_TOKEN
    );
    assert_eq!(xbl_session.user_hash, xsts_session.user_hash);
    assert_eq!(xsts_session.xuid, XUID);
    assert_eq!(xsts_session.token.expose(), XSTS_TOKEN);
}

#[test]
fn a_transport_fault_stops_the_chain_before_any_parsing() {
    let transport = FakeTransport::new(vec![Err(HttpError::transport(XBL_AUTHENTICATE_URL))]);

    let access = crate::token::AccessToken::new(ACCESS_TOKEN);
    let error = transport
        .execute(&build_xbl_authenticate_request(&access))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "network failure while contacting user.auth.xboxlive.com"
    );
    assert!(!error.to_string().contains(ACCESS_TOKEN));
    assert_eq!(transport.seen().len(), 1);
}
