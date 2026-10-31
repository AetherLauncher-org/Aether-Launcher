use std::cell::RefCell;

use aether_auth::http::{HttpError, HttpRequest, HttpResponse};

use super::*;

const TOKEN: &str = "mc-access-token-abcdefghij";

struct Answer {
    result: Result<HttpResponse, HttpError>,
    seen: RefCell<Vec<HttpRequest>>,
}

impl Answer {
    fn status(status: u16) -> Self {
        Self {
            result: Ok(HttpResponse::new(status, "")),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl HttpTransport for Answer {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.seen.borrow_mut().push(request.clone());
        self.result.clone()
    }
}

fn run(answer: &Answer) -> Result<(), String> {
    upload(answer, &MinecraftToken::new(TOKEN), true, b"png-bytes")
}

#[test]
fn a_skin_is_uploaded_once_with_the_token_in_the_header_only() {
    let answer = Answer::status(200);
    assert_eq!(run(&answer), Ok(()));
    let seen = answer.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].url, aether_auth::minecraft::SKINS_URL);
    assert_eq!(
        seen[0].header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert!(!seen[0].url.contains(TOKEN));
}

#[test]
fn refusals_get_plain_messages_without_the_token() {
    for (status, needle) in [
        (401, "sign in"),
        (403, "sign in"),
        (400, "did not accept"),
        (429, "wait"),
        (500, "try again later"),
    ] {
        let message = run(&Answer::status(status)).unwrap_err();
        assert!(message.contains(needle), "{status}: {message}");
        assert!(!message.contains(TOKEN));
    }
}

#[test]
fn a_dead_network_is_reported_without_the_host_details() {
    let answer = Answer {
        result: Err(HttpError::transport("https://api.minecraftservices.com/x")),
        seen: RefCell::new(Vec::new()),
    };
    let message = run(&answer).unwrap_err();
    assert!(message.contains("network"));
    assert!(!message.contains("minecraftservices"));
}
