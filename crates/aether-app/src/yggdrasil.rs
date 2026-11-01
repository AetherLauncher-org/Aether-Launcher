//! Third-party accounts. Only identity crosses the output boundary.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use aether_auth::http::{HttpRequest, HttpResponse, HttpTransport};
use aether_auth::secure_store::KeyringCredentialStore;
use aether_auth::store::{CredentialStore, FileCredentialStore, StoredCredential};
use aether_auth::yggdrasil::{self as protocol, Profile, Server, Session, Tokens};
use aether_auth::{AccountId, AccountKind};
use serde::Serialize;

use crate::Host;
#[path = "yggdrasil_transport.rs"]
mod transport;
use transport::PublicTransport;

// Serialize token rotation and removal; retain at most one profile selection.
static GATE: Mutex<Option<Pending>> = Mutex::new(None);
const STORAGE: &str = "yggdrasil-storage-error";
const LOGIN_TTL: Duration = Duration::from_secs(300);
struct Pending {
    id: AccountId,
    server: Server,
    session: Session,
    expires: Instant,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    pub id: String,
    pub profiles: Vec<Profile>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub server: String,
    pub profile: Profile,
}
fn root(host: &dyn Host) -> Result<std::path::PathBuf, String> {
    host.data_dir().map(|p| p.join("yggdrasil-accounts")).map_err(|_| STORAGE.to_owned())
}
fn execute(transport: &dyn HttpTransport, request: &HttpRequest) -> Result<HttpResponse, String> {
    transport.execute(request).map_err(|_| "yggdrasil-network-error".to_owned())
}
fn identity(value: &StoredCredential) -> Option<Account> {
    let AccountKind::Yggdrasil { server, profile_id } = &value.kind else { return None; };
    let profile = Profile { id: profile_id.clone(), name: value.display_name.clone() };
    if !profile.valid() || Server::parse(server).ok()?.root() != server { return None; }
    Some(Account { id: value.account_id.as_uuid().to_string(), server: server.clone(), profile })
}
fn list_with(store: &dyn CredentialStore) -> Result<Vec<Account>, String> {
    let mut accounts = Vec::new();
    for id in store.list().map_err(|_| STORAGE)? {
        if let Some(account) = store.load(id).map_err(|_| STORAGE)?.as_ref().and_then(identity) {
            accounts.push(account);
        }
    }
    accounts.sort_by(|a, b| a.profile.name.cmp(&b.profile.name).then(a.id.cmp(&b.id)));
    Ok(accounts)
}
/// Reads metadata only, without unlocking the OS vault.
pub fn accounts(host: &dyn Host) -> Result<Vec<Account>, String> {
    list_with(&FileCredentialStore::new(root(host)?))
}
fn begin_with(pending: &mut Option<Pending>, transport: &dyn HttpTransport, server: &str, username: &str, password: &str, now: Instant) -> Result<Challenge, String> {
    if pending.as_ref().is_some_and(|p| p.expires > now) { return Err("yggdrasil-busy".to_owned()); }
    if let Some(old) = pending.take() {
        let _ = execute(transport, &protocol::build_invalidate(&old.server, &old.session.tokens));
    }
    let server = Server::parse(server)?;
    let client = AccountId::new().as_uuid().simple().to_string();
    // Validate the inputs before performing even the metadata request.
    let request = protocol::build_authenticate(&server, username, password, &client)?;
    protocol::parse_metadata(&execute(transport, &HttpRequest::get(server.root()))?)?;
    let session = protocol::parse_session(&execute(transport, &request)?, Some(&client))?;
    let profiles = if let Some(profile) = &session.selected { vec![profile.clone()] } else { session.profiles.clone() };
    if profiles.is_empty() {
        let _ = execute(transport, &protocol::build_invalidate(&server, &session.tokens));
        return Err("yggdrasil-no-profile".to_owned());
    }
    let id = AccountId::new();
    *pending = Some(Pending { id, server, session, expires: now + LOGIN_TTL });
    Ok(Challenge { id: id.as_uuid().to_string(), profiles })
}
/// Password is used only to construct the authentication request, never saved.
pub fn begin(host: &dyn Host, server: &str, username: &str, password: &str) -> Result<Challenge, String> {
    let _ = root(host)?;
    let mut pending = GATE.lock().map_err(|_| "yggdrasil-busy")?;
    begin_with(&mut pending, &PublicTransport, server, username, password, Instant::now())
}
fn bind(transport: &dyn HttpTransport, server: &Server, session: &Session, profile_id: &str) -> Result<(Tokens, Profile), String> {
    if let Some(profile) = &session.selected {
        if profile.id != profile_id { return Err("yggdrasil-invalid-profile".to_owned()); }
        return Ok((session.tokens.clone(), profile.clone()));
    }
    let profile = session.profiles.iter().find(|p| p.id == profile_id).ok_or("yggdrasil-invalid-profile")?;
    let response = execute(transport, &protocol::build_refresh(server, &session.tokens, Some(profile)))?;
    let fresh = protocol::parse_session(&response, Some(session.tokens.client()))?;
    let bound = fresh.selected.filter(|p| p.id == profile.id).ok_or("yggdrasil-invalid-profile")?;
    Ok((fresh.tokens, bound))
}
fn finish_with(pending: &mut Option<Pending>, store: &dyn CredentialStore, transport: &dyn HttpTransport, id: &str, profile_id: &str, now: Instant) -> Result<Account, String> {
    if pending.as_ref().is_some_and(|p| p.expires <= now) {
        if let Some(old) = pending.take() {
            let _ = execute(transport, &protocol::build_invalidate(&old.server, &old.session.tokens));
        }
        return Err("yggdrasil-login-expired".to_owned());
    }
    let entry = pending.as_mut().filter(|p| p.id.as_uuid().to_string() == id).ok_or("yggdrasil-login-expired")?;
    let (tokens, profile) = bind(transport, &entry.server, &entry.session, profile_id)?;
    // Keep the new token even if metadata lookup or the vault write fails.
    entry.session.tokens = tokens;
    entry.session.selected = Some(profile.clone());
    let existing = list_with(store)?.into_iter().find(|a| a.server == entry.server.root() && a.profile.id == profile.id);
    let account_id = existing.and_then(|a| AccountId::parse(&a.id)).unwrap_or(entry.id);
    let credential = StoredCredential::metadata_only(account_id, &profile.name, AccountKind::Yggdrasil { server: entry.server.root().to_owned(), profile_id: profile.id }).with_refresh(entry.session.tokens.to_vault());
    store.save(&credential).map_err(|_| STORAGE)?;
    *pending = None;
    identity(&credential).ok_or_else(|| "yggdrasil-invalid-profile".to_owned())
}
pub fn finish(host: &dyn Host, id: &str, profile_id: &str) -> Result<Account, String> {
    let mut pending = GATE.lock().map_err(|_| "yggdrasil-busy")?;
    finish_with(&mut pending, &KeyringCredentialStore::new(root(host)?), &PublicTransport, id, profile_id, Instant::now())
}
fn cancel_with(pending: &mut Option<Pending>, transport: &dyn HttpTransport, id: &str) {
    if pending.as_ref().is_some_and(|p| p.id.as_uuid().to_string() == id)
        && let Some(p) = pending.take()
    {
        let _ = execute(transport, &protocol::build_invalidate(&p.server, &p.session.tokens));
    }
}
pub fn cancel(id: &str) -> Result<(), String> {
    let mut pending = GATE.lock().map_err(|_| "yggdrasil-busy")?;
    cancel_with(&mut pending, &PublicTransport, id);
    Ok(())
}
fn refresh_with(store: &dyn CredentialStore, transport: &dyn HttpTransport, id: AccountId) -> Result<(), String> {
    let mut value = store.load(id).map_err(|_| STORAGE)?.ok_or("yggdrasil-sign-in-required")?;
    let account = identity(&value).ok_or("yggdrasil-sign-in-required")?;
    let server = Server::parse(&account.server)?;
    let tokens = Tokens::from_vault(value.refresh.as_ref().ok_or("yggdrasil-sign-in-required")?)?;
    let response = execute(transport, &protocol::build_refresh(&server, &tokens, None))?;
    let fresh = protocol::parse_session(&response, Some(tokens.client()))?;
    let profile = fresh.selected.filter(|p| p.id == account.profile.id).ok_or("yggdrasil-invalid-profile")?;
    value.display_name = profile.name;
    value.refresh = Some(fresh.tokens.to_vault());
    store.save(&value).map_err(|_| STORAGE.to_owned())
}
pub fn refresh(host: &dyn Host, id: &str) -> Result<(), String> {
    let _guard = GATE.lock().map_err(|_| "yggdrasil-busy")?;
    let id = AccountId::parse(id).ok_or("yggdrasil-invalid-account")?;
    refresh_with(&KeyringCredentialStore::new(root(host)?), &PublicTransport, id)
}
fn remove_with(store: &dyn CredentialStore, transport: &dyn HttpTransport, id: AccountId) -> Result<(), String> {
    // Revocation is best effort: an offline server must not prevent local sign-out.
    if let Ok(Some(value)) = store.load(id)
        && let Some(account) = identity(&value)
        && let Some(secret) = value.refresh
        && let (Ok(server), Ok(tokens)) = (Server::parse(&account.server), Tokens::from_vault(&secret))
    {
        let _ = execute(transport, &protocol::build_invalidate(&server, &tokens));
    }
    store.remove(id).map_err(|_| STORAGE.to_owned())
}
pub fn remove(host: &dyn Host, id: &str) -> Result<(), String> {
    let _guard = GATE.lock().map_err(|_| "yggdrasil-busy")?;
    let id = AccountId::parse(id).ok_or("yggdrasil-invalid-account")?;
    remove_with(&KeyringCredentialStore::new(root(host)?), &PublicTransport, id)
}

#[cfg(test)]
#[path = "yggdrasil_tests.rs"]
mod tests;
