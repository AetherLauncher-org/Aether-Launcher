use std::io::{self, Cursor};
use std::sync::Mutex;

use super::*;

const LIB_URL: &str = "https://example.invalid/lib.jar";

/// 返回固定 200 响应的内存替身，并记录最近一次请求的 URL。
struct FakeTransport {
    last_url: Mutex<Option<String>>,
}

impl HttpTransport for FakeTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        *self.last_url.lock().expect("mutex poisoned") = Some(request.url.clone());
        Ok(TransportResponse {
            status: 200,
            content_length: Some(5),
            supports_range: false,
            content_range: None,
            body: Box::new(Cursor::new(b"hello".to_vec())),
        })
    }
}

fn request() -> TransportRequest {
    TransportRequest {
        url: LIB_URL.to_owned(),
        range: Some(Range {
            start: 10,
            end: Some(99),
        }),
        timeout_secs: 30,
    }
}

#[test]
fn range_to_header_value_formats_open_ended_and_bounded() {
    assert_eq!(
        Range {
            start: 5,
            end: None
        }
        .to_header_value(),
        "bytes=5-",
        "open-ended range should render without an end offset"
    );
    assert_eq!(
        Range {
            start: 0,
            end: Some(99)
        }
        .to_header_value(),
        "bytes=0-99"
    );
    assert_eq!(
        Range {
            start: 10,
            end: Some(10)
        }
        .to_header_value(),
        "bytes=10-10",
        "single-byte range keeps start equal to end"
    );
}

#[test]
fn fake_transport_returns_canned_response_and_sees_request() {
    let transport = FakeTransport {
        last_url: Mutex::new(None),
    };
    // 通过 trait 对象调用，确认接口可对象安全地使用。
    let getter: &dyn HttpTransport = &transport;
    let mut response = getter.get(&request()).expect("fake transport succeeds");
    assert_eq!(response.status, 200);
    assert_eq!(response.content_length, Some(5));
    assert!(!response.supports_range);
    let mut body = String::new();
    response
        .body
        .read_to_string(&mut body)
        .expect("body is readable");
    assert_eq!(body, "hello");
    assert_eq!(
        *transport.last_url.lock().expect("mutex poisoned"),
        Some(LIB_URL.to_owned())
    );
}

#[test]
fn http_status_error_message_contains_status_and_url_only() {
    let error = TransportError::HttpStatus {
        url: LIB_URL.to_owned(),
        status: 404,
    };
    let message = error.to_string();
    assert!(message.contains("404"), "message was {message}");
    assert!(message.contains(LIB_URL), "message was {message}");
    assert!(!message.contains(":\\"), "message was {message}");
    assert!(
        !message.contains("G:\\") && !message.contains("C:\\"),
        "message must not contain an absolute local path: {message}"
    );
}

#[test]
fn connect_error_display_redacts_credentials_and_query() {
    let error = TransportError::Connect {
        url: "https://user:pass@example.invalid/f?token=abc".to_owned(),
        source: io::Error::other("boom"),
    };
    let message = error.to_string();
    assert!(message.contains("example.invalid"), "message was {message}");
    assert!(!message.contains("pass"), "message was {message}");
    assert!(!message.contains("abc"), "message was {message}");
}

#[test]
fn connect_error_display_includes_source_message() {
    let error = TransportError::Connect {
        url: LIB_URL.to_owned(),
        source: io::Error::other("boom"),
    };
    let message = error.to_string();
    assert!(message.contains(LIB_URL), "message was {message}");
    assert!(message.contains("boom"), "message was {message}");
}
