//! Pure Yggdrasil request builders and bounded response parsers.
use crate::http::{HttpRequest, HttpResponse};
use crate::token::RefreshToken;
use serde::{Deserialize, Serialize};

pub const LITTLE_SKIN: &str = "https://littleskin.cn/api/yggdrasil/";
pub const ELY_BY: &str = "https://account.ely.by/api/authlib-injector/";
pub const MAX_BODY: usize = 256 * 1024;
pub type Result<T> = std::result::Result<T, &'static str>;

/// A deliberately narrow HTTPS API root. No ALI discovery or redirects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server(String);
impl Server {
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        let rest = input
            .strip_prefix("https://")
            .ok_or("yggdrasil-invalid-server")?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority
            .strip_suffix(":443")
            .unwrap_or(authority)
            .to_ascii_lowercase();
        let labels: Vec<_> = host.split('.').collect();
        if input.len() > 2048
            || labels.len() < 2
            || host.len() > 253
            || labels.iter().any(|s| {
                s.is_empty()
                    || s.len() > 63
                    || s.starts_with('-')
                    || s.ends_with('-')
                    || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
            || !labels
                .last()
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_alphabetic()))
            || [
                "localhost",
                "local",
                "internal",
                "lan",
                "home",
                "test",
                "invalid",
            ]
            .iter()
            .any(|s| host == *s || host.ends_with(&format!(".{s}")))
            || !path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_.~".contains(&b))
            || path.split('/').any(|s| s == "." || s == "..")
        {
            return Err("yggdrasil-invalid-server");
        }
        let path = path.trim_end_matches('/');
        Ok(Self(if path.is_empty() {
            format!("https://{host}/")
        } else {
            format!("https://{host}/{path}/")
        }))
    }
    pub fn root(&self) -> &str {
        &self.0
    }
    fn endpoint(&self, name: &str) -> String {
        format!("{}authserver/{name}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
}
impl Profile {
    pub fn valid(&self) -> bool {
        self.id.len() == 32
            && self.id.bytes().all(|b| b.is_ascii_hexdigit())
            && !self.name.is_empty()
            && self.name.len() <= 64
            && !self.name.chars().any(char::is_control)
    }
}

/// No Debug or Serialize: secrets may only be packed for the OS vault.
#[derive(Clone)]
pub struct Tokens {
    access: String,
    client: String,
}
impl Tokens {
    pub fn client(&self) -> &str {
        &self.client
    }
    pub fn access(&self) -> &str {
        &self.access
    }
    pub fn to_vault(&self) -> RefreshToken {
        // The constant trailer keeps even the legacy redacted Debug tail nonsecret.
        RefreshToken::new(format!(
            "{}#END",
            serde_json::json!([self.access, self.client])
        ))
    }
    pub fn from_vault(value: &RefreshToken) -> Result<Self> {
        let raw = value
            .expose()
            .strip_suffix("#END")
            .ok_or("yggdrasil-sign-in-required")?;
        if raw.len() > MAX_BODY {
            return Err("yggdrasil-sign-in-required");
        }
        let pair: [String; 2] =
            serde_json::from_str(raw).map_err(|_| "yggdrasil-sign-in-required")?;
        let [access, client] = pair;
        if access.is_empty() || client.is_empty() {
            return Err("yggdrasil-sign-in-required");
        }
        Ok(Self { access, client })
    }
}
pub struct Session {
    pub tokens: Tokens,
    pub profiles: Vec<Profile>,
    pub selected: Option<Profile>,
}

pub fn build_authenticate(
    server: &Server,
    username: &str,
    password: &str,
    client: &str,
) -> Result<HttpRequest> {
    if username.is_empty()
        || username.len() > 320
        || password.is_empty()
        || password.len() > 4096
        || client.is_empty()
        || client.len() > 128
    {
        return Err("yggdrasil-invalid-input");
    }
    Ok(HttpRequest::post_json(server.endpoint("authenticate"), serde_json::json!({"agent":{"name":"Minecraft","version":1},"username":username,"password":password,"clientToken":client,"requestUser":false}).to_string()))
}
pub fn build_refresh(server: &Server, tokens: &Tokens, selected: Option<&Profile>) -> HttpRequest {
    let mut body = serde_json::json!({"accessToken":tokens.access,"clientToken":tokens.client,"requestUser":false});
    if let Some(profile) = selected {
        body["selectedProfile"] = serde_json::json!(profile);
    }
    HttpRequest::post_json(server.endpoint("refresh"), body.to_string())
}
pub fn build_validate(server: &Server, tokens: &Tokens) -> HttpRequest {
    token_request(server, tokens, "validate")
}
pub fn build_invalidate(server: &Server, tokens: &Tokens) -> HttpRequest {
    token_request(server, tokens, "invalidate")
}
fn token_request(server: &Server, tokens: &Tokens, method: &str) -> HttpRequest {
    HttpRequest::post_json(
        server.endpoint(method),
        serde_json::json!({"accessToken":tokens.access,"clientToken":tokens.client}).to_string(),
    )
}
fn check(response: &HttpResponse, status: u16) -> Result<()> {
    if response.body.len() > MAX_BODY {
        return Err("yggdrasil-response-too-large");
    }
    match response.status {
        s if s == status => Ok(()),
        401 | 403 => Err("yggdrasil-sign-in-required"),
        _ => Err("yggdrasil-service-error"),
    }
}
pub fn parse_empty(response: &HttpResponse) -> Result<()> {
    check(response, 204)
}
pub fn parse_session(response: &HttpResponse, expected_client: Option<&str>) -> Result<Session> {
    check(response, 200)?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Wire {
        access_token: String,
        client_token: String,
        #[serde(default)]
        available_profiles: Vec<Profile>,
        selected_profile: Option<Profile>,
    }
    let wire: Wire =
        serde_json::from_str(&response.body).map_err(|_| "yggdrasil-invalid-response")?;
    if wire.access_token.is_empty()
        || wire.client_token.is_empty()
        || expected_client.is_some_and(|c| c != wire.client_token)
        || wire.available_profiles.len() > 100
        || wire.available_profiles.iter().any(|p| !p.valid())
        || wire.selected_profile.as_ref().is_some_and(|p| !p.valid())
    {
        return Err("yggdrasil-invalid-response");
    }
    Ok(Session {
        tokens: Tokens {
            access: wire.access_token,
            client: wire.client_token,
        },
        profiles: wire.available_profiles,
        selected: wire.selected_profile,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub skin_domains: Vec<String>,
    pub signature_publickey: String,
}
pub fn parse_metadata(response: &HttpResponse) -> Result<Metadata> {
    check(response, 200)?;
    let value: Metadata =
        serde_json::from_str(&response.body).map_err(|_| "yggdrasil-invalid-metadata")?;
    if value.skin_domains.len() > 100
        || value
            .skin_domains
            .iter()
            .any(|s| s.len() > 253 || s.chars().any(char::is_control))
        || value.signature_publickey.len() > 16384
        || !value
            .signature_publickey
            .starts_with("-----BEGIN PUBLIC KEY-----\n")
        || !value
            .signature_publickey
            .trim_end()
            .ends_with("-----END PUBLIC KEY-----")
    {
        return Err("yggdrasil-invalid-metadata");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yggdrasil_server_policy() {
        assert_eq!(
            Server::parse(" https://EXAMPLE.org:443/api ")
                .unwrap()
                .root(),
            "https://example.org/api/"
        );
        for value in [
            "http://example.org",
            "https://user:pass@example.org/",
            "https://127.0.0.1",
            "https://[::1]",
            "https://example.local",
            "https://example.org/?token=x",
            "https://example.org/#x",
            "https://example.org/%2f",
            "https://example.org/../x",
            "https://example.org:444",
            "https://2130706433",
        ] {
            assert!(Server::parse(value).is_err(), "{value}");
        }
    }
    #[test]
    fn yggdrasil_secrets_and_errors() {
        let server = Server::parse(LITTLE_SKIN).unwrap();
        let request =
            build_authenticate(&server, "user", "password-secret", "client-secret").unwrap();
        assert!(request.url.ends_with("/authserver/authenticate"));
        assert!(!format!("{request:?}").contains("password-secret"));
        assert_eq!(
            parse_empty(&HttpResponse::new(403, "secret-body")),
            Err("yggdrasil-sign-in-required")
        );
        assert_eq!(
            parse_empty(&HttpResponse::new(200, "")),
            Err("yggdrasil-service-error")
        );
        assert!(parse_session(&HttpResponse::new(200, "[".repeat(MAX_BODY + 1)), None).is_err());
        assert!(
            parse_session(
                &HttpResponse::new(200, format!("{}0{}", "[".repeat(140), "]".repeat(140))),
                None
            )
            .is_err()
        );
    }
    #[test]
    fn yggdrasil_session_roundtrip() {
        let response = HttpResponse::new(
            200,
            r#"{"accessToken":"access-secret","clientToken":"client-secret","availableProfiles":[{"id":"0123456789abcdef0123456789abcdef","name":"Player"}]}"#,
        );
        let session = parse_session(&response, Some("client-secret")).unwrap();
        assert!(parse_session(&response, Some("other")).is_err());
        let vault = session.tokens.to_vault();
        assert!(!format!("{vault:?}").contains("secret"));
        assert_eq!(
            Tokens::from_vault(&vault).unwrap().access(),
            "access-secret"
        );
        let server = Server::parse(LITTLE_SKIN).unwrap();
        let refresh = build_refresh(&server, &session.tokens, session.profiles.first());
        assert!(refresh.body().unwrap().contains("selectedProfile"));
        assert!(
            build_validate(&server, &session.tokens)
                .url
                .ends_with("/validate")
        );
        assert!(
            build_invalidate(&server, &session.tokens)
                .url
                .ends_with("/invalidate")
        );
    }
}
