//! Thin Tauri boundary; passwords are input-only and never instrumented.
use super::host::run;
use aether_app::yggdrasil::{self, Account, Challenge};

#[tauri::command]
pub async fn begin_yggdrasil_login(
    app: tauri::AppHandle,
    server: String,
    username: String,
    password: String,
) -> Result<Challenge, String> {
    run(app, move |host| {
        yggdrasil::begin(host, &server, &username, &password)
    })
    .await
}
#[tauri::command]
pub async fn finish_yggdrasil_login(
    app: tauri::AppHandle,
    id: String,
    profile_id: String,
) -> Result<Account, String> {
    run(app, move |host| yggdrasil::finish(host, &id, &profile_id)).await
}
#[tauri::command]
pub async fn cancel_yggdrasil_login(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |_| yggdrasil::cancel(&id)).await
}
#[tauri::command]
pub async fn list_yggdrasil_accounts(app: tauri::AppHandle) -> Result<Vec<Account>, String> {
    run(app, |host| yggdrasil::accounts(host)).await
}
#[tauri::command]
pub async fn refresh_yggdrasil_account(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |host| yggdrasil::refresh(host, &id)).await
}
#[tauri::command]
pub async fn remove_yggdrasil_account(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |host| yggdrasil::remove(host, &id)).await
}
