use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use super::{
    DEFAULT_SCOPE, DEVICE_CODE_URL, DeviceCode, DeviceCodeAuthorization, DeviceCodeEndpoints,
    DeviceCodeError, DeviceCodePoll, DeviceCodeRequest, PollAction, PollOutcome, TOKEN_URL,
    TokenPollRequest, parse_token_response,
};
use crate::http::{HttpError, HttpMethod, HttpRequest, HttpResponse, HttpTransport};
use crate::token::MicrosoftTokens;

/// A device code long enough to spare a redacted tail.
const DEVICE_CODE: &str = "device-code-value-1234567890-abcd";
const USER_CODE: &str = "ABCD1234";
const CLIENT_ID: &str = "00000000-0000-0000-0000-00000000dead";
const ACCESS_TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.payload.signature-9876";
const REFRESH_TOKEN: &str = "M.R3_BAY.refresh-value-4321";

fn epoch() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn challenge(interval_secs: u64, expires_secs: u64) -> DeviceCodeAuthorization {
    DeviceCodeAuthorization {
        user_code: USER_CODE.to_owned(),
        verification_uri: "https://microsoft.com/devicelogin".to_owned(),
        device_code: DeviceCode::new(DEVICE_CODE),
        expires_in: Duration::from_secs(expires_secs),
        interval: Duration::from_secs(interval_secs),
    }
}

fn authorized_outcome(expires_in: u64) -> PollOutcome {
    parse_token_response(
        200,
        &format!(
            r#"{{"access_token":"{ACCESS_TOKEN}","refresh_token":"{REFRESH_TOKEN}","expires_in":{expires_in},"token_type":"Bearer"}}"#
        ),
        epoch(),
    )
    .expect("the fixture parses")
}

fn tokens_of(outcome: &PollOutcome) -> &MicrosoftTokens {
    match outcome {
        PollOutcome::Authorized(tokens) => tokens,
        other => panic!("expected Authorized, got {other:?}"),
    }
}

// --------------------------------------------------------------------------------------
// Request construction
// --------------------------------------------------------------------------------------

#[test]
fn device_code_request_is_a_form_post_to_the_configured_endpoint() {
    let endpoints = DeviceCodeEndpoints::microsoft();
    let request = DeviceCodeRequest::new(CLIENT_ID).to_http(&endpoints);

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.url, DEVICE_CODE_URL);
    assert_eq!(
        request.header("Content-Type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(request.header("Accept"), Some("application/json"));
    assert_eq!(
        request.body(),
        Some(format!("client_id={CLIENT_ID}&scope=XboxLive.signin+offline_access",).as_str())
    );
}

#[test]
fn device_code_request_keeps_the_default_scope() {
    assert_eq!(DeviceCodeRequest::new(CLIENT_ID).scope, DEFAULT_SCOPE);

    let custom = DeviceCodeRequest::new(CLIENT_ID).with_scope("XboxLive.signin");
    let request = custom.to_http(&DeviceCodeEndpoints::microsoft());

    assert_eq!(
        request.body(),
        Some(format!("client_id={CLIENT_ID}&scope=XboxLive.signin").as_str())
    );
}

#[test]
fn device_code_request_encodes_reserved_characters() {
    let request = DeviceCodeRequest::new("client id/with:symbols")
        .with_scope("XboxLive.signin&extra=1")
        .to_http(&DeviceCodeEndpoints::microsoft());

    assert_eq!(
        request.body(),
        Some("client_id=client+id%2Fwith%3Asymbols&scope=XboxLive.signin%26extra%3D1")
    );
}

#[test]
fn token_poll_request_encodes_the_grant_type_client_id_and_device_code() {
    let endpoints = DeviceCodeEndpoints::microsoft();
    let request =
        TokenPollRequest::new(CLIENT_ID, DeviceCode::new(DEVICE_CODE)).to_http(&endpoints);

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.url, TOKEN_URL);
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        request.body(),
        Some(
            format!(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id={CLIENT_ID}&device_code={DEVICE_CODE}"
            )
            .as_str()
        )
    );
    // The separators of the URN are percent-encoded rather than sent raw.
    assert!(!request.body().unwrap().contains("grant_type=urn:"));
}

#[test]
fn endpoints_are_injectable_and_default_to_microsoft() {
    let custom = DeviceCodeEndpoints {
        device_code_url: "https://login.example.invalid/devicecode".to_owned(),
        token_url: "https://login.example.invalid/token".to_owned(),
    };

    assert_eq!(
        DeviceCodeEndpoints::default(),
        DeviceCodeEndpoints::microsoft()
    );

    let first = DeviceCodeRequest::new(CLIENT_ID).to_http(&custom);
    let second = TokenPollRequest::new(CLIENT_ID, DeviceCode::new(DEVICE_CODE)).to_http(&custom);

    assert_eq!(first.url, "https://login.example.invalid/devicecode");
    assert_eq!(second.url, "https://login.example.invalid/token");
}

/// An in-memory transport. No test in this module is allowed to reach a real socket, so the
/// `HttpTransport` seam is exercised with queued answers instead.
#[derive(Default)]
struct FakeTransport {
    responses: RefCell<VecDeque<Result<HttpResponse, HttpError>>>,
    seen: RefCell<Vec<HttpRequest>>,
}

impl FakeTransport {
    fn with(responses: Vec<Result<HttpResponse, HttpError>>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.seen.borrow().len()
    }
}

impl HttpTransport for FakeTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.seen.borrow_mut().push(request.clone());
        self.responses
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Err(HttpError::transport(&request.url)))
    }
}

#[test]
fn the_requests_run_through_an_injected_transport() {
    let transport = FakeTransport::with(vec![
        Ok(HttpResponse::new(
            200,
            format!(
                r#"{{"user_code":"{USER_CODE}","verification_uri":"https://microsoft.com/devicelogin","device_code":"{DEVICE_CODE}","expires_in":900,"interval":5}}"#
            ),
        )),
        Ok(HttpResponse::new(
            400,
            r#"{"error":"authorization_pending"}"#,
        )),
    ]);
    let endpoints = DeviceCodeEndpoints::microsoft();

    let first = transport
        .execute(&DeviceCodeRequest::new(CLIENT_ID).to_http(&endpoints))
        .unwrap();
    let authorization = DeviceCodeAuthorization::from_json(&first.body).unwrap();
    let second = transport
        .execute(
            &TokenPollRequest::new(CLIENT_ID, authorization.device_code.clone())
                .to_http(&endpoints),
        )
        .unwrap();
    let outcome = parse_token_response(second.status, &second.body, epoch()).unwrap();

    assert_eq!(outcome, PollOutcome::Pending);
    assert_eq!(transport.calls(), 2);

    let seen = transport.seen.borrow();
    assert_eq!(seen[0].url, DEVICE_CODE_URL);
    assert_eq!(seen[1].url, TOKEN_URL);
    // The recorder proves the device code travelled in the body, not in the URL.
    assert!(seen[1].body().unwrap().contains(DEVICE_CODE));
    assert!(!seen[1].url.contains(DEVICE_CODE));
}

#[test]
fn transport_faults_do_not_mention_the_device_code() {
    let transport = FakeTransport::with(vec![Err(HttpError::transport(TOKEN_URL))]);
    let request = TokenPollRequest::new(CLIENT_ID, DeviceCode::new(DEVICE_CODE))
        .to_http(&DeviceCodeEndpoints::microsoft());

    let error = transport.execute(&request).unwrap_err();

    assert_eq!(
        error.to_string(),
        "network failure while contacting login.microsoftonline.com"
    );
    assert!(!error.to_string().contains(DEVICE_CODE));
}

// --------------------------------------------------------------------------------------
// Authorization parsing
// --------------------------------------------------------------------------------------

#[test]
fn authorization_parses_the_documented_response() {
    let body = format!(
        r#"{{
            "user_code": "{USER_CODE}",
            "device_code": "{DEVICE_CODE}",
            "verification_uri": "https://microsoft.com/devicelogin",
            "expires_in": 900,
            "interval": 5,
            "message": "To sign in, use a web browser..."
        }}"#
    );

    let authorization = DeviceCodeAuthorization::from_json(&body).unwrap();

    assert_eq!(authorization.user_code, USER_CODE);
    assert_eq!(
        authorization.verification_uri,
        "https://microsoft.com/devicelogin"
    );
    assert_eq!(authorization.device_code, DeviceCode::new(DEVICE_CODE));
    assert_eq!(authorization.expires_in, Duration::from_secs(900));
    assert_eq!(authorization.interval, Duration::from_secs(5));
}

#[test]
fn a_missing_interval_defaults_to_five_seconds() {
    let body = format!(
        r#"{{"user_code":"{USER_CODE}","verification_uri":"https://microsoft.com/devicelogin","device_code":"{DEVICE_CODE}","expires_in":900}}"#
    );

    let authorization = DeviceCodeAuthorization::from_json(&body).unwrap();

    assert_eq!(authorization.interval, Duration::from_secs(5));
}

#[test]
fn a_zero_interval_is_clamped_to_one_second() {
    let body = format!(
        r#"{{"user_code":"{USER_CODE}","verification_uri":"https://microsoft.com/devicelogin","device_code":"{DEVICE_CODE}","expires_in":900,"interval":0}}"#
    );

    let authorization = DeviceCodeAuthorization::from_json(&body).unwrap();

    assert_eq!(authorization.interval, Duration::from_secs(1));

    let poll = DeviceCodePoll::from_challenge(&authorization);
    assert_eq!(poll.interval(), Duration::from_secs(1));
}

#[test]
fn an_authorization_response_missing_a_field_is_malformed() {
    for body in [
        format!(
            r#"{{"verification_uri":"https://x.invalid","device_code":"{DEVICE_CODE}","expires_in":900}}"#
        ),
        format!(r#"{{"user_code":"{USER_CODE}","device_code":"{DEVICE_CODE}","expires_in":900}}"#),
        format!(
            r#"{{"user_code":"{USER_CODE}","verification_uri":"https://x.invalid","expires_in":900}}"#
        ),
        format!(
            r#"{{"user_code":"{USER_CODE}","verification_uri":"https://x.invalid","device_code":"{DEVICE_CODE}"}}"#
        ),
        String::from(r#"{"user_code":"ABCD1234"}"#),
        String::from("not json at all"),
        String::new(),
    ] {
        let error = DeviceCodeAuthorization::from_json(&body).unwrap_err();

        assert!(
            matches!(error, DeviceCodeError::MalformedResponse),
            "unexpected error for {body}: {error:?}"
        );
    }
}

#[test]
fn an_authorization_response_with_empty_credentials_is_malformed() {
    for body in [
        r#"{"user_code":"","verification_uri":"https://x.invalid","device_code":"abcdefghijklmnop","expires_in":900}"#.to_owned(),
        r#"{"user_code":"ABCD1234","verification_uri":"https://x.invalid","device_code":"","expires_in":900}"#.to_owned(),
        r#"{"user_code":"ABCD1234","verification_uri":"https://x.invalid","device_code":"abcdefghijklmnop","expires_in":0}"#.to_owned(),
    ] {
        assert!(matches!(
            DeviceCodeAuthorization::from_json(&body),
            Err(DeviceCodeError::MalformedResponse)
        ));
    }
}

#[test]
fn a_malformed_authorization_error_does_not_echo_the_body() {
    let body = format!(
        r#"{{"user_code":"{USER_CODE}","device_code":"{DEVICE_CODE}","expires_in":900,"verification_uri":42}}"#
    );

    let error = DeviceCodeAuthorization::from_json(&body).unwrap_err();
    let rendered = error.to_string();

    assert_eq!(
        rendered,
        "the device code response is missing required fields"
    );
    assert!(!rendered.contains(DEVICE_CODE));
    assert!(!rendered.contains(USER_CODE));
}

#[test]
fn the_device_code_redacts_itself() {
    let code = DeviceCode::new(DEVICE_CODE);

    assert_eq!(format!("{code:?}"), "DeviceCode(****abcd)");
    assert_eq!(format!("{code}"), "DeviceCode(****abcd)");
    assert!(!format!("{code:?}").contains("device-code-value"));
    assert!(!format!("{code:?}").contains("1234567890"));
}

#[test]
fn a_short_device_code_is_fully_hidden() {
    let code = DeviceCode::new("short");

    assert_eq!(format!("{code:?}"), "DeviceCode(****)");
    assert!(!format!("{code:?}").contains("short"));
}

#[test]
fn the_debug_of_a_poll_request_does_not_leak_the_device_code() {
    let request = TokenPollRequest::new(CLIENT_ID, DeviceCode::new(DEVICE_CODE));

    let rendered = format!("{request:?}");

    assert!(rendered.contains("DeviceCode(****abcd)"));
    assert!(rendered.contains(CLIENT_ID));
    assert!(!rendered.contains(DEVICE_CODE));
}

#[test]
fn the_device_code_accessors_round_trip() {
    let code = DeviceCode::new(DEVICE_CODE);

    assert_eq!(code.expose(), DEVICE_CODE);
    assert!(!code.is_empty());
    assert!(DeviceCode::new("").is_empty());
}

// --------------------------------------------------------------------------------------
// Token response parsing
// --------------------------------------------------------------------------------------

#[test]
fn an_authorized_response_yields_tokens_and_a_lifetime() {
    let body = format!(
        r#"{{"access_token":"{ACCESS_TOKEN}","refresh_token":"{REFRESH_TOKEN}","expires_in":3600,"scope":"XboxLive.signin offline_access","token_type":"Bearer"}}"#
    );

    let outcome = parse_token_response(200, &body, epoch()).unwrap();

    let tokens = tokens_of(&outcome);
    assert_eq!(tokens.access.expose(), ACCESS_TOKEN);
    assert_eq!(
        tokens.refresh.as_ref().map(|refresh| refresh.expose()),
        Some(REFRESH_TOKEN)
    );
    assert_eq!(tokens.lifetime.issued_at(), epoch());
    assert_eq!(tokens.lifetime.expires_in(), Duration::from_secs(3600));
    assert_eq!(
        tokens.lifetime.expires_at(),
        epoch() + Duration::from_secs(3600)
    );
}

#[test]
fn an_authorized_response_may_omit_the_refresh_token() {
    let body = format!(r#"{{"access_token":"{ACCESS_TOKEN}","expires_in":1800}}"#);

    let outcome = parse_token_response(200, &body, epoch()).unwrap();

    let tokens = tokens_of(&outcome);
    assert!(tokens.refresh.is_none());
    assert_eq!(tokens.lifetime.expires_in(), Duration::from_secs(1800));
}

#[test]
fn an_empty_refresh_token_is_treated_as_absent() {
    let body =
        format!(r#"{{"access_token":"{ACCESS_TOKEN}","refresh_token":"","expires_in":1800}}"#);

    let tokens = parse_token_response(200, &body, epoch())
        .map(|outcome| tokens_of(&outcome).refresh.clone())
        .unwrap();

    assert!(tokens.is_none());
}

#[test]
fn the_four_rfc_8628_errors_map_to_outcomes() {
    let cases = [
        ("authorization_pending", PollOutcome::Pending),
        ("slow_down", PollOutcome::SlowDown),
        ("expired_token", PollOutcome::Expired),
        ("authorization_declined", PollOutcome::Denied),
    ];

    for (error, expected) in cases {
        let body = format!(r#"{{"error":"{error}"}}"#);

        let outcome = parse_token_response(400, &body, epoch()).unwrap();

        assert_eq!(outcome, expected, "error code {error}");
    }
}

#[test]
fn any_4xx_carries_the_rfc_8628_codes() {
    // Microsoft sometimes answers 401 for an expired device code; the mapping does not
    // depend on which 4xx it is.
    for status in [400_u16, 401] {
        let outcome = parse_token_response(
            status,
            r#"{"error":"expired_token","error_description":"long text"}"#,
            epoch(),
        )
        .unwrap();

        assert_eq!(outcome, PollOutcome::Expired);
    }
}

#[test]
fn an_unknown_oauth_error_becomes_an_oauth_error() {
    let error = parse_token_response(
        401,
        r#"{"error":"invalid_grant","error_description":"AADSTS70000: something"}"#,
        epoch(),
    )
    .unwrap_err();

    assert!(matches!(error, DeviceCodeError::OAuth { ref code } if code == "invalid_grant"));
    assert_eq!(
        error.to_string(),
        "the OAuth endpoint rejected the request with invalid_grant"
    );
}

#[test]
fn a_4xx_without_an_error_code_is_malformed() {
    let error = parse_token_response(400, r#"{"error_description":"nope"}"#, epoch()).unwrap_err();

    assert!(matches!(error, DeviceCodeError::MalformedResponse));
}

#[test]
fn an_error_response_never_leaks_tokens_or_the_body() {
    // A hostile or buggy endpoint echoing the request back must not turn into a log line
    // containing credentials.
    let body = format!(
        r#"{{"error":"invalid_request","error_description":"device_code={DEVICE_CODE}&refresh_token={REFRESH_TOKEN}&access_token={ACCESS_TOKEN}"}}"#
    );

    let error = parse_token_response(400, &body, epoch()).unwrap_err();
    let rendered = error.to_string();
    let debugged = format!("{error:?}");

    for secret in [DEVICE_CODE, REFRESH_TOKEN, ACCESS_TOKEN] {
        assert!(!rendered.contains(secret), "message leaked {secret}");
        assert!(!debugged.contains(secret), "debug leaked {secret}");
    }
    assert!(!rendered.contains("error_description"));
    assert!(!debugged.contains("error_description"));
}

#[test]
fn a_malformed_success_body_does_not_leak_the_body() {
    for body in [
        format!(r#"{{"access_token":"{ACCESS_TOKEN}"}}"#),
        r#"{"expires_in":3600}"#.to_owned(),
        r#"{"access_token":"","expires_in":3600}"#.to_owned(),
        format!(r#"{{"access_token":"{ACCESS_TOKEN}","expires_in":0}}"#),
        format!(r#"access_token={ACCESS_TOKEN}&expires_in=3600"#),
        String::new(),
    ] {
        let error = parse_token_response(200, &body, epoch()).unwrap_err();

        assert!(matches!(error, DeviceCodeError::MalformedResponse));
        assert_eq!(
            error.to_string(),
            "the device code response is missing required fields"
        );
        assert!(!error.to_string().contains(ACCESS_TOKEN));
    }
}

#[test]
fn non_2xx_non_4xx_statuses_are_unexpected() {
    for status in [0_u16, 100, 301, 404, 500, 503] {
        let error = parse_token_response(status, "{}", epoch()).unwrap_err();

        match error {
            DeviceCodeError::UnexpectedStatus { status: reported } => assert_eq!(reported, status),
            other => panic!("expected UnexpectedStatus for {status}, got {other:?}"),
        }
    }
}

#[test]
fn token_outcome_debug_does_not_expose_the_tokens() {
    let outcome = authorized_outcome(3600);

    let rendered = format!("{outcome:?}");

    assert!(rendered.contains("AccessToken(****9876)"));
    assert!(rendered.contains("RefreshToken(****4321)"));
    assert!(!rendered.contains(ACCESS_TOKEN));
    assert!(!rendered.contains("M.R3_BAY"));
}

// --------------------------------------------------------------------------------------
// The polling state machine
// --------------------------------------------------------------------------------------

#[test]
fn a_pending_poll_waits_for_the_current_interval() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 900));

    assert_eq!(poll.interval(), Duration::from_secs(5));
    assert_eq!(poll.elapsed(), Duration::ZERO);

    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(5))
    );
    assert_eq!(poll.elapsed(), Duration::from_secs(5));

    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(5))
    );
    assert_eq!(poll.elapsed(), Duration::from_secs(10));
}

#[test]
fn repeated_pending_polls_eventually_expire() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 12));

    // 5s, 10s: still within the lifetime.
    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(5))
    );
    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(5))
    );
    // 15s >= 12s: the next poll would be past the deadline.
    assert_eq!(poll.advance(PollOutcome::Pending), PollAction::Expired);

    // The decision sticks; nothing later revives the attempt.
    assert_eq!(poll.advance(PollOutcome::Pending), PollAction::Expired);
}

#[test]
fn the_elapsed_time_may_reach_the_lifetime_exactly() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 10));

    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(5))
    );
    assert_eq!(poll.advance(PollOutcome::Pending), PollAction::Expired);
}

#[test]
fn slow_down_adds_five_seconds_and_returns_the_new_interval() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 900));

    assert_eq!(
        poll.advance(PollOutcome::SlowDown),
        PollAction::Wait(Duration::from_secs(10))
    );
    assert_eq!(poll.interval(), Duration::from_secs(10));
    // The penalty interval, not the old one, is what got charged.
    assert_eq!(poll.elapsed(), Duration::from_secs(10));

    assert_eq!(
        poll.advance(PollOutcome::SlowDown),
        PollAction::Wait(Duration::from_secs(15))
    );
    assert_eq!(poll.interval(), Duration::from_secs(15));
    assert_eq!(poll.elapsed(), Duration::from_secs(25));

    // A pending poll afterwards keeps the grown interval.
    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(15))
    );
    assert_eq!(poll.interval(), Duration::from_secs(15));
}

#[test]
fn slow_down_can_run_the_attempt_into_the_deadline() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 20));

    // 10s, then 15s more = 25s >= 20s.
    assert_eq!(
        poll.advance(PollOutcome::SlowDown),
        PollAction::Wait(Duration::from_secs(10))
    );
    assert_eq!(poll.advance(PollOutcome::SlowDown), PollAction::Expired);
}

#[test]
fn an_authorized_poll_finishes_the_flow() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 900));
    let outcome = authorized_outcome(3600);

    let action = poll.advance(outcome.clone());

    match action {
        PollAction::Done(tokens) => assert_eq!(PollOutcome::Authorized(tokens), outcome),
        other => panic!("expected Done, got {other:?}"),
    }
    assert_eq!(poll.elapsed(), Duration::from_secs(5));
}

#[test]
fn an_authorized_poll_wins_even_after_the_deadline() {
    // The service answered with tokens, so the result stands even though the local clock
    // says the attempt is overdue.
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 3));

    assert!(matches!(
        poll.advance(authorized_outcome(3600)),
        PollAction::Done(_)
    ));
}

#[test]
fn an_authorized_poll_after_an_expiry_is_still_honoured() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 5));

    assert_eq!(poll.advance(PollOutcome::Pending), PollAction::Expired);

    assert!(matches!(
        poll.advance(authorized_outcome(3600)),
        PollAction::Done(_)
    ));
}

#[test]
fn a_declined_attempt_ends_the_flow() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 900));

    assert_eq!(poll.advance(PollOutcome::Denied), PollAction::Expired);
}

#[test]
fn an_expired_outcome_ends_the_flow() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 900));

    assert_eq!(poll.advance(PollOutcome::Expired), PollAction::Expired);
}

#[test]
fn the_state_machine_never_polls_faster_than_one_second() {
    let degraded = DeviceCodeAuthorization {
        interval: Duration::ZERO,
        ..challenge(5, 900)
    };

    let mut poll = DeviceCodePoll::from_challenge(&degraded);

    assert_eq!(
        poll.advance(PollOutcome::Pending),
        PollAction::Wait(Duration::from_secs(1))
    );
}

#[test]
fn a_full_polling_loop_reaches_the_tokens() {
    let mut poll = DeviceCodePoll::from_challenge(&challenge(5, 900));
    let scripted = [
        PollOutcome::Pending,
        PollOutcome::SlowDown,
        PollOutcome::Pending,
        authorized_outcome(3600),
    ];

    let mut waits = Vec::new();
    let mut tokens = None;
    for outcome in scripted {
        match poll.advance(outcome) {
            PollAction::Wait(delay) => waits.push(delay),
            PollAction::Done(result) => {
                tokens = Some(result);
                break;
            }
            PollAction::Expired => panic!("the fixture must not expire"),
        }
    }

    assert_eq!(
        waits,
        vec![
            Duration::from_secs(5),
            Duration::from_secs(10),
            Duration::from_secs(10)
        ]
    );
    let tokens = tokens.expect("the loop ended with tokens");
    assert_eq!(tokens.access.expose(), ACCESS_TOKEN);
    // 5 + 10 + 10 + 10.
    assert_eq!(poll.elapsed(), Duration::from_secs(35));
}
