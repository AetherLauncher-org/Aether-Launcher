use std::time::{Duration, SystemTime};

use super::*;
use crate::device_code::DeviceCodeError;

const STATE: &str = "state-of-this-attempt";

#[test]
fn the_challenge_matches_the_rfc_7636_example() {
    let pair = PkceChallenge::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    assert_eq!(
        pair.challenge(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn generated_pairs_are_long_unique_and_url_safe() {
    let first = PkceChallenge::generate();
    let second = PkceChallenge::generate();
    assert_ne!(first.verifier(), second.verifier());
    assert_ne!(random_state(), random_state());
    let verifier = first.verifier().expose();
    assert!((43..=128).contains(&verifier.len()), "{}", verifier.len());
    assert!(verifier.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    // SHA-256 is 32 bytes, which is 43 characters of unpadded base64url.
    assert_eq!(first.challenge().len(), 43);
    assert!(
        first
            .challenge()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    );
}

#[test]
fn the_secrets_redact_themselves() {
    let pair = PkceChallenge::from_verifier("a-very-secret-verifier-value-1234567890abcdef");
    let shown = format!("{pair:?} {} {:?}", pair.verifier(), pair.verifier());
    assert!(!shown.contains("secret-verifier"), "{shown}");
    let code = parse_callback_query(&format!("code=abcdef123456&state={STATE}"), STATE).unwrap();
    let shown = format!("{code:?} {code}");
    assert!(!shown.contains("abcdef"), "{shown}");
}

#[test]
fn the_authorization_url_carries_pkce_state_and_the_scope() {
    let pair = PkceChallenge::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    let url = AuthorizationRequest::new("client-id", "http://localhost:5123", &pair, STATE)
        .url(&AuthCodeEndpoints::microsoft());
    assert!(url.starts_with(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id=client-id&"
    ));
    for part in [
        "response_type=code",
        "redirect_uri=http%3A%2F%2Flocalhost%3A5123",
        "response_mode=query",
        "scope=XboxLive.signin+offline_access",
        &format!("state={STATE}"),
        "code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        "code_challenge_method=S256",
        "prompt=select_account",
    ] {
        assert!(url.contains(part), "{part} missing from {url}");
    }
    // The verifier never appears in the URL.
    assert!(!url.contains("dBjftJeZ4CVP"));
}

#[test]
fn a_redirect_with_the_right_state_yields_the_code() {
    let code = parse_callback_query(&format!("code=M.C5_abc%2Bdef&state={STATE}"), STATE).unwrap();
    // Percent escapes are decoded; the code is used verbatim afterwards.
    assert_eq!(code.expose(), "M.C5_abc+def");
}

#[test]
fn a_redirect_from_another_attempt_is_refused_before_anything_else() {
    for query in [
        "code=abc&state=someone-elses".to_owned(),
        "code=abc".to_owned(),
        format!("error=access_denied&state=wrong-{STATE}"),
        String::new(),
    ] {
        assert_eq!(
            parse_callback_query(&query, STATE),
            Err(CallbackError::StateMismatch),
            "{query}"
        );
    }
}

#[test]
fn declined_and_failed_sign_ins_are_told_apart() {
    assert_eq!(
        parse_callback_query(&format!("error=access_denied&state={STATE}"), STATE),
        Err(CallbackError::Denied)
    );
    assert_eq!(
        parse_callback_query(
            &format!("error=invalid_request&error_description=nope&state={STATE}"),
            STATE
        ),
        Err(CallbackError::OAuth {
            code: "invalid_request".to_owned()
        })
    );
    // The code in a message is plain text, whatever the query contained.
    assert_eq!(
        parse_callback_query(&format!("error=%3Cscript%3E&state={STATE}"), STATE),
        Err(CallbackError::OAuth {
            code: "invalid_error".to_owned()
        })
    );
}

#[test]
fn a_redirect_without_a_usable_code_is_refused() {
    for query in [
        format!("state={STATE}"),
        format!("code=&state={STATE}"),
        format!("code={}&state={STATE}", "x".repeat(MAX_CODE_LEN + 1)),
    ] {
        assert_eq!(
            parse_callback_query(&query, STATE),
            Err(CallbackError::MissingCode)
        );
    }
    assert_eq!(
        parse_callback_query(&format!("code=%zz&state={STATE}"), STATE),
        Err(CallbackError::Malformed)
    );
    assert_eq!(
        parse_callback_query(&format!("code=%ff%fe&state={STATE}"), STATE),
        Err(CallbackError::Malformed)
    );
    assert_eq!(
        parse_callback_query(&"a=b&".repeat(MAX_REQUEST_HEAD_BYTES), STATE),
        Err(CallbackError::Malformed)
    );
}

#[test]
fn errors_never_contain_the_code() {
    let code = "SECRETCODE123";
    for query in [
        format!("code={code}&state=wrong"),
        format!("code={code}%zz&state={STATE}"),
    ] {
        let error = parse_callback_query(&query, STATE).unwrap_err();
        assert!(!error.to_string().contains(code));
        assert!(!format!("{error:?}").contains(code));
    }
}

fn request(target: &str, host: &str) -> String {
    format!("GET {target} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: test\r\n\r\n")
}

#[test]
fn only_the_redirect_request_reaches_the_parser() {
    let query = "code=abc&state=xyz";
    for host in [
        "localhost:5123",
        "127.0.0.1:5123",
        "[::1]:5123",
        "LOCALHOST:5123",
    ] {
        assert_eq!(
            callback_query(&request(&format!("/?{query}"), host), 5123).as_deref(),
            Some(query),
            "{host}"
        );
    }
}

#[test]
fn everything_else_is_ignored() {
    let good = "/?code=abc&state=xyz";
    // A foreign Host header is what a DNS-rebinding page sends.
    for host in [
        "evil.example:5123",
        "localhost:5124",
        "localhost",
        "localhost.evil.example:5123",
    ] {
        assert_eq!(callback_query(&request(good, host), 5123), None, "{host}");
    }
    for target in ["/favicon.ico", "/other?code=a", "/", "/?", "*"] {
        assert_eq!(
            callback_query(&request(target, "localhost:5123"), 5123),
            None,
            "{target}"
        );
    }
    let post = "POST /?code=a HTTP/1.1\r\nHost: localhost:5123\r\n\r\n";
    assert_eq!(callback_query(post, 5123), None);
    let old = "GET /?code=a HTTP/2\r\nHost: localhost:5123\r\n\r\n";
    assert_eq!(callback_query(old, 5123), None);
    let no_host = "GET /?code=a HTTP/1.1\r\nAccept: */*\r\n\r\n";
    assert_eq!(callback_query(no_host, 5123), None);
    let extra = "GET /?code=a HTTP/1.1 extra\r\nHost: localhost:5123\r\n\r\n";
    assert_eq!(callback_query(extra, 5123), None);
    let huge = format!(
        "GET /?code={} HTTP/1.1\r\nHost: localhost:5123\r\n\r\n",
        "a".repeat(10_000)
    );
    assert_eq!(callback_query(&huge, 5123), None);
    assert_eq!(callback_query("", 5123), None);
}

#[test]
fn the_token_request_repeats_the_verifier_and_the_redirect() {
    let pair = PkceChallenge::from_verifier("verifier-0123456789-0123456789-0123456789-abc");
    let code = parse_callback_query(&format!("code=M.C5%2Babc&state={STATE}"), STATE).unwrap();
    let request =
        CodeExchangeRequest::new("client-id", &code, "http://localhost:5123", pair.verifier())
            .to_http(&AuthCodeEndpoints::microsoft());
    assert_eq!(
        request.url,
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
    );
    let body = request.body().unwrap();
    for part in [
        "grant_type=authorization_code",
        "client_id=client-id",
        "code=M.C5%2Babc",
        "redirect_uri=http%3A%2F%2Flocalhost%3A5123",
        "code_verifier=verifier-0123456789-0123456789-0123456789-abc",
        "scope=XboxLive.signin+offline_access",
    ] {
        assert!(body.contains(part), "{part} missing from {body}");
    }
}

#[test]
fn the_exchange_answer_is_tokens_or_an_oauth_error() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let tokens = parse_exchange_response(
        200,
        r#"{"access_token":"at-123456789012","refresh_token":"rt-123456789012","expires_in":3600}"#,
        now,
    )
    .unwrap();
    assert!(tokens.refresh.is_some());

    let error = parse_exchange_response(400, r#"{"error":"invalid_grant"}"#, now).unwrap_err();
    assert!(matches!(error, DeviceCodeError::OAuth { ref code } if code == "invalid_grant"));
}
