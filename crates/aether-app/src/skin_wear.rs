//! Wearing a skin: setting the skin of a signed-in Microsoft account to one from the library.
//!
//! The front end names a skin (its id) and an account (its id). Everything else stays in the
//! backend: the picture is read from the library and checked again, the account's sign-in is
//! refreshed to get a short-lived Minecraft token, and the token goes only into the upload's
//! `Authorization` header. Nothing but a short message comes back. Offline accounts have no
//! skin service to talk to, so only Microsoft accounts can wear a skin.

use aether_auth::http::HttpTransport;
use aether_auth::minecraft::{MinecraftError, build_skin_upload_request, parse_skin_upload};
use aether_auth::token::MinecraftToken;

use crate::host::Host;
use crate::microsoft::{self, UreqAuthTransport};
use crate::skin_library;

/// Uploads one picture as the skin of the account the token belongs to.
pub(crate) fn upload(
    transport: &dyn HttpTransport,
    token: &MinecraftToken,
    slim: bool,
    png: &[u8],
) -> Result<(), String> {
    let request = build_skin_upload_request(token, slim, png);
    let response = transport
        .execute(&request)
        .map_err(|_| "could not reach Minecraft services; check the network".to_owned())?;
    parse_skin_upload(response.status).map_err(|error| match error {
        MinecraftError::Unauthorized => {
            "Minecraft services refused this account; sign in to it again".to_owned()
        }
        MinecraftError::UnexpectedStatus { status: 400 } => {
            "Minecraft services did not accept this skin".to_owned()
        }
        MinecraftError::UnexpectedStatus { status: 429 } => {
            "Minecraft services asks you to wait a little before changing the skin again".to_owned()
        }
        _ => "Minecraft services could not change the skin; try again later".to_owned(),
    })
}

/// Makes the skin from the library the skin of a Microsoft account.
///
/// # Errors
///
/// For a skin that is not in the library, an account that is not signed in (or must sign in
/// again), or a refusal or failure from Minecraft services. The messages carry no token.
pub fn wear(host: &dyn Host, skin_id: &str, account_id: &str) -> Result<(), String> {
    let (png, slim) = skin_library::picture(host, skin_id)?;
    let credentials = microsoft::launch_credentials_for(host, account_id)?;
    upload(&UreqAuthTransport, &credentials.access_token, slim, &png)
}

#[cfg(test)]
#[path = "skin_wear_tests.rs"]
mod tests;
