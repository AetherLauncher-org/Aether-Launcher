use std::io::{self, Cursor, Read};
use std::sync::{Arc, Mutex};

use super::*;

const LIB_URL: &str = "https://example.invalid/lib.jar";

/// `TransportResponse` 含 `dyn Read`，未实现 `Debug`，不能直接 `expect_err`。
fn expect_error(
    result: Result<TransportResponse, TransportError>,
    context: &str,
) -> TransportError {
    match result {
        Ok(_) => panic!("{context}"),
        Err(error) => error,
    }
}

/// 用闭包充当内核；同时返回共享的请求记录器，便于断言内核收到了什么。
fn fn_kernel(
    handler: impl Fn(&KernelRequest) -> Result<KernelResponse, KernelError> + Send + Sync + 'static,
) -> (FnKernel, CallLog) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let log = CallLog {
        calls: calls.clone(),
    };
    (
        FnKernel {
            calls,
            handler: Box::new(handler),
        },
        log,
    )
}

/// 对内核收到过的请求序列的共享视图。
#[derive(Clone)]
struct CallLog {
    calls: Arc<Mutex<Vec<KernelRequest>>>,
}

impl CallLog {
    fn urls(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("mutex poisoned")
            .iter()
            .map(|request| request.url.clone())
            .collect()
    }

    fn last(&self) -> KernelRequest {
        self.calls
            .lock()
            .expect("mutex poisoned")
            .last()
            .map(|request| KernelRequest {
                url: request.url.clone(),
                range_header: request.range_header.clone(),
                timeout_secs: request.timeout_secs,
            })
            .expect("kernel was never called")
    }
}

/// 内核替身的处理函数类型。
type KernelFn = Box<dyn Fn(&KernelRequest) -> Result<KernelResponse, KernelError> + Send + Sync>;

/// 用闭包充当内核的测试替身，记录每次收到的请求。
struct FnKernel {
    calls: Arc<Mutex<Vec<KernelRequest>>>,
    handler: KernelFn,
}

impl HttpKernel for FnKernel {
    fn get(&self, request: &KernelRequest) -> Result<KernelResponse, KernelError> {
        self.calls
            .lock()
            .expect("mutex poisoned")
            .push(KernelRequest {
                url: request.url.clone(),
                range_header: request.range_header.clone(),
                timeout_secs: request.timeout_secs,
            });
        (self.handler)(request)
    }
}

fn ok_response(body: &[u8]) -> KernelResponse {
    KernelResponse {
        status: 200,
        content_length: Some(body.len() as u64),
        headers: Vec::new(),
        body: Box::new(Cursor::new(body.to_vec())),
    }
}

fn redirect_response(status: u16, location: &str) -> KernelResponse {
    KernelResponse {
        status,
        content_length: None,
        headers: vec![("Location".to_owned(), location.to_owned())],
        body: Box::new(Cursor::new(Vec::new())),
    }
}

fn partial_response(content_range: &str, body: &[u8]) -> KernelResponse {
    KernelResponse {
        status: 206,
        content_length: Some(body.len() as u64),
        headers: vec![("Content-Range".to_owned(), content_range.to_owned())],
        body: Box::new(Cursor::new(body.to_vec())),
    }
}

fn plain_response(status: u16) -> KernelResponse {
    KernelResponse {
        status,
        content_length: None,
        headers: Vec::new(),
        body: Box::new(Cursor::new(Vec::new())),
    }
}

fn request(url: &str) -> TransportRequest {
    TransportRequest {
        url: url.to_owned(),
        range: None,
        timeout_secs: 30,
    }
}

fn range_request(url: &str) -> TransportRequest {
    TransportRequest {
        url: url.to_owned(),
        range: Some(Range {
            start: 10,
            end: Some(99),
        }),
        timeout_secs: 30,
    }
}

fn read_all(response: &mut TransportResponse) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    response.body.read_to_end(&mut body)?;
    Ok(body)
}

#[test]
fn returns_200_body_and_content_length() {
    let (kernel, _log) = fn_kernel(|_| Ok(ok_response(b"hello")));
    let transport = KernelHttpTransport::new(kernel);
    let mut response = transport.get(&request(LIB_URL)).expect("200 succeeds");
    assert_eq!(response.status, 200);
    assert_eq!(response.content_length, Some(5));
    assert!(!response.supports_range);
    assert_eq!(read_all(&mut response).expect("body is readable"), b"hello");
}

#[test]
fn parses_content_range_on_206() {
    let (kernel, _log) = fn_kernel(|_| Ok(partial_response("bytes 10-14/100", b"world")));
    let transport = KernelHttpTransport::new(kernel);
    let mut response = transport
        .get(&range_request(LIB_URL))
        .expect("206 succeeds");
    assert_eq!(response.status, 206);
    assert!(response.supports_range);
    assert_eq!(
        response.content_range,
        Some(ContentRange {
            start: 10,
            end: 14,
            total: Some(100),
        })
    );
    assert_eq!(read_all(&mut response).expect("body is readable"), b"world");
}

#[test]
fn parses_unknown_total_in_content_range_on_206() {
    let (kernel, _log) = fn_kernel(|_| Ok(partial_response("bytes 0-4/*", b"hello")));
    let transport = KernelHttpTransport::new(kernel);
    let response = transport
        .get(&range_request(LIB_URL))
        .expect("206 with unknown total succeeds");
    assert_eq!(
        response.content_range,
        Some(ContentRange {
            start: 0,
            end: 4,
            total: None,
        })
    );
}

#[test]
fn rejects_malformed_content_range_on_206() {
    for malformed in [
        "chunk 10-14/100",
        "bytes 10/100",
        "bytes 14-10/100",
        "bytes a-b/c",
    ] {
        let (kernel, _log) = fn_kernel(move |_| Ok(partial_response(malformed, b"")));
        let transport = KernelHttpTransport::new(kernel);
        let error = expect_error(
            transport.get(&range_request(LIB_URL)),
            "malformed Content-Range must fail",
        );
        let message = error.to_string();
        assert!(
            message.contains("malformed Content-Range"),
            "{malformed:?}: message was {message}"
        );
    }
}

#[test]
fn forwards_range_header_and_timeout_to_kernel() {
    let (kernel, log) = fn_kernel(|_| Ok(partial_response("bytes 10-99/100", b"0123456789")));
    let transport = KernelHttpTransport::new(kernel);
    transport
        .get(&range_request(LIB_URL))
        .expect("range request succeeds");
    let last = log.last();
    assert_eq!(last.range_header.as_deref(), Some("bytes=10-99"));
    assert_eq!(last.timeout_secs, 30);
}

#[test]
fn maps_kernel_timeout_to_transport_timeout() {
    let (kernel, _log) = fn_kernel(|_| Err(KernelError::Timeout));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(transport.get(&request(LIB_URL)), "timeout fails");
    assert!(
        matches!(
            error,
            TransportError::Timeout {
                timeout_secs: 30,
                ..
            }
        ),
        "{error}"
    );
    assert!(error.to_string().contains("timed out"));
}

#[test]
fn maps_kernel_connection_error() {
    let (kernel, _log) = fn_kernel(|_| Err(KernelError::Connection("dns failure".to_owned())));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(transport.get(&request(LIB_URL)), "connection failure fails");
    assert!(matches!(error, TransportError::Connect { .. }), "{error}");
    let message = error.to_string();
    assert!(message.contains("dns failure"), "message was {message}");
    assert!(message.contains("example.invalid"), "message was {message}");
}

#[test]
fn maps_kernel_io_error() {
    let (kernel, _log) = fn_kernel(|_| Err(KernelError::Io("socket reset".to_owned())));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(transport.get(&request(LIB_URL)), "io failure fails");
    assert!(
        matches!(error, TransportError::ReadInterrupted { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("socket reset"));
}

#[test]
fn reports_200_verbatim_when_range_is_ignored() {
    let (kernel, _log) = fn_kernel(|_| Ok(ok_response(b"full body")));
    let transport = KernelHttpTransport::new(kernel);
    let response = transport
        .get(&range_request(LIB_URL))
        .expect("200 with range requested still succeeds");
    assert_eq!(response.status, 200);
    assert!(!response.supports_range);
    assert_eq!(response.content_range, None);
}

#[test]
fn returns_http_status_error_for_non_2xx() {
    for status in [416u16, 404, 500] {
        let (kernel, _log) = fn_kernel(move |_| Ok(plain_response(status)));
        let transport = KernelHttpTransport::new(kernel);
        let error = expect_error(transport.get(&request(LIB_URL)), "non-2xx must fail");
        match error {
            TransportError::HttpStatus { status: actual, .. } => assert_eq!(actual, status),
            other => panic!("expected HttpStatus for {status}, got {other}"),
        }
    }
}

#[test]
fn follows_one_two_and_three_redirects() {
    for hops in 1..=3 {
        let letters = ["a", "b", "c", "d"];
        let (kernel, log) = fn_kernel(move |request| {
            for index in 0..hops {
                if request.url.ends_with(&format!("/{}.jar", letters[index])) {
                    return Ok(redirect_response(
                        302,
                        &format!("https://example.invalid/{}.jar", letters[index + 1]),
                    ));
                }
            }
            Ok(ok_response(b"landed"))
        });
        let transport = KernelHttpTransport::new(kernel);
        let mut response = transport
            .get(&request("https://example.invalid/a.jar"))
            .expect("redirects within limit succeed");
        assert_eq!(response.status, 200, "{hops} hops");
        assert_eq!(
            read_all(&mut response).expect("body is readable"),
            b"landed"
        );
        assert_eq!(
            log.urls(),
            (0..=hops)
                .map(|index| format!("https://example.invalid/{}.jar", letters[index]))
                .collect::<Vec<_>>(),
            "each hop should re-request the redirect target"
        );
    }
}

#[test]
fn rejects_fourth_redirect() {
    let (kernel, _log) = fn_kernel(|request: &KernelRequest| {
        for letter in ["a", "b", "c", "d"] {
            if request.url.ends_with(&format!("/{letter}.jar")) {
                let next = match letter {
                    "a" => "b",
                    "b" => "c",
                    "c" => "d",
                    _ => "e",
                };
                return Ok(redirect_response(
                    302,
                    &format!("https://example.invalid/{next}.jar"),
                ));
            }
        }
        Ok(ok_response(b"never reached"))
    });
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(
        transport.get(&request("https://example.invalid/a.jar")),
        "fourth redirect must fail",
    );
    assert!(error.to_string().contains("redirect"), "{error}");
}

#[test]
fn rejects_redirect_without_location() {
    let (kernel, _log) = fn_kernel(|_| Ok(plain_response(302)));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(
        transport.get(&request(LIB_URL)),
        "redirect without Location must fail",
    );
    assert!(error.to_string().contains("Location"), "{error}");
}

#[test]
fn rejects_https_to_http_downgrade() {
    let (kernel, _log) =
        fn_kernel(|_| Ok(redirect_response(302, "http://example.invalid/lib.jar")));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(
        transport.get(&request("https://example.invalid/lib.jar")),
        "downgrade must fail",
    );
    let message = error.to_string();
    assert!(message.contains("downgrade"), "message was {message}");
}

#[test]
fn allows_http_to_https_upgrade() {
    let (kernel, _log) = fn_kernel(|request| {
        if request.url.starts_with("http://") {
            Ok(redirect_response(301, "https://example.invalid/lib.jar"))
        } else {
            Ok(ok_response(b"upgraded"))
        }
    });
    let transport = KernelHttpTransport::new(kernel);
    let mut response = transport
        .get(&request("http://example.invalid/lib.jar"))
        .expect("http to https redirect is allowed");
    assert_eq!(response.status, 200);
    assert_eq!(
        read_all(&mut response).expect("body is readable"),
        b"upgraded"
    );
}

#[test]
fn rejects_invalid_redirect_targets() {
    for target in [
        "https:///lib.jar",
        "ftp://example.invalid/lib.jar",
        "example.invalid/lib.jar",
    ] {
        let (kernel, _log) = fn_kernel(move |_| Ok(redirect_response(302, target)));
        let transport = KernelHttpTransport::new(kernel);
        let error = expect_error(
            transport.get(&request(LIB_URL)),
            "invalid redirect target must fail",
        );
        assert!(
            error.to_string().contains("invalid URL"),
            "{target:?}: {error}"
        );
    }
}

#[test]
fn errors_when_reading_past_size_cap() {
    let (kernel, _log) = fn_kernel(|_| Ok(ok_response(&[0u8; 16])));
    let transport = KernelHttpTransport::new(kernel).with_max_bytes(8);
    let mut response = transport.get(&request(LIB_URL)).expect("response opens");
    let mut buffer = [0u8; 4];
    for _ in 0..2 {
        let read = response.body.read(&mut buffer).expect("within cap");
        assert_eq!(read, 4);
    }
    // 第 9 个字节起超出上限。
    let error = response
        .body
        .read(&mut buffer)
        .expect_err("reading past the cap must fail");
    assert!(error.to_string().contains("exceeds size cap"), "{error}");
}

#[test]
fn default_max_bytes_is_512_mib() {
    assert_eq!(DEFAULT_MAX_BYTES, 512 * 1024 * 1024);
}

#[test]
fn redacts_access_token_in_error_messages() {
    let (kernel, _log) = fn_kernel(|_| Err(KernelError::Connection("refused".to_owned())));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(
        transport.get(&request(
            "https://example.invalid/lib.jar?access_token=secret&token=hush",
        )),
        "connection failure fails",
    );
    let message = error.to_string();
    assert!(!message.contains("secret"), "message was {message}");
    assert!(!message.contains("hush"), "message was {message}");
    assert!(!message.contains("access_token"), "message was {message}");
    assert!(message.contains("example.invalid"), "message was {message}");
}

#[test]
fn handles_empty_body_without_content_length() {
    let (kernel, _log) = fn_kernel(|_| Ok(plain_response(200)));
    let transport = KernelHttpTransport::new(kernel);
    let mut response = transport
        .get(&request(LIB_URL))
        .expect("empty body succeeds");
    assert_eq!(response.content_length, None);
    assert_eq!(
        read_all(&mut response).expect("empty body is readable"),
        b""
    );
}

/// 无限产字节的响应体，模拟 chunked 流（无 Content-Length）的恶意超限响应。
struct EndlessBody {
    served: usize,
}

impl Read for EndlessBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.served += buf.len();
        buf.fill(b'x');
        Ok(buf.len())
    }
}

#[test]
fn disconnects_endless_chunked_body_at_size_cap() {
    // 无 Content-Length（chunked）：大小上限必须在读取中实时截断。
    let (kernel, _log) = fn_kernel(|_| {
        Ok(KernelResponse {
            status: 200,
            content_length: None,
            headers: Vec::new(),
            body: Box::new(EndlessBody { served: 0 }),
        })
    });
    let transport = KernelHttpTransport::new(kernel).with_max_bytes(16);
    let mut response = transport
        .get(&request(LIB_URL))
        .expect("stream opens before the cap is reached");
    let mut buffer = [0u8; 8];
    assert_eq!(response.body.read(&mut buffer).expect("first 8 bytes"), 8);
    assert_eq!(response.body.read(&mut buffer).expect("next 8 bytes"), 8);
    // 第 17 个字节起超限：读取必须报错断开，且之后持续报错（不再吐字节）。
    let error = response
        .body
        .read(&mut buffer)
        .expect_err("reading past the cap must fail");
    assert!(error.to_string().contains("exceeds size cap"), "{error}");
    assert!(
        response.body.read(&mut buffer).is_err(),
        "reader stays disconnected after the cap"
    );
}

#[test]
fn redacts_credentials_and_query_in_display_and_debug() {
    let url = "https://user:pw@downloads.example.invalid/lib.jar?token=secret&key=hush";
    let (kernel, _log) = fn_kernel(|_| Err(KernelError::Timeout));
    let transport = KernelHttpTransport::new(kernel);
    let error = expect_error(transport.get(&request(url)), "timeout fails");
    for output in [error.to_string(), format!("{error:?}")] {
        assert!(!output.contains("secret"), "output was {output}");
        assert!(!output.contains("hush"), "output was {output}");
        assert!(!output.contains("token="), "output was {output}");
        assert!(!output.contains("user:pw"), "output was {output}");
        assert!(
            output.contains("downloads.example.invalid"),
            "output was {output}"
        );
    }
}

#[test]
fn maps_ureq_timeout_kinds_to_kernel_timeout() {
    for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
        let error = map_ureq_error(ureq::Error::Io(io::Error::new(kind, "timed out")));
        assert!(
            matches!(error, KernelError::Timeout),
            "{kind:?} should map to Timeout"
        );
    }
}

#[test]
fn maps_ureq_connection_failures_without_url_leak() {
    let error = map_ureq_error(ureq::Error::HostNotFound);
    let KernelError::Connection(message) = error else {
        panic!("HostNotFound should map to Connection");
    };
    assert_eq!(message, "host not found");
    let generic = map_ureq_error(ureq::Error::StatusCode(416));
    let KernelError::Connection(message) = generic else {
        panic!("unexpected mapping");
    };
    assert_eq!(message, "network error");
}

#[test]
fn ureq_kernel_defaults_construct_without_network() {
    // 只验证生产内核可以构造、满足内核契约；不做任何真实请求（零外网）。
    let kernel = UreqKernel::new();
    let also_default = UreqKernel::default();
    let _ = &kernel;
    let _ = &also_default;
}

#[test]
fn production_transport_implements_shared_trait() {
    fn assert_shared_trait<T: super::HttpTransport>() {}
    assert_shared_trait::<KernelHttpTransport<UreqKernel>>();
}
