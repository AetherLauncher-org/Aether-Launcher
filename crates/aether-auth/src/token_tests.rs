use std::time::{Duration, SystemTime};

use super::{
    AccessToken, MicrosoftTokens, MinecraftSession, MinecraftToken, RefreshToken, TokenError,
    TokenLifetime, XstsToken,
};

const SECRET: &str = "eyJhbGciOiJSUzI1NiJ9.payload.signature-9876";

fn epoch() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

#[test]
fn debug_and_display_reveal_only_the_last_four_characters() {
    let token = AccessToken::new(SECRET);

    assert_eq!(format!("{token:?}"), "AccessToken(****9876)");
    assert_eq!(format!("{token}"), "AccessToken(****9876)");
    assert!(!format!("{token:?}").contains(SECRET));
    assert!(!format!("{token}").contains("payload"));
}

#[test]
fn short_secrets_are_fully_hidden() {
    let token = RefreshToken::new("short1234");

    assert_eq!(format!("{token:?}"), "RefreshToken(****)");
    assert!(!format!("{token:?}").contains("1234"));
}

#[test]
fn debug_of_a_container_does_not_leak_any_token() {
    let tokens = MicrosoftTokens::new(
        AccessToken::new(SECRET),
        Some(RefreshToken::new("M.R3_BAY.refresh-value-1234")),
        TokenLifetime::new(epoch(), Duration::from_secs(3600)),
    );

    let rendered = format!("{tokens:?}");

    assert!(rendered.contains("AccessToken(****9876)"));
    assert!(rendered.contains("RefreshToken(****1234)"));
    assert!(!rendered.contains("payload"));
    assert!(!rendered.contains("M.R3_BAY"));
    assert!(!rendered.contains("signature"));
}

#[test]
fn xsts_and_minecraft_tokens_are_masked_too() {
    let xsts = XstsToken::new("xsts-token-value-4321");
    let minecraft = MinecraftToken::new("minecraft-token-value-8765");

    assert_eq!(format!("{xsts:?}"), "XstsToken(****4321)");
    assert_eq!(format!("{minecraft:?}"), "MinecraftToken(****8765)");
    assert!(!format!("{xsts:?}").contains("xsts-token-value"));
    assert!(!format!("{minecraft:?}").contains("minecraft-token-value"));
}

#[test]
fn expose_returns_the_raw_value() {
    assert_eq!(AccessToken::new(SECRET).expose(), SECRET);
}

#[test]
fn empty_access_token_is_rejected() {
    let error = AccessToken::parse("").unwrap_err();

    assert!(matches!(error, TokenError::EmptySecret("access token")));
    assert_eq!(error.to_string(), "the access token is empty");
}

#[test]
fn empty_refresh_token_is_rejected_but_absent_one_is_fine() {
    assert!(RefreshToken::parse("").is_err());
    assert_eq!(RefreshToken::from_optional(None), None);
    assert_eq!(RefreshToken::from_optional(Some(String::new())), None);
    assert_eq!(
        RefreshToken::from_optional(Some("value".to_owned())),
        Some(RefreshToken::new("value"))
    );
}

#[test]
fn lifetime_reports_expiry_with_skew() {
    let lifetime = TokenLifetime::new(epoch(), Duration::from_secs(3600));

    assert_eq!(lifetime.expires_at(), epoch() + Duration::from_secs(3600));
    assert_eq!(lifetime.expires_in(), Duration::from_secs(3600));
    assert!(!lifetime.is_expired_at(epoch() + Duration::from_secs(3000), Duration::ZERO));
    assert!(lifetime.is_expired_at(epoch() + Duration::from_secs(3600), Duration::ZERO));
}

#[test]
fn skew_expires_a_token_early() {
    let lifetime = TokenLifetime::new(epoch(), Duration::from_secs(3600));
    let five_minutes_early = epoch() + Duration::from_secs(3300);

    assert!(!lifetime.is_expired_at(five_minutes_early, Duration::ZERO));
    assert!(lifetime.is_expired_at(five_minutes_early, Duration::from_secs(300)));
}

#[test]
fn a_refresh_token_keeps_the_session_alive_after_the_access_token_expires() {
    let expired = TokenLifetime::new(epoch(), Duration::from_secs(60));
    let now = epoch() + Duration::from_secs(600);

    let refreshable = MicrosoftTokens::new(
        AccessToken::new(SECRET),
        Some(RefreshToken::new("refresh-1234")),
        expired.clone(),
    );
    let not_refreshable = MicrosoftTokens::new(AccessToken::new(SECRET), None, expired);

    assert!(!refreshable.requires_sign_in_at(now, Duration::ZERO));
    assert!(not_refreshable.requires_sign_in_at(now, Duration::ZERO));
}

#[test]
fn minecraft_session_debug_is_redacted() {
    let session = MinecraftSession::new(
        MinecraftToken::new("mc-access-token-9999"),
        TokenLifetime::new(epoch(), Duration::from_secs(86_400)),
        Some("Steve".to_owned()),
    );

    let rendered = format!("{session:?}");

    assert!(rendered.contains("MinecraftToken(****9999)"));
    assert!(rendered.contains("Steve"));
    assert!(!rendered.contains("mc-access-token"));
}
