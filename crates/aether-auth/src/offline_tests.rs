//! Tests for the offline account module: name validation, the hand-written MD5 core
//! and the Java-compatible name-based UUID derivation.

use uuid::Uuid;
use uuid::Variant;

use super::{
    MAX_PLAYER_NAME_LEN, MIN_PLAYER_NAME_LEN, OfflineError, is_valid_player_name, md5,
    offline_account, offline_uuid, validate_player_name,
};
use crate::AccountKind;

/// Renders a 16-byte digest as lowercase hex, the form every MD5 reference uses.
fn hex(digest: &[u8; 16]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// RFC 1321 section A.5 test vectors.
const MD5_VECTORS: &[(&str, &str)] = &[
    ("", "d41d8cd98f00b204e9800998ecf8427e"),
    ("abc", "900150983cd24fb0d6963f7d28e17f72"),
    ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
    (
        "abcdefghijklmnopqrstuvwxyz",
        "c3fcd3d76192e4007dfb496cca67e13b",
    ),
    (
        "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
        "57edf4a22be3c955ac49da2e2107b67a",
    ),
    (
        "The quick brown fox jumps over the lazy dog",
        "9e107d9d372bb6826bd81d3542a419d6",
    ),
];

#[test]
fn md5_matches_rfc1321_vectors() {
    for (input, expected) in MD5_VECTORS {
        let actual = hex(&md5::digest(input.as_bytes()));
        assert_eq!(&actual, expected, "MD5 mismatch for input {input:?}");
    }
}

#[test]
fn md5_matches_the_lowercase_alphabet_vector() {
    // The seventh RFC 1321 vector is missing from the brief but is standard; it is
    // worth keeping because 62 bytes already spans a block boundary.
    let input = "abcdefghijklmnopqrstuvwxyz";
    assert_eq!(
        hex(&md5::digest(input.as_bytes())),
        "c3fcd3d76192e4007dfb496cca67e13b"
    );
}

#[test]
fn md5_handles_inputs_across_block_boundaries() {
    // 55/56/57 bytes straddle the point where the length field needs a second block,
    // 64 bytes is exactly one block and 1000 bytes spans many. Every expected value
    // below comes from an independent implementation (.NET MD5), not from this code.
    let cases = [
        ("a".repeat(55), "ef1772b6dff9a122358552954ad0df65"),
        ("a".repeat(56), "3b0c8ac703f828b04c6c197006d17218"),
        ("a".repeat(57), "652b906d60af96844ebd21b674f35e93"),
        ("a".repeat(64), "014842d480b571495a4a0363793f7367"),
        ("a".repeat(1000), "cabe45dcc9ae5b66ba86600cca6b8ba8"),
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".to_owned(),
            "d174ab98d277d9f5a5611c2c9f419d9f",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            hex(&md5::digest(input.as_bytes())),
            expected,
            "MD5 mismatch for a {}-byte input",
            input.len()
        );
    }
}

#[test]
fn md5_is_deterministic_and_outputs_sixteen_bytes() {
    let first = md5::digest(b"Notch");
    assert_eq!(first, md5::digest(b"Notch"));
    assert_eq!(first.len(), 16);
    assert_ne!(md5::digest(b"Notch"), md5::digest(b"notch"));
}

#[test]
fn player_name_limits_are_the_documented_values() {
    assert_eq!(MIN_PLAYER_NAME_LEN, 3);
    assert_eq!(MAX_PLAYER_NAME_LEN, 16);
}

#[test]
fn short_names_are_rejected() {
    for name in ["", "a", "ab"] {
        assert_eq!(
            validate_player_name(name),
            Err(OfflineError::NameLength),
            "expected {name:?} to be too short"
        );
    }
}

#[test]
fn long_names_are_rejected() {
    let just_too_long = "a".repeat(MAX_PLAYER_NAME_LEN + 1);
    assert_eq!(
        validate_player_name(&just_too_long),
        Err(OfflineError::NameLength)
    );
    let much_too_long = "a".repeat(64);
    assert_eq!(
        validate_player_name(&much_too_long),
        Err(OfflineError::NameLength)
    );
}

#[test]
fn length_boundaries_are_accepted() {
    let shortest = "a".repeat(MIN_PLAYER_NAME_LEN);
    let longest = "a".repeat(MAX_PLAYER_NAME_LEN);
    assert_eq!(validate_player_name(&shortest), Ok(()));
    assert_eq!(validate_player_name(&longest), Ok(()));
    assert!(is_valid_player_name(&shortest));
    assert!(is_valid_player_name(&longest));
    assert!(!is_valid_player_name(&"a".repeat(MAX_PLAYER_NAME_LEN + 1)));
}

#[test]
fn names_with_spaces_or_punctuation_are_rejected() {
    for name in ["with space", "has-dash", "has.dot", "a b", "!!abc"] {
        assert_eq!(
            validate_player_name(name),
            Err(OfflineError::NameCharacters),
            "expected {name:?} to be rejected for its characters"
        );
    }
}

#[test]
fn non_ascii_names_are_rejected() {
    for name in ["史蒂夫", "玩家abc", "café", "naïve"] {
        assert_eq!(
            validate_player_name(name),
            Err(OfflineError::NameCharacters),
            "expected {name:?} to be rejected as non-ASCII"
        );
    }
}

#[test]
fn emoji_names_are_rejected() {
    // The first two names are long enough to pass the length check by character count,
    // so they fail on the alphabet; the single emoji fails on length instead.
    assert_eq!(
        validate_player_name("Steve🎮"),
        Err(OfflineError::NameCharacters)
    );
    assert_eq!(
        validate_player_name("🎮🎮🎮"),
        Err(OfflineError::NameCharacters)
    );
    assert_eq!(validate_player_name("🎮"), Err(OfflineError::NameLength));
    assert!(!is_valid_player_name("🎮🎮🎮"));
}

#[test]
fn ordinary_names_are_accepted() {
    for name in ["abc", "Steve_123", "_x_", "a_1", "Notch"] {
        assert_eq!(
            validate_player_name(name),
            Ok(()),
            "expected {name:?} to be accepted"
        );
    }
    assert!(is_valid_player_name("Steve_123"));
    assert!(is_valid_player_name("_x_"));
}

#[test]
fn validation_reports_the_documented_messages() {
    assert_eq!(
        OfflineError::NameLength.to_string(),
        "the player name must be 3 to 16 characters long"
    );
    assert_eq!(
        OfflineError::NameCharacters.to_string(),
        "the player name may only contain ASCII letters, digits and underscores"
    );
}

#[test]
fn validation_messages_never_echo_the_rejected_input() {
    // The messages are constant strings, so a hostile name or a local path cannot leak
    // into a log line through the error.
    let secret = "C:/Users/someone/AppData/secret-token";
    let err = validate_player_name(secret).expect_err("invalid name");
    let text = err.to_string();
    assert!(!text.contains(secret));
    assert!(!text.contains("AppData"));
    assert!(!text.contains("OfflinePlayer"));

    let long = "x".repeat(40);
    let err = validate_player_name(&long).expect_err("invalid name");
    assert!(!err.to_string().contains(&long));
}

#[test]
fn offline_uuid_matches_the_notch_reference_value() {
    // The canonical example of the derivation: MD5("OfflinePlayer:Notch") with version
    // and variant patched in.
    let uuid = offline_uuid("Notch").expect("Notch is a valid name");
    assert_eq!(
        uuid.to_string(),
        "b50ad385-829d-3141-a216-7e7d7539ba7f",
        "offline UUID mismatch for Notch"
    );
}

#[test]
fn offline_uuid_matches_the_derived_references() {
    // These two are NOT the players' well-known Mojang (online) UUIDs: those are
    // version 4 and can never come out of `nameUUIDFromBytes`, which forces version 3.
    // The expected values below come from an independent derivation of
    // `UUID.nameUUIDFromBytes("OfflinePlayer:<name>")`, confirmed with .NET's MD5.
    let expected = [
        ("jeb_", "a762f560-4fce-3236-812a-b80efff0b62b"),
        ("Dinnerbone", "4d258a81-2358-3084-8166-05b9faccad80"),
        ("Steve", "5627dd98-e6be-3c21-b8a8-e92344183641"),
        ("Steve_123", "77cae0ff-71ba-39ba-b80c-74428607dfcc"),
        ("_x_", "fa7246b5-41db-3b84-837a-3c43e5cca127"),
    ];
    for (name, reference) in expected {
        let derived = offline_uuid(name).expect("valid name");
        assert_eq!(
            derived.to_string(),
            reference,
            "offline UUID mismatch for {name:?}"
        );
        assert_eq!(
            derived,
            Uuid::parse_str(reference).expect("reference value parses")
        );
    }
}

#[test]
fn offline_uuid_sets_version_three_and_the_rfc4122_variant() {
    for name in ["Notch", "jeb_", "Dinnerbone", "Steve_123", "_x_"] {
        let uuid = offline_uuid(name).expect("valid name");
        assert_eq!(uuid.get_version_num(), 3, "wrong version for {name:?}");
        assert_eq!(
            uuid.get_variant(),
            Variant::RFC4122,
            "wrong variant for {name:?}"
        );
    }
}

#[test]
fn offline_uuid_is_case_sensitive() {
    let upper = offline_uuid("Steve").expect("valid name");
    let lower = offline_uuid("steve").expect("valid name");
    assert_ne!(upper, lower);
    assert_eq!(
        lower.to_string(),
        "53909932-f794-33c0-9329-948045a4c1ce",
        "lowercase names hash separately"
    );
}

#[test]
fn offline_uuid_is_deterministic() {
    for name in ["Notch", "Steve_123", "_x_"] {
        assert_eq!(offline_uuid(name), offline_uuid(name));
    }
}

#[test]
fn offline_uuid_rejects_invalid_names() {
    assert_eq!(offline_uuid("ab"), Err(OfflineError::NameLength));
    assert_eq!(offline_uuid("bad name"), Err(OfflineError::NameCharacters));
    assert_eq!(
        offline_uuid(&"a".repeat(MAX_PLAYER_NAME_LEN + 1)),
        Err(OfflineError::NameLength)
    );
}

#[test]
fn offline_account_mirrors_the_derived_identity() {
    let profile = offline_account("Notch").expect("Notch is a valid name");

    assert_eq!(profile.kind, AccountKind::Offline);
    assert_eq!(profile.display_name, "Notch");
    assert_eq!(profile.xuid, None);
    assert_eq!(
        profile.id.as_uuid(),
        offline_uuid("Notch").expect("valid name")
    );
    assert_eq!(
        profile.id.as_uuid().to_string(),
        "b50ad385-829d-3141-a216-7e7d7539ba7f"
    );
}

#[test]
fn offline_account_rejects_invalid_names() {
    assert_eq!(offline_account(""), Err(OfflineError::NameLength));
    assert_eq!(offline_account("a-b"), Err(OfflineError::NameCharacters));
}
