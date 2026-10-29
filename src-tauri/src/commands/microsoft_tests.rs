use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use aether_auth::device_code::{DEVICE_CODE_URL, DeviceCode, TOKEN_URL};
use aether_auth::minecraft::{ENTITLEMENTS_URL, LOGIN_WITH_XBOX_URL, PROFILE_URL};
use aether_auth::store::StoreError;
use aether_auth::token::{AccessToken, RefreshToken, TokenLifetime};
use aether_auth::xbox::{XBL_AUTHENTICATE_URL, XSTS_AUTHORIZE_URL};

use super::*;

const CLIENT_ID: &str = "00000000-0000-0000-0000-00000000dead";
const PROFILE_ID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
const MC_ACCESS: &str = "mc-access-token-abcdefghij";
const OLD_REFRESH: &str = "old-refresh-token-123456789";
const NEW_REFRESH: &str = "new-refresh-token-987654321";

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn challenge() -> DeviceCodeAuthorization {
    DeviceCodeAuthorization {
        user_code: "ABCD1234".to_owned(),
        verification_uri: "https://microsoft.com/devicelogin".to_owned(),
        device_code: DeviceCode::new("device-code-value-1234567890"),
        expires_in: Duration::from_secs(900),
        interval: Duration::from_secs(5),
    }
}

fn ms_tokens() -> MicrosoftTokens {
    MicrosoftTokens::new(
        AccessToken::new("ms-access-token-0123456789"),
        Some(RefreshToken::new(OLD_REFRESH)),
        TokenLifetime::new(now(), Duration::from_secs(3600)),
    )
}

/// A scripted transport: answers by URL, a queue per URL, remembering every request.
struct Script {
    answers: RefCell<HashMap<String, VecDeque<Result<HttpResponse, HttpError>>>>,
    log: RefCell<Vec<String>>,
}

impl Script {
    fn new() -> Self {
        Self {
            answers: RefCell::new(HashMap::new()),
            log: RefCell::new(Vec::new()),
        }
    }

    fn on(self, url: &str, status: u16, body: &str) -> Self {
        self.answers
            .borrow_mut()
            .entry(url.to_owned())
            .or_default()
            .push_back(Ok(HttpResponse::new(status, body)));
        self
    }

    fn failing(self, url: &str) -> Self {
        self.answers
            .borrow_mut()
            .entry(url.to_owned())
            .or_default()
            .push_back(Err(HttpError::transport(url)));
        self
    }

    /// The five Xbox/Minecraft answers for an account that owns the game.
    fn chain(self) -> Self {
        self.on(
            XBL_AUTHENTICATE_URL,
            200,
            r#"{"Token":"xbl-token","DisplayClaims":{"xui":[{"uhs":"hash123"}]}}"#,
        )
        .on(
            XSTS_AUTHORIZE_URL,
            200,
            r#"{"Token":"xsts-token","DisplayClaims":{"xui":[{"uhs":"hash123","xid":"2535400000000001"}]}}"#,
        )
        .on(
            LOGIN_WITH_XBOX_URL,
            200,
            &format!(r#"{{"access_token":"{MC_ACCESS}","expires_in":86400}}"#),
        )
        .on(
            ENTITLEMENTS_URL,
            200,
            r#"{"items":[{"name":"product_minecraft","signature":"s"}]}"#,
        )
        .on(
            PROFILE_URL,
            200,
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#,
        )
    }

    fn requested(&self, url: &str) -> usize {
        self.log.borrow().iter().filter(|seen| *seen == url).count()
    }
}

impl HttpTransport for Script {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.log.borrow_mut().push(request.url.clone());
        self.answers
            .borrow_mut()
            .get_mut(&request.url)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| Err(HttpError::transport(&request.url)))
    }
}

/// An in-memory vault+metadata store, so the tests touch neither disk nor keyring.
#[derive(Default)]
struct MemoryStore(Mutex<HashMap<AccountId, StoredCredential>>);

impl CredentialStore for MemoryStore {
    fn load(&self, account: AccountId) -> Result<Option<StoredCredential>, StoreError> {
        Ok(self.0.lock().unwrap().get(&account).cloned())
    }
    fn save(&self, credential: &StoredCredential) -> Result<(), StoreError> {
        self.0
            .lock()
            .unwrap()
            .insert(credential.account_id, credential.clone());
        Ok(())
    }
    fn remove(&self, account: AccountId) -> Result<(), StoreError> {
        self.0.lock().unwrap().remove(&account);
        Ok(())
    }
    fn list(&self) -> Result<Vec<AccountId>, StoreError> {
        Ok(self.0.lock().unwrap().keys().copied().collect())
    }
}

struct DeniedStore;

impl CredentialStore for DeniedStore {
    fn load(&self, _: AccountId) -> Result<Option<StoredCredential>, StoreError> {
        Err(StoreError::ProtectedStoreUnavailable)
    }
    fn save(&self, _: &StoredCredential) -> Result<(), StoreError> {
        Err(StoreError::ProtectedStoreUnavailable)
    }
    fn remove(&self, _: AccountId) -> Result<(), StoreError> {
        Err(StoreError::ProtectedStoreUnavailable)
    }
    fn list(&self) -> Result<Vec<AccountId>, StoreError> {
        Err(StoreError::ProtectedStoreUnavailable)
    }
}

fn stored_account(store: &MemoryStore) -> AccountId {
    let id = AccountId::parse(PROFILE_ID).unwrap();
    store
        .save(
            &StoredCredential::metadata_only(id, "Notch", AccountKind::Microsoft)
                .with_refresh(RefreshToken::new(OLD_REFRESH)),
        )
        .unwrap();
    id
}

const TOKENS_OK: &str = r#"{"access_token":"fresh-access-token-1111","refresh_token":"new-refresh-token-987654321","expires_in":3600}"#;

fn poll(script: &Script) -> Result<MicrosoftTokens, PollEnd> {
    poll_for_tokens(
        script,
        &DeviceCodeEndpoints::microsoft(),
        CLIENT_ID,
        &challenge(),
        &|_| true,
        &|| now(),
    )
}

// ---- configuration ------------------------------------------------------------------

#[test]
fn only_an_azure_application_id_counts_as_a_client_id() {
    assert!(is_client_id(CLIENT_ID));
    assert!(is_client_id("0A1B2C3D-4e5f-6789-abcd-ef0123456789"));
    for bad in [
        "",
        "not-a-guid",
        "0000",
        &CLIENT_ID.replace('-', ""),
        &format!("{CLIENT_ID} "),
    ] {
        assert!(!is_client_id(bad), "{bad:?}");
    }
}

// ---- the poll loop ------------------------------------------------------------------

#[test]
fn polling_waits_through_pending_and_returns_the_tokens() {
    let script = Script::new()
        .on(TOKEN_URL, 400, r#"{"error":"authorization_pending"}"#)
        .on(TOKEN_URL, 400, r#"{"error":"authorization_pending"}"#)
        .on(TOKEN_URL, 200, TOKENS_OK);

    let tokens = poll(&script).expect("authorized");

    assert_eq!(tokens.access.expose(), "fresh-access-token-1111");
    assert_eq!(script.requested(TOKEN_URL), 3);
}

#[test]
fn a_declined_sign_in_ends_the_poll() {
    let script = Script::new().on(TOKEN_URL, 400, r#"{"error":"authorization_declined"}"#);
    assert_eq!(poll(&script).unwrap_err(), PollEnd::Denied);
}

#[test]
fn an_expired_code_ends_the_poll() {
    let script = Script::new().on(TOKEN_URL, 400, r#"{"error":"expired_token"}"#);
    assert_eq!(poll(&script).unwrap_err(), PollEnd::Expired);
}

#[test]
fn cancelling_stops_before_any_request() {
    let script = Script::new().on(TOKEN_URL, 200, TOKENS_OK);

    let end = poll_for_tokens(
        &script,
        &DeviceCodeEndpoints::microsoft(),
        CLIENT_ID,
        &challenge(),
        &|_| false,
        &|| now(),
    )
    .unwrap_err();

    assert_eq!(end, PollEnd::Cancelled);
    assert_eq!(script.requested(TOKEN_URL), 0);
}

#[test]
fn a_few_network_blips_are_tolerated_but_a_dead_network_is_not() {
    let blips = Script::new()
        .failing(TOKEN_URL)
        .failing(TOKEN_URL)
        .on(TOKEN_URL, 200, TOKENS_OK);
    assert!(poll(&blips).is_ok());

    let dead = Script::new();
    assert_eq!(poll(&dead).unwrap_err(), PollEnd::Network);
    assert_eq!(dead.requested(TOKEN_URL), MAX_POLL_FAILURES as usize);
}

#[test]
fn an_unknown_oauth_error_ends_the_poll_without_leaking_the_body() {
    let script = Script::new().on(
        TOKEN_URL,
        400,
        r#"{"error":"invalid_client","detail":"secret-detail"}"#,
    );

    let PollEnd::Failed(message) = poll(&script).unwrap_err() else {
        panic!("expected Failed");
    };

    assert!(message.contains("invalid_client"));
    assert!(!message.contains("secret-detail"));
}

// ---- finishing a sign-in --------------------------------------------------------------

#[test]
fn a_finished_sign_in_stores_the_account_and_its_refresh_token() {
    let script = Script::new().chain();
    let store = MemoryStore::default();

    let account = finish_login(&script, &store, &ms_tokens(), now()).expect("signed in");

    assert_eq!(account.id, PROFILE_ID);
    assert_eq!(account.display_name, "Notch");
    assert_eq!(account.uuid, "069a79f444e94726a5befca90e38aaf5");
    assert_eq!(account.xuid.as_deref(), Some("2535400000000001"));

    let saved = store
        .load(AccountId::parse(PROFILE_ID).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(saved.kind, AccountKind::Microsoft);
    assert_eq!(
        saved.refresh.as_ref().map(RefreshToken::expose),
        Some(OLD_REFRESH)
    );
}

#[test]
fn signing_in_again_replaces_the_account_instead_of_duplicating_it() {
    let store = MemoryStore::default();
    for _ in 0..2 {
        finish_login(&Script::new().chain(), &store, &ms_tokens(), now()).unwrap();
    }
    assert_eq!(store.list().unwrap().len(), 1);
}

#[test]
fn an_account_that_cannot_be_stored_is_a_storage_failure() {
    let failure = finish_login(&Script::new().chain(), &DeniedStore, &ms_tokens(), now())
        .expect_err("storage denied");
    assert_eq!(failure.reason, "storage");
}

#[test]
fn xbox_and_minecraft_refusals_get_stable_reasons() {
    let child = Script::new()
        .on(
            XBL_AUTHENTICATE_URL,
            200,
            r#"{"Token":"t","DisplayClaims":{"xui":[{"uhs":"h"}]}}"#,
        )
        .on(XSTS_AUTHORIZE_URL, 401, r#"{"XErr":2148916238}"#);
    assert_eq!(
        finish_login(&child, &MemoryStore::default(), &ms_tokens(), now())
            .unwrap_err()
            .reason,
        "child-account"
    );

    let mut no_game = Script::new().chain();
    no_game.answers.get_mut().insert(
        ENTITLEMENTS_URL.to_owned(),
        VecDeque::from([Ok(HttpResponse::new(200, r#"{"items":[]}"#))]),
    );
    assert_eq!(
        finish_login(&no_game, &MemoryStore::default(), &ms_tokens(), now())
            .unwrap_err()
            .reason,
        "not-entitled"
    );

    let offline = Script::new();
    assert_eq!(
        finish_login(&offline, &MemoryStore::default(), &ms_tokens(), now())
            .unwrap_err()
            .reason,
        "network"
    );
}

// ---- launch credentials ----------------------------------------------------------------

fn launch(
    script: &Script,
    store: &dyn CredentialStore,
    id: &str,
) -> Result<LaunchCredentials, String> {
    credentials_for_launch(
        script,
        store,
        &DeviceCodeEndpoints::microsoft(),
        CLIENT_ID,
        id,
        now(),
    )
}

#[test]
fn a_launch_refreshes_the_sign_in_and_rotates_the_stored_token() {
    let store = MemoryStore::default();
    let id = stored_account(&store);
    let script = Script::new().on(TOKEN_URL, 200, TOKENS_OK).chain();

    let credentials = launch(&script, &store, PROFILE_ID).expect("credentials");

    assert_eq!(credentials.name, "Notch");
    assert_eq!(credentials.uuid, "069a79f444e94726a5befca90e38aaf5");
    assert_eq!(credentials.access_token.expose(), MC_ACCESS);
    assert_eq!(credentials.xuid, "2535400000000001");
    let saved = store.load(id).unwrap().unwrap();
    assert_eq!(
        saved.refresh.as_ref().map(RefreshToken::expose),
        Some(NEW_REFRESH)
    );
}

#[test]
fn a_revoked_sign_in_asks_the_user_to_sign_in_again() {
    let store = MemoryStore::default();
    stored_account(&store);
    let script = Script::new().on(TOKEN_URL, 400, r#"{"error":"invalid_grant"}"#);

    assert_eq!(
        launch(&script, &store, PROFILE_ID).err().as_deref(),
        Some(SIGN_IN_REQUIRED)
    );
}

#[test]
fn an_account_without_a_stored_refresh_token_needs_a_new_sign_in() {
    let store = MemoryStore::default();
    let id = AccountId::parse(PROFILE_ID).unwrap();
    store
        .save(&StoredCredential::metadata_only(
            id,
            "Notch",
            AccountKind::Microsoft,
        ))
        .unwrap();

    let script = Script::new();
    assert_eq!(
        launch(&script, &store, PROFILE_ID).err().as_deref(),
        Some(SIGN_IN_REQUIRED)
    );
    assert_eq!(script.requested(TOKEN_URL), 0);
}

#[test]
fn an_outage_is_not_mistaken_for_a_revoked_sign_in() {
    let store = MemoryStore::default();
    stored_account(&store);
    let error = launch(&Script::new(), &store, PROFILE_ID).err().unwrap();
    assert_ne!(error, SIGN_IN_REQUIRED);
    assert!(error.contains("login.microsoftonline.com"));
}

#[test]
fn launch_rejects_unknown_malformed_and_offline_accounts() {
    let store = MemoryStore::default();
    assert!(launch(&Script::new(), &store, "../escape").is_err());
    assert!(launch(&Script::new(), &store, PROFILE_ID).is_err());

    let offline = AccountId::new();
    store
        .save(&StoredCredential::metadata_only(
            offline,
            "Steve",
            AccountKind::Offline,
        ))
        .unwrap();
    assert!(launch(&Script::new(), &store, &offline.as_uuid().to_string()).is_err());
}

#[test]
fn a_failure_to_store_the_rotated_token_does_not_stop_the_launch() {
    struct ReadOnly(MemoryStore);
    impl CredentialStore for ReadOnly {
        fn load(&self, a: AccountId) -> Result<Option<StoredCredential>, StoreError> {
            self.0.load(a)
        }
        fn save(&self, _: &StoredCredential) -> Result<(), StoreError> {
            Err(StoreError::ProtectedStoreUnavailable)
        }
        fn remove(&self, a: AccountId) -> Result<(), StoreError> {
            self.0.remove(a)
        }
        fn list(&self) -> Result<Vec<AccountId>, StoreError> {
            self.0.list()
        }
    }
    let inner = MemoryStore::default();
    stored_account(&inner);
    let script = Script::new().on(TOKEN_URL, 200, TOKENS_OK).chain();

    assert!(launch(&script, &ReadOnly(inner), PROFILE_ID).is_ok());
}

// ---- listing ------------------------------------------------------------------------------

#[test]
fn listing_returns_only_microsoft_accounts_sorted_by_name() {
    let store = MemoryStore::default();
    for (name, kind) in [
        ("zed", AccountKind::Microsoft),
        ("Alex", AccountKind::Microsoft),
        ("offline", AccountKind::Offline),
    ] {
        store
            .save(&StoredCredential::metadata_only(
                AccountId::new(),
                name,
                kind,
            ))
            .unwrap();
    }

    let names: Vec<String> = list_accounts(&store)
        .unwrap()
        .into_iter()
        .map(|account| account.display_name)
        .collect();

    assert_eq!(names, ["Alex", "zed"]);
}

#[test]
fn an_unreadable_store_is_reported_without_details() {
    assert_eq!(
        list_accounts(&DeniedStore).unwrap_err(),
        "the account list could not be read"
    );
}

// ---- what crosses IPC -----------------------------------------------------------------------

#[test]
fn events_use_camel_case_and_carry_no_token() {
    let signed_in = LoginEvent::SignedIn {
        account: MicrosoftAccountDto {
            id: PROFILE_ID.to_owned(),
            display_name: "Notch".to_owned(),
            uuid: "069a79f444e94726a5befca90e38aaf5".to_owned(),
            xuid: Some("1".to_owned()),
        },
    };
    let value = serde_json::to_value(&signed_in).unwrap();
    assert_eq!(value["state"], "signed-in");
    assert_eq!(value["account"]["displayName"], "Notch");
    assert!(value["account"].get("display_name").is_none());

    let failed = serde_json::to_value(LoginEvent::Failed {
        failure: LoginFailure {
            reason: "denied",
            message: "the sign-in was declined".to_owned(),
        },
    })
    .unwrap();
    assert_eq!(failed["state"], "failed");
    assert_eq!(failed["failure"]["reason"], "denied");

    assert_eq!(
        serde_json::to_value(LoginEvent::Waiting).unwrap()["state"],
        "waiting"
    );
    assert_eq!(
        serde_json::to_value(LoginEvent::Cancelled).unwrap()["state"],
        "cancelled"
    );
}

#[test]
fn the_device_code_dto_shows_only_what_the_user_needs() {
    let value = serde_json::to_value(DeviceCodeDto {
        user_code: "ABCD1234".to_owned(),
        verification_uri: "https://microsoft.com/devicelogin".to_owned(),
        expires_in_secs: 900,
    })
    .unwrap();

    assert_eq!(value["userCode"], "ABCD1234");
    assert_eq!(value["expiresInSecs"], 900);
    assert_eq!(value.as_object().unwrap().len(), 3);
}

#[test]
fn only_one_sign_in_runs_at_a_time_and_it_can_be_cancelled() {
    let state = MicrosoftLoginState::default();
    assert!(!state.cancel());

    let first = state.start().expect("first");
    assert!(state.start().is_err());
    assert!(state.cancel());
    assert!(first.cancel.load(Ordering::SeqCst));

    first.done.store(true, Ordering::SeqCst);
    assert!(!state.cancel());
    assert!(state.start().is_ok());
}

#[test]
fn the_device_code_request_goes_to_the_microsoft_endpoint() {
    let script = Script::new().on(
        DEVICE_CODE_URL,
        200,
        r#"{"user_code":"ABCD1234","verification_uri":"https://microsoft.com/devicelogin","device_code":"device-code-value-1234567890","expires_in":900,"interval":5}"#,
    );
    let challenge = request_device_code(&script, CLIENT_ID).expect("challenge");
    assert_eq!(challenge.user_code, "ABCD1234");

    let refused = Script::new().on(
        DEVICE_CODE_URL,
        400,
        r#"{"error":"unauthorized_client","secret":"x"}"#,
    );
    let message = request_device_code(&refused, CLIENT_ID).unwrap_err();
    assert!(message.contains("HTTP 400"));
    assert!(!message.contains("secret"));
}

#[test]
fn only_https_microsoft_pages_are_opened() {
    for good in [
        "https://www.microsoft.com/link",
        "https://microsoft.com/devicelogin",
        "https://login.microsoftonline.com/common/oauth2/deviceauth",
        "https://aka.ms/devicelogin",
    ] {
        assert!(is_trusted_verification_uri(good), "{good}");
    }
    for bad in [
        "http://microsoft.com/devicelogin",
        "https://evil.example/devicelogin",
        "https://microsoft.com.evil.example/devicelogin",
        "https://user@microsoft.com/x",
        "https://microsoft.com:8443/x",
        "https://microsoft.com/a b",
        "file:///C:/Windows/System32/calc.exe",
        "javascript:alert(1)",
        "",
    ] {
        assert!(!is_trusted_verification_uri(bad), "{bad}");
    }
}
