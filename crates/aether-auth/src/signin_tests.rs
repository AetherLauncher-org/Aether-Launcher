use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use super::{SignInError, refresh_microsoft_tokens, sign_in_to_minecraft};
use crate::device_code::{DeviceCodeEndpoints, DeviceCodeError, TOKEN_URL};
use crate::http::{HttpError, HttpRequest, HttpResponse, HttpTransport};
use crate::minecraft::{ENTITLEMENTS_URL, LOGIN_WITH_XBOX_URL, MinecraftError, PROFILE_URL};
use crate::token::{AccessToken, MicrosoftTokens, RefreshToken, TokenLifetime};
use crate::xbox::{XBL_AUTHENTICATE_URL, XSTS_AUTHORIZE_URL, XboxError};

const MS_ACCESS: &str = "ms-access-token-0123456789";
const MC_ACCESS: &str = "mc-access-token-abcdefghij";

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn tokens() -> MicrosoftTokens {
    MicrosoftTokens::new(
        AccessToken::new(MS_ACCESS),
        Some(RefreshToken::new("refresh-token-9876543210")),
        TokenLifetime::new(now(), Duration::from_secs(3600)),
    )
}

/// Answers by URL and remembers every request, in order.
struct Fake {
    answers: HashMap<&'static str, (u16, String)>,
    log: RefCell<Vec<String>>,
}

impl Fake {
    fn happy() -> Self {
        let mut answers = HashMap::new();
        answers.insert(
            XBL_AUTHENTICATE_URL,
            (
                200,
                r#"{"Token":"xbl-token","DisplayClaims":{"xui":[{"uhs":"hash123"}]}}"#.to_owned(),
            ),
        );
        answers.insert(
            XSTS_AUTHORIZE_URL,
            (
                200,
                r#"{"Token":"xsts-token","DisplayClaims":{"xui":[{"uhs":"hash123","xid":"2535400000000001"}]}}"#
                    .to_owned(),
            ),
        );
        answers.insert(
            LOGIN_WITH_XBOX_URL,
            (
                200,
                format!(r#"{{"access_token":"{MC_ACCESS}","expires_in":86400,"username":"u"}}"#),
            ),
        );
        answers.insert(
            ENTITLEMENTS_URL,
            (
                200,
                r#"{"items":[{"name":"product_minecraft","signature":"s"}]}"#.to_owned(),
            ),
        );
        answers.insert(
            PROFILE_URL,
            (
                200,
                r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#.to_owned(),
            ),
        );
        Self {
            answers,
            log: RefCell::new(Vec::new()),
        }
    }

    fn with(mut self, url: &'static str, status: u16, body: &str) -> Self {
        self.answers.insert(url, (status, body.to_owned()));
        self
    }

    fn urls(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

impl HttpTransport for Fake {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.log.borrow_mut().push(request.url.clone());
        match self.answers.get(request.url.as_str()) {
            Some((status, body)) => Ok(HttpResponse::new(*status, body.clone())),
            None => Err(HttpError::transport(&request.url)),
        }
    }
}

#[test]
fn the_chain_runs_in_order_and_yields_the_profile() {
    let fake = Fake::happy();

    let login = sign_in_to_minecraft(&fake, &tokens(), now()).expect("sign-in");

    assert_eq!(
        fake.urls(),
        [
            XBL_AUTHENTICATE_URL,
            XSTS_AUTHORIZE_URL,
            LOGIN_WITH_XBOX_URL,
            ENTITLEMENTS_URL,
            PROFILE_URL,
        ]
    );
    assert_eq!(login.profile.name, "Notch");
    assert_eq!(
        login.profile.undashed_id(),
        "069a79f444e94726a5befca90e38aaf5"
    );
    assert_eq!(login.xuid, "2535400000000001");
    assert_eq!(login.session.access.expose(), MC_ACCESS);
}

#[test]
fn an_account_without_the_game_stops_before_the_profile() {
    let fake = Fake::happy().with(ENTITLEMENTS_URL, 200, r#"{"items":[]}"#);

    let error = sign_in_to_minecraft(&fake, &tokens(), now()).expect_err("not entitled");

    assert!(matches!(
        error,
        SignInError::Minecraft(MinecraftError::NotEntitled)
    ));
    assert!(!fake.urls().contains(&PROFILE_URL.to_owned()));
}

#[test]
fn an_account_without_a_player_name_is_reported_as_such() {
    let fake = Fake::happy().with(PROFILE_URL, 404, "{}");

    let error = sign_in_to_minecraft(&fake, &tokens(), now()).expect_err("no profile");

    assert!(matches!(
        error,
        SignInError::Minecraft(MinecraftError::ProfileNotFound)
    ));
}

#[test]
fn an_xbox_refusal_stops_the_chain_with_the_specific_reason() {
    let fake = Fake::happy().with(XSTS_AUTHORIZE_URL, 401, r#"{"XErr":2148916238}"#);

    let error = sign_in_to_minecraft(&fake, &tokens(), now()).expect_err("child account");

    assert!(matches!(error, SignInError::Xbox(XboxError::ChildAccount)));
    assert_eq!(fake.urls().len(), 2, "nothing is sent after the refusal");
}

#[test]
fn a_network_failure_is_reported_without_the_request_body() {
    let mut fake = Fake::happy();
    fake.answers.remove(XBL_AUTHENTICATE_URL);

    let error = sign_in_to_minecraft(&fake, &tokens(), now()).expect_err("offline");

    assert!(matches!(error, SignInError::Http(_)));
    let message = error.to_string();
    assert!(!message.contains(MS_ACCESS));
    assert!(message.contains("user.auth.xboxlive.com"));
}

#[test]
fn errors_never_contain_tokens() {
    let fake = Fake::happy().with(LOGIN_WITH_XBOX_URL, 403, "forbidden");

    let error = sign_in_to_minecraft(&fake, &tokens(), now()).expect_err("rejected");

    let text = format!("{error} {error:?}");
    for secret in [MS_ACCESS, "xbl-token", "xsts-token", "hash123"] {
        assert!(!text.contains(secret), "leaked {secret}");
    }
}

#[test]
fn refresh_returns_the_rotated_tokens() {
    let fake = Fake::happy().with(
        TOKEN_URL,
        200,
        r#"{"access_token":"new-access-token-1111","refresh_token":"new-refresh-token-2222","expires_in":3600}"#,
    );
    let old = RefreshToken::new("refresh-token-9876543210");

    let fresh = refresh_microsoft_tokens(
        &fake,
        &DeviceCodeEndpoints::microsoft(),
        "client-id",
        &old,
        now(),
    )
    .expect("refreshed");

    assert_eq!(fresh.access.expose(), "new-access-token-1111");
    assert_eq!(
        fresh.refresh.as_ref().map(RefreshToken::expose),
        Some("new-refresh-token-2222")
    );
}

#[test]
fn a_revoked_refresh_token_requires_a_new_sign_in_but_an_outage_does_not() {
    let fake = Fake::happy().with(TOKEN_URL, 400, r#"{"error":"invalid_grant"}"#);
    let old = RefreshToken::new("refresh-token-9876543210");

    let revoked = refresh_microsoft_tokens(
        &fake,
        &DeviceCodeEndpoints::microsoft(),
        "client-id",
        &old,
        now(),
    )
    .expect_err("revoked");
    assert!(revoked.requires_sign_in());

    let outage = SignInError::OAuth(DeviceCodeError::UnexpectedStatus { status: 503 });
    assert!(!outage.requires_sign_in());
    assert!(!SignInError::Http(HttpError::transport("https://x.example/")).requires_sign_in());
    assert!(SignInError::Minecraft(MinecraftError::Unauthorized).requires_sign_in());
}
