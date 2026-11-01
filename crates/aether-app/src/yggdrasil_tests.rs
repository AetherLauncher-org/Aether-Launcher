use super::*;
use std::cell::{Cell, RefCell};
use aether_auth::http::HttpError;
use aether_auth::store::StoreError;

const PROFILE: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "1123456789abcdef0123456789abcdef";
#[derive(Default)]
struct Store {
    values: RefCell<Vec<StoredCredential>>,
    fail_save: Cell<bool>,
}
impl CredentialStore for Store {
    fn load(&self, id: AccountId) -> Result<Option<StoredCredential>, StoreError> {
        Ok(self.values.borrow().iter().find(|v| v.account_id == id).cloned())
    }
    fn save(&self, value: &StoredCredential) -> Result<(), StoreError> {
        if self.fail_save.get() { return Err(StoreError::ProtectedStoreUnavailable); }
        self.values.borrow_mut().retain(|v| v.account_id != value.account_id);
        self.values.borrow_mut().push(value.clone());
        Ok(())
    }
    fn remove(&self, id: AccountId) -> Result<(), StoreError> {
        self.values.borrow_mut().retain(|v| v.account_id != id);
        Ok(())
    }
    fn list(&self) -> Result<Vec<AccountId>, StoreError> {
        Ok(self.values.borrow().iter().map(|v| v.account_id).collect())
    }
}
struct Transport {
    requests: RefCell<Vec<HttpRequest>>,
    fail: bool,
    wrong_profile: bool,
}
impl Transport {
    fn new() -> Self { Self { requests: RefCell::new(Vec::new()), fail: false, wrong_profile: false } }
}
impl HttpTransport for Transport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.requests.borrow_mut().push(request.clone());
        if self.fail { return Err(HttpError::transport(&request.url)); }
        if request.url.ends_with("/authserver/invalidate") { return Ok(HttpResponse::new(204, "")); }
        if request.url.ends_with("/authserver/authenticate") || request.url.ends_with("/authserver/refresh") {
            let body: serde_json::Value = serde_json::from_str(request.body().unwrap()).unwrap();
            let mut response = serde_json::json!({"accessToken":"access-secret-rotated", "clientToken":body["clientToken"], "availableProfiles":[{"id":PROFILE,"name":"Player"},{"id":OTHER,"name":"Other"}]});
            if request.url.ends_with("/refresh") {
                response["selectedProfile"] = serde_json::json!({"id":if self.wrong_profile {OTHER} else {PROFILE},"name":"Renamed"});
            }
            return Ok(HttpResponse::new(200, response.to_string()));
        }
        Ok(HttpResponse::new(200, r#"{"signaturePublickey":"-----BEGIN PUBLIC KEY-----\nYWJj\n-----END PUBLIC KEY-----","skinDomains":[]}"#))
    }
}
fn pending() -> (Option<Pending>, Transport, Challenge) {
    let transport = Transport::new();
    let mut pending = None;
    let challenge = begin_with(&mut pending, &transport, protocol::LITTLE_SKIN, "user", "password-secret", Instant::now()).unwrap();
    (pending, transport, challenge)
}
#[test]
fn yggdrasil_login_binds_profile_and_returns_identity_only() {
    let (mut pending, transport, challenge) = pending();
    assert_eq!(challenge.profiles.len(), 2);
    let store = Store::default();
    let account = finish_with(&mut pending, &store, &transport, &challenge.id, PROFILE, Instant::now()).unwrap();
    assert!(pending.is_none());
    let json = serde_json::to_string(&account).unwrap();
    assert!(!json.contains("secret"));
    assert!(!json.contains("Token"));
    let request = transport.requests.borrow().last().unwrap().clone();
    let body: serde_json::Value = serde_json::from_str(request.body().unwrap()).unwrap();
    assert_eq!(body["selectedProfile"]["id"], PROFILE);
    assert_eq!(Tokens::from_vault(store.values.borrow()[0].refresh.as_ref().unwrap()).unwrap().access(), "access-secret-rotated");
}
#[test]
fn yggdrasil_save_retry_does_not_refresh_rotated_token_again() {
    let (mut pending, transport, challenge) = pending();
    let store = Store::default();
    store.fail_save.set(true);
    assert_eq!(finish_with(&mut pending, &store, &transport, &challenge.id, PROFILE, Instant::now()).err().as_deref(), Some(STORAGE));
    let count = transport.requests.borrow().len();
    store.fail_save.set(false);
    finish_with(&mut pending, &store, &transport, &challenge.id, PROFILE, Instant::now()).unwrap();
    assert_eq!(transport.requests.borrow().len(), count);
}
#[test]
fn yggdrasil_relogin_updates_same_server_profile() {
    let store = Store::default();
    let (mut first, transport, challenge) = pending();
    let one = finish_with(&mut first, &store, &transport, &challenge.id, PROFILE, Instant::now()).unwrap();
    let (mut second, transport, challenge) = pending();
    let two = finish_with(&mut second, &store, &transport, &challenge.id, PROFILE, Instant::now()).unwrap();
    assert_eq!(one.id, two.id);
    assert_eq!(store.values.borrow().len(), 1);
}
#[test]
fn yggdrasil_rejects_unoffered_and_substituted_profiles() {
    let (mut pending, mut transport, challenge) = pending();
    let store = Store::default();
    let count = transport.requests.borrow().len();
    assert!(finish_with(&mut pending, &store, &transport, &challenge.id, "bad", Instant::now()).is_err());
    assert_eq!(transport.requests.borrow().len(), count);
    transport.wrong_profile = true;
    assert_eq!(finish_with(&mut pending, &store, &transport, &challenge.id, PROFILE, Instant::now()).err().as_deref(), Some("yggdrasil-invalid-profile"));
    assert!(store.values.borrow().is_empty());
}
#[test]
fn yggdrasil_cancel_and_expiry_discard_pending_credentials() {
    let (mut login, transport, challenge) = pending();
    cancel_with(&mut login, &transport, "wrong");
    assert!(login.is_some());
    cancel_with(&mut login, &transport, &challenge.id);
    assert!(login.is_none());
    assert!(transport.requests.borrow().last().unwrap().url.ends_with("/invalidate"));
    let (mut login, transport, challenge) = pending();
    assert_eq!(finish_with(&mut login, &Store::default(), &transport, &challenge.id, PROFILE, Instant::now() + LOGIN_TTL).err().as_deref(), Some("yggdrasil-login-expired"));
    assert!(login.is_none());
}
#[test]
fn yggdrasil_refresh_checks_identity_and_remove_works_offline() {
    let (mut login, mut transport, challenge) = pending();
    let store = Store::default();
    let account = finish_with(&mut login, &store, &transport, &challenge.id, PROFILE, Instant::now()).unwrap();
    let id = AccountId::parse(&account.id).unwrap();
    refresh_with(&store, &transport, id).unwrap();
    transport.wrong_profile = true;
    assert_eq!(refresh_with(&store, &transport, id).unwrap_err(), "yggdrasil-invalid-profile");
    transport.fail = true;
    remove_with(&store, &transport, id).unwrap();
    assert!(store.values.borrow().is_empty());
}
#[test]
fn yggdrasil_invalid_input_never_contacts_server() {
    let transport = Transport::new();
    assert!(begin_with(&mut None, &transport, protocol::LITTLE_SKIN, "user", "", Instant::now()).is_err());
    assert!(transport.requests.borrow().is_empty());
}
#[test]
fn yggdrasil_network_policy_rejects_private_and_transition_addresses() {
    for ip in ["127.0.0.1", "10.0.0.1", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "192.0.2.1", "198.51.100.1", "203.0.113.1", "198.18.0.1", "224.0.0.1", "0.0.0.0", "240.0.0.1", "::1", "::ffff:127.0.0.1", "fc00::1", "fe80::1", "2001:db8::1", "2002:7f00:1::", "3fff::1"] {
        assert!(!transport::public_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
        assert!(transport::public_ip(ip.parse().unwrap()), "{ip}");
    }
}
