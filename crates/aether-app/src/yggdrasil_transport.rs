//! Direct HTTPS only; resolved socket addresses are checked before connection.
use std::time::Duration;
use std::net::IpAddr;
use aether_auth::http::{HttpRequest, HttpResponse, HttpTransport, HttpMethod, HttpError};
use aether_auth::yggdrasil as protocol;
use ureq::unversioned::resolver::{Resolver, DefaultResolver, ResolvedSocketAddrs};
use ureq::unversioned::transport::NextTimeout;
#[derive(Debug)]
struct PublicResolver;
impl Resolver for PublicResolver {
 fn resolve(&self, uri: &ureq::http::Uri, config: &ureq::config::Config, timeout: NextTimeout) -> Result<ResolvedSocketAddrs, ureq::Error> {
  let addresses = DefaultResolver::default().resolve(uri, config, timeout)?;
  if addresses.iter().any(|s| !public_ip(s.ip())) { return Err(ureq::Error::HostNotFound); }
  Ok(addresses)
 }
}
pub(super) fn public_ip(ip: IpAddr) -> bool {
 match ip {
  IpAddr::V4(ip) => { let [a,b,c,_] = ip.octets();
   !ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_multicast() && !ip.is_broadcast() && !ip.is_unspecified()
   && a != 0 && a < 224 && !(a == 100 && (64..=127).contains(&b)) && !(a == 192 && b == 0) && !(a == 192 && b == 88 && c == 99)
   && !(a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100))) && !(a == 203 && b == 0 && c == 113)
  },
  // Conservative IPv6 policy: global unicast only, exclude documentation and transition ranges.
  IpAddr::V6(ip) => { let s = ip.segments(); (s[0] & 0xe000) == 0x2000 && s[0] != 0x2001 && s[0] != 0x2002 && !(s[0] == 0x3fff) }
 }
}
pub(super) struct PublicTransport;
impl HttpTransport for PublicTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let config = ureq::config::Config::builder()
            .http_status_as_error(false)
            .proxy(None)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(concat!("AetherLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        let agent = ureq::Agent::with_parts(config, ureq::unversioned::transport::DefaultConnector::default(), PublicResolver);
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

