//! Direct HTTPS only; resolved socket addresses are checked before connection.
use aether_auth::http::{HttpError, HttpMethod, HttpRequest, HttpResponse, HttpTransport};
use aether_auth::yggdrasil as protocol;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;
#[derive(Debug)]
struct PublicResolver;
impl Resolver for PublicResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = DefaultResolver::default().resolve(uri, config, timeout)?;
        // A dual-stack host often answers with a mix of A and AAAA records; keep every public
        // address and fail only when the filtered set is empty. This still re-checks the DNS
        // answer on every request, so rebinding is not weakened.
        let mut public =
            ResolvedSocketAddrs::from_fn(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
        for socket in addresses.iter() {
            if public_ip(socket.ip()) {
                public
                    .try_push(*socket)
                    .map_err(|_| ureq::Error::HostNotFound)?;
            }
        }
        if public.is_empty() {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(public)
    }
}
pub(super) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_unspecified()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0)
                && !(a == 192 && b == 88 && c == 99)
                && !(a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                && !(a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            // 2000::/3 is the globally routable unicast space. Everything outside it is refused.
            if (s[0] & 0xe000) != 0x2000 {
                return false;
            }
            // Exclude the transition, benchmarking, ORCHID and documentation ranges that sit
            // inside 2000::/3 but must not be used as ordinary public destinations.
            !(s[0] == 0x2001 && s[1] == 0x0000)          // Teredo, RFC 4380
    && !(s[0] == 0x2001 && s[1] == 0x0002)       // benchmarking, RFC 5180
    && !(s[0] == 0x2001 && (s[1] & 0xfff0) == 0x0010) // ORCHIDv1, RFC 4843
    && !(s[0] == 0x2001 && (s[1] & 0xfff0) == 0x0020) // ORCHIDv2, RFC 7343
    && !(s[0] == 0x2001 && s[1] == 0x0db8)       // documentation, RFC 3849
    && s[0] != 0x2002                              // 6to4, RFC 3056
    && (s[0] & 0xfff0) != 0x3ff0 // documentation, RFC 9637
        }
    }
}
fn agent() -> ureq::Agent {
    let config = ureq::config::Config::builder()
        .http_status_as_error(false)
        .proxy(None)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(concat!("AetherLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(30)))
        .build();
    ureq::Agent::with_parts(
        config,
        ureq::unversioned::transport::DefaultConnector::default(),
        PublicResolver,
    )
}
/// A raw, capped `GET` of binary bytes over the public-address-only resolver. Used to
/// download the authlib-injector JAR; the caller verifies the bytes before trusting them.
pub(crate) fn public_get_bytes(url: &str, cap: u64) -> Result<Vec<u8>, ()> {
    let mut response = agent().get(url).call().map_err(|_| ())?;
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(());
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(cap)
        .read_to_vec()
        .map_err(|_| ())?;
    Ok(bytes)
}
pub(crate) fn public_get_artifact_bytes(url: &str, cap: u64) -> Result<Vec<u8>, ()> {
    let agent = agent();
    let mut next = url.to_owned();
    for _ in 0..5 {
        let uri: ureq::http::Uri = next.parse().map_err(|_| ())?;
        if uri.scheme_str() != Some("https")
            || uri.authority().is_none_or(|a| a.as_str().contains('@'))
        {
            return Err(());
        }
        let mut response = agent.get(&next).call().map_err(|_| ())?;
        if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            let location = response
                .headers()
                .get("location")
                .ok_or(())?
                .to_str()
                .map_err(|_| ())?;
            next = if location.starts_with('/') && !location.starts_with("//") {
                format!("https://{}{}", uri.authority().ok_or(())?, location)
            } else {
                location.to_owned()
            };
            continue;
        }
        if !response.status().is_success() {
            return Err(());
        }
        return response
            .body_mut()
            .with_config()
            .limit(cap)
            .read_to_vec()
            .map_err(|_| ());
    }
    Err(())
}

pub(crate) struct PublicTransport;
impl HttpTransport for PublicTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let agent = agent();
        let failed = |_| HttpError::transport(&request.url);

        let mut response = match request.method {
            HttpMethod::Get => {
                let mut call = agent.get(&request.url);
                for (name, value) in &request.headers {
                    call = call.header(name, value);
                }
                call.call().map_err(failed)?
            }
            HttpMethod::Post => {
                let mut call = agent.post(&request.url);
                for (name, value) in &request.headers {
                    call = call.header(name, value);
                }
                match request.body_bytes() {
                    Some(bytes) => call.send(bytes).map_err(failed)?,
                    None => call.send(request.body().unwrap_or("")).map_err(failed)?,
                }
            }
        };
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(protocol::MAX_BODY as u64)
            .read_to_string()
            .map_err(failed)?;
        Ok(HttpResponse::new(status, body))
    }
}
