//! Microsoft account sign-in and the credentials a launch needs.
//!
//! Sign-in is the OAuth device code flow: [`begin_microsoft_login`] returns a short code
//! and a verification page, a background thread polls until the user finishes in the
//! browser, then runs the Xbox and Minecraft stages (`aether_auth::signin`). Progress
//! reaches the frontend as `login://status` events.
//!
//! # What crosses IPC
//!
//! Only the device code *display* values, account identity (name, UUID, Xbox user id)
//! and stable failure labels. Access tokens, refresh tokens and device codes never leave
//! the backend. The refresh token lives in the operating system's credential vault
//! (`KeyringCredentialStore`); the JSON next to it holds metadata only. A launch gets a
//! fresh Minecraft token by refreshing, so no Minecraft token is stored at all.
//!
//! # The client id
//!
//! The Azure application id is public but must be registered and approved by Mojang
//! (see `docs/modules/microsoft-login.md`). It is read from the `AETHER_MS_CLIENT_ID`
//! environment variable at run time, falling back to the value baked in at build time.
//! Without one, [`microsoft_login_available`] is `false` and sign-in reports
//! [`NOT_CONFIGURED`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aether_auth::device_code::{
    DeviceCodeAuthorization, DeviceCodeEndpoints, DeviceCodeError, DeviceCodePoll,
    DeviceCodeRequest, PollAction, PollOutcome, TokenPollRequest, parse_token_response,
};
use aether_auth::http::{HttpError, HttpMethod, HttpRequest, HttpResponse, HttpTransport};
use aether_auth::minecraft::MinecraftError;
use aether_auth::secure_store::KeyringCredentialStore;
use aether_auth::signin::{SignInError, refresh_microsoft_tokens, sign_in_to_minecraft};
use aether_auth::store::{CredentialStore, FileCredentialStore, StoredCredential};
use aether_auth::token::{MicrosoftTokens, MinecraftToken};
use aether_auth::xbox::XboxError;
use aether_auth::{AccountId, AccountKind};
use serde::Serialize;
use tauri::{Emitter, Manager};

/// Error text when no Azure application id is configured; the frontend matches on it.
pub const NOT_CONFIGURED: &str = "microsoft-not-configured";

/// Error text when the stored sign-in is no longer valid; the frontend matches on it.
pub const SIGN_IN_REQUIRED: &str = "microsoft-sign-in-required";

/// Error text when the account does not own Java Edition.
pub const NOT_ENTITLED: &str = "microsoft-not-entitled";

/// Event name carrying [`LoginEvent`] payloads.
pub const STATUS_EVENT: &str = "login://status";

/// Environment variable that overrides the built-in client id.
pub const CLIENT_ID_VARIABLE: &str = "AETHER_MS_CLIENT_ID";

const BUILT_IN_CLIENT_ID: Option<&str> = option_env!("AETHER_MS_CLIENT_ID");

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
/// Consecutive network failures tolerated while polling before giving up.
const MAX_POLL_FAILURES: u32 = 5;
const WAIT_SLICE: Duration = Duration::from_millis(200);

// --------------------------------------------------------------------------------------
// Transport
// --------------------------------------------------------------------------------------

/// The production [`HttpTransport`]: blocking `ureq`, no redirects, bounded bodies.
///
/// Errors carry only the host (see [`HttpError::transport`]), never the URL or body.
pub struct UreqAuthTransport;

impl HttpTransport for UreqAuthTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let config = ureq::config::Config::builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(concat!("AetherLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        let agent = ureq::Agent::new_with_config(config);
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
                call.send(request.body().unwrap_or("")).map_err(failed)?
            }
        };
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(failed)?;
        Ok(HttpResponse::new(status, body))
    }
}

// --------------------------------------------------------------------------------------
// Configuration
// --------------------------------------------------------------------------------------

/// `true` for an Azure application id: a hyphenated UUID.
fn is_client_id(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, length)| {
            group.len() == length && group.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

/// The configured Azure application id, if there is a valid one.
fn configured_client_id() -> Option<String> {
    let runtime = std::env::var(CLIENT_ID_VARIABLE).ok();
    runtime
        .as_deref()
        .or(BUILT_IN_CLIENT_ID)
        .map(str::trim)
        .filter(|value| is_client_id(value))
        .map(str::to_owned)
}

fn accounts_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|base| base.join("accounts"))
        .map_err(|_| "application data directory is unavailable".to_owned())
}

// --------------------------------------------------------------------------------------
// DTOs
// --------------------------------------------------------------------------------------

/// What the user needs to finish the sign-in in a browser.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeDto {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_secs: u64,
}

/// A signed-in Microsoft account. Identity only: no token of any kind.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftAccountDto {
    /// The launcher's account id (the Minecraft profile UUID, hyphenated).
    pub id: String,
    pub display_name: String,
    /// The Minecraft profile UUID, 32 lowercase hex digits.
    pub uuid: String,
    pub xuid: Option<String>,
}

impl MicrosoftAccountDto {
    fn from_credential(credential: &StoredCredential) -> Self {
        let uuid = credential.account_id.as_uuid();
        Self {
            id: uuid.to_string(),
            display_name: credential.display_name.clone(),
            uuid: uuid.simple().to_string(),
            xuid: credential.xuid.clone(),
        }
    }
}

/// Why a sign-in did not finish.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginFailure {
    /// Stable label: `expired`, `denied`, `network`, `service`, `storage`,
    /// `no-xbox-profile`, `child-account`, `region`, `not-entitled`, `no-profile`.
    pub reason: &'static str,
    /// A readable English sentence without secrets, for the fallback display.
    pub message: String,
}

/// One step of a sign-in, pushed as `login://status`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(
    tag = "state",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum LoginEvent {
    /// The user has the code; the launcher is polling.
    Waiting,
    SignedIn {
        account: MicrosoftAccountDto,
    },
    Failed {
        failure: LoginFailure,
    },
    Cancelled,
}

fn failure_of(error: &SignInError) -> LoginFailure {
    let reason = match error {
        SignInError::Http(_) => "network",
        SignInError::Xbox(XboxError::NoXboxProfile) => "no-xbox-profile",
        SignInError::Xbox(XboxError::ChildAccount) => "child-account",
        SignInError::Xbox(XboxError::RegionUnsupported) => "region",
        SignInError::Minecraft(MinecraftError::NotEntitled) => "not-entitled",
        SignInError::Minecraft(MinecraftError::ProfileNotFound) => "no-profile",
        _ => "service",
    };
    LoginFailure {
        reason,
        message: error.to_string(),
    }
}

// --------------------------------------------------------------------------------------
// The sign-in flow (pure given a transport, a store and a clock)
// --------------------------------------------------------------------------------------

/// How a poll for tokens ended without tokens.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PollEnd {
    Cancelled,
    Expired,
    Denied,
    Network,
    Failed(String),
}

/// Polls the token endpoint until the user finishes, declines, or the code expires.
///
/// `wait` sleeps for the given interval and returns `false` when the sign-in was
/// cancelled; it is a parameter so tests do not sleep.
pub(super) fn poll_for_tokens(
    transport: &dyn HttpTransport,
    endpoints: &DeviceCodeEndpoints,
    client_id: &str,
    challenge: &DeviceCodeAuthorization,
    wait: &dyn Fn(Duration) -> bool,
    now: &dyn Fn() -> SystemTime,
) -> Result<MicrosoftTokens, PollEnd> {
    let request =
        TokenPollRequest::new(client_id, challenge.device_code.clone()).to_http(endpoints);
    let mut poll = DeviceCodePoll::from_challenge(challenge);
    let mut failures = 0;
    loop {
        if !wait(poll.interval()) {
            return Err(PollEnd::Cancelled);
        }
        let response = match transport.execute(&request) {
            Ok(response) => response,
            Err(_) => {
                failures += 1;
                if failures >= MAX_POLL_FAILURES {
                    return Err(PollEnd::Network);
                }
                continue;
            }
        };
        failures = 0;
        let outcome = parse_token_response(response.status, &response.body, now())
            .map_err(|error| PollEnd::Failed(describe_oauth(&error)))?;
        if outcome == PollOutcome::Denied {
            return Err(PollEnd::Denied);
        }
        match poll.advance(outcome) {
            PollAction::Done(tokens) => return Ok(tokens),
            PollAction::Wait(_) => {}
            PollAction::Expired => return Err(PollEnd::Expired),
        }
    }
}

fn describe_oauth(error: &DeviceCodeError) -> String {
    error.to_string()
}

/// Runs the Xbox and Minecraft stages and stores the account.
///
/// The account id is the Minecraft profile UUID, so signing in again with the same
/// account replaces the earlier entry instead of duplicating it.
pub(super) fn finish_login(
    transport: &dyn HttpTransport,
    store: &dyn CredentialStore,
    tokens: &MicrosoftTokens,
    now: SystemTime,
) -> Result<MicrosoftAccountDto, LoginFailure> {
    let login = sign_in_to_minecraft(transport, tokens, now).map_err(|error| failure_of(&error))?;

    let mut credential = StoredCredential::metadata_only(
        AccountId::from_uuid(login.profile.id),
        login.profile.name.clone(),
        AccountKind::Microsoft,
    )
    .with_xuid(Some(login.xuid.clone()));
    if let Some(refresh) = tokens.refresh.clone() {
        credential = credential.with_refresh(refresh);
    }
    store.save(&credential).map_err(|_| LoginFailure {
        reason: "storage",
        message: "the account could not be saved to the system credential vault".to_owned(),
    })?;
    Ok(MicrosoftAccountDto::from_credential(&credential))
}

fn poll_failure(end: PollEnd) -> Option<LoginFailure> {
    let (reason, message) = match end {
        PollEnd::Cancelled => return None,
        PollEnd::Expired => ("expired", "the sign-in code expired".to_owned()),
        PollEnd::Denied => ("denied", "the sign-in was declined".to_owned()),
        PollEnd::Network => (
            "network",
            "the Microsoft sign-in service is unreachable".to_owned(),
        ),
        PollEnd::Failed(message) => ("service", message),
    };
    Some(LoginFailure { reason, message })
}

// --------------------------------------------------------------------------------------
// Launch credentials
// --------------------------------------------------------------------------------------

/// What the game is started with for a Microsoft account.
pub(super) struct LaunchCredentials {
    pub name: String,
    /// The Minecraft profile UUID, 32 lowercase hex digits.
    pub uuid: String,
    pub access_token: MinecraftToken,
    pub xuid: String,
}

fn launch_error(error: &SignInError) -> String {
    if matches!(error, SignInError::Minecraft(MinecraftError::NotEntitled)) {
        return NOT_ENTITLED.to_owned();
    }
    if error.requires_sign_in() {
        return SIGN_IN_REQUIRED.to_owned();
    }
    format!("Microsoft sign-in failed: {error}")
}

/// Refreshes the stored sign-in and returns the values a launch needs.
///
/// The refresh token is rotated on every use; the new one is saved before returning. A
/// failure to save it is not fatal for this launch (the old token usually stays valid),
/// so the game still starts.
pub(super) fn credentials_for_launch(
    transport: &dyn HttpTransport,
    store: &dyn CredentialStore,
    endpoints: &DeviceCodeEndpoints,
    client_id: &str,
    account_id: &str,
    now: SystemTime,
) -> Result<LaunchCredentials, String> {
    let id = AccountId::parse(account_id).ok_or_else(|| "invalid account ID".to_owned())?;
    let stored = store
        .load(id)
        .map_err(|_| "the account could not be read from the system credential vault".to_owned())?
        .filter(|credential| credential.kind == AccountKind::Microsoft)
        .ok_or_else(|| "this Microsoft account is not signed in on this computer".to_owned())?;
    let refresh = stored
        .refresh
        .clone()
        .ok_or_else(|| SIGN_IN_REQUIRED.to_owned())?;

    let tokens = refresh_microsoft_tokens(transport, endpoints, client_id, &refresh, now)
        .map_err(|error| launch_error(&error))?;
    let login =
        sign_in_to_minecraft(transport, &tokens, now).map_err(|error| launch_error(&error))?;

    let mut updated = StoredCredential::metadata_only(
        stored.account_id,
        login.profile.name.clone(),
        AccountKind::Microsoft,
    )
    .with_xuid(Some(login.xuid.clone()));
    updated = updated.with_refresh(tokens.refresh.clone().unwrap_or(refresh));
    let _ = store.save(&updated);

    Ok(LaunchCredentials {
        uuid: login.profile.undashed_id(),
        name: login.profile.name,
        access_token: login.session.access,
        xuid: login.xuid,
    })
}

/// Loads launch credentials with the production transport, vault and clock.
pub(super) fn launch_credentials_for(
    app: &tauri::AppHandle,
    account_id: &str,
) -> Result<LaunchCredentials, String> {
    let client_id = configured_client_id().ok_or_else(|| NOT_CONFIGURED.to_owned())?;
    let store = KeyringCredentialStore::new(accounts_root(app)?);
    credentials_for_launch(
        &UreqAuthTransport,
        &store,
        &DeviceCodeEndpoints::microsoft(),
        &client_id,
        account_id,
        SystemTime::now(),
    )
}

// --------------------------------------------------------------------------------------
// Commands
// --------------------------------------------------------------------------------------

/// One sign-in attempt, shared between the command and its worker thread.
struct Session {
    cancel: AtomicBool,
    done: AtomicBool,
    /// The verification page the backend itself received from Microsoft; the only URL
    /// [`open_microsoft_verification_page`] will open.
    page: Mutex<Option<String>>,
}

/// At most one sign-in runs at a time.
#[derive(Default)]
pub struct MicrosoftLoginState {
    current: Mutex<Option<Arc<Session>>>,
}

impl MicrosoftLoginState {
    fn start(&self) -> Result<Arc<Session>, String> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| "sign-in state is unavailable".to_owned())?;
        if current
            .as_ref()
            .is_some_and(|session| !session.done.load(Ordering::SeqCst))
        {
            return Err("a Microsoft sign-in is already in progress".to_owned());
        }
        let session = Arc::new(Session {
            cancel: AtomicBool::new(false),
            done: AtomicBool::new(false),
            page: Mutex::new(None),
        });
        *current = Some(Arc::clone(&session));
        Ok(session)
    }

    fn cancel(&self) -> bool {
        let Ok(current) = self.current.lock() else {
            return false;
        };
        match current.as_ref() {
            Some(session) if !session.done.load(Ordering::SeqCst) => {
                session.cancel.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }
}

/// Whether a sign-in can be started at all (an Azure application id is configured).
#[tauri::command]
pub fn microsoft_login_available() -> bool {
    configured_client_id().is_some()
}

/// Requests a device code and starts polling in the background.
///
/// Returns the code to show the user; the outcome arrives as `login://status` events.
#[tauri::command]
pub async fn begin_microsoft_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, MicrosoftLoginState>,
) -> Result<DeviceCodeDto, String> {
    let client_id = configured_client_id().ok_or_else(|| NOT_CONFIGURED.to_owned())?;
    let root = accounts_root(&app)?;
    let session = state.start()?;

    let requested = {
        let client_id = client_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            request_device_code(&UreqAuthTransport, &client_id)
        })
        .await
        .map_err(|_| "sign-in task failed".to_owned())
        .and_then(|result| result)
    };
    let challenge = match requested {
        Ok(challenge) => challenge,
        Err(message) => {
            session.done.store(true, Ordering::SeqCst);
            return Err(message);
        }
    };

    if let Ok(mut page) = session.page.lock() {
        *page = Some(challenge.verification_uri.clone());
    }
    let dto = DeviceCodeDto {
        user_code: challenge.user_code.clone(),
        verification_uri: challenge.verification_uri.clone(),
        expires_in_secs: challenge.expires_in.as_secs(),
    };

    let worker_session = Arc::clone(&session);
    let spawned = std::thread::Builder::new()
        .name("microsoft-login".to_owned())
        .spawn(move || {
            let _ = app.emit(STATUS_EVENT, &LoginEvent::Waiting);
            let event = run_login(&client_id, &root, &challenge, &worker_session);
            worker_session.done.store(true, Ordering::SeqCst);
            let _ = app.emit(STATUS_EVENT, &event);
        });
    if spawned.is_err() {
        session.done.store(true, Ordering::SeqCst);
        return Err("cannot start the sign-in worker".to_owned());
    }
    Ok(dto)
}

fn request_device_code(
    transport: &dyn HttpTransport,
    client_id: &str,
) -> Result<DeviceCodeAuthorization, String> {
    let request = DeviceCodeRequest::new(client_id).to_http(&DeviceCodeEndpoints::microsoft());
    let response = transport
        .execute(&request)
        .map_err(|error| error.to_string())?;
    if !response.is_success() {
        // A rejected client id looks like a 4xx here; say so without echoing the body.
        return Err(format!(
            "the Microsoft sign-in service refused the request (HTTP {}); check the application registration",
            response.status
        ));
    }
    DeviceCodeAuthorization::from_json(&response.body).map_err(|error| error.to_string())
}

/// The worker: polls, finishes the chain and reports one terminal event.
fn run_login(
    client_id: &str,
    root: &std::path::Path,
    challenge: &DeviceCodeAuthorization,
    session: &Session,
) -> LoginEvent {
    let wait = |interval: Duration| {
        let mut left = interval;
        while !left.is_zero() {
            if session.cancel.load(Ordering::SeqCst) {
                return false;
            }
            let slice = left.min(WAIT_SLICE);
            std::thread::sleep(slice);
            left -= slice;
        }
        !session.cancel.load(Ordering::SeqCst)
    };

    let transport = UreqAuthTransport;
    let tokens = match poll_for_tokens(
        &transport,
        &DeviceCodeEndpoints::microsoft(),
        client_id,
        challenge,
        &wait,
        &SystemTime::now,
    ) {
        Ok(tokens) => tokens,
        Err(end) => {
            return match poll_failure(end) {
                Some(failure) => LoginEvent::Failed { failure },
                None => LoginEvent::Cancelled,
            };
        }
    };

    let store = KeyringCredentialStore::new(root);
    match finish_login(&transport, &store, &tokens, SystemTime::now()) {
        Ok(account) => LoginEvent::SignedIn { account },
        Err(failure) => LoginEvent::Failed { failure },
    }
}

/// Hosts the sign-in page may live on. Anything else the service might send is refused
/// rather than handed to the operating system.
const TRUSTED_PAGE_HOSTS: [&str; 5] = [
    "microsoft.com",
    "www.microsoft.com",
    "login.microsoftonline.com",
    "login.live.com",
    "aka.ms",
];

/// `true` for an `https` URL on a Microsoft sign-in host, with no credentials in it.
pub(super) fn is_trusted_verification_uri(uri: &str) -> bool {
    let Some(rest) = uri.strip_prefix("https://") else {
        return false;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    !authority.contains('@')
        && !authority.contains(':')
        && TRUSTED_PAGE_HOSTS.contains(&authority.to_ascii_lowercase().as_str())
        && !uri.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Opens the sign-in page of the running sign-in in the user's browser.
///
/// The frontend passes nothing: the URL is the one Microsoft returned for the current
/// attempt, and it is opened only if it is `https` on a Microsoft host.
#[tauri::command]
pub fn open_microsoft_verification_page(
    state: tauri::State<'_, MicrosoftLoginState>,
) -> Result<(), String> {
    let uri = {
        let current = state
            .current
            .lock()
            .map_err(|_| "sign-in state is unavailable".to_owned())?;
        let session = current
            .as_ref()
            .filter(|session| !session.done.load(Ordering::SeqCst))
            .ok_or_else(|| "no Microsoft sign-in is in progress".to_owned())?;
        let page = session
            .page
            .lock()
            .map_err(|_| "sign-in state is unavailable".to_owned())?;
        page.clone()
            .ok_or_else(|| "no Microsoft sign-in is in progress".to_owned())?
    };
    if !is_trusted_verification_uri(&uri) {
        return Err("the sign-in page is not on a Microsoft host".to_owned());
    }
    open_in_browser(&uri)
}

/// Hands `uri` to the platform's default-browser opener. The URL is one argument, never
/// interpreted by a shell.
fn open_in_browser(uri: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", uri]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(uri);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(uri);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "cannot open the default browser".to_owned())
}

/// Stops the running sign-in, if any. Returns whether one was running.
#[tauri::command]
pub fn cancel_microsoft_login(state: tauri::State<'_, MicrosoftLoginState>) -> bool {
    state.cancel()
}

/// The Microsoft accounts signed in on this computer. Reads metadata only, so the
/// system credential vault is not touched.
#[tauri::command]
pub async fn list_microsoft_accounts(
    app: tauri::AppHandle,
) -> Result<Vec<MicrosoftAccountDto>, String> {
    let root = accounts_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || list_accounts(&FileCredentialStore::new(root)))
        .await
        .map_err(|_| "account listing task failed".to_owned())?
}

pub(super) fn list_accounts(
    store: &dyn CredentialStore,
) -> Result<Vec<MicrosoftAccountDto>, String> {
    let ids = store
        .list()
        .map_err(|_| "the account list could not be read".to_owned())?;
    let mut accounts: Vec<MicrosoftAccountDto> = ids
        .into_iter()
        .filter_map(|id| store.load(id).ok().flatten())
        .filter(|credential| credential.kind == AccountKind::Microsoft)
        .map(|credential| MicrosoftAccountDto::from_credential(&credential))
        .collect();
    accounts.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    Ok(accounts)
}

/// Signs an account out: deletes its refresh token from the vault and its metadata.
#[tauri::command]
pub async fn remove_microsoft_account(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let root = accounts_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let id = AccountId::parse(&id).ok_or_else(|| "invalid account ID".to_owned())?;
        KeyringCredentialStore::new(root).remove(id).map_err(|_| {
            "the account could not be removed from the system credential vault".to_owned()
        })
    })
    .await
    .map_err(|_| "account removal task failed".to_owned())?
}

#[cfg(test)]
#[path = "microsoft_tests.rs"]
mod tests;
