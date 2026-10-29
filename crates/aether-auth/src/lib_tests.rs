use uuid::Uuid;

use super::{AccountId, AccountKind, AccountProfile};

#[test]
fn account_id_wraps_an_existing_uuid() {
    let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);

    let id = AccountId::from_uuid(uuid);

    assert_eq!(id.as_uuid(), uuid);
}

#[test]
fn account_id_parses_hyphenated_and_compact_text_only() {
    let id = AccountId::new();
    assert_eq!(AccountId::parse(&id.as_uuid().to_string()), Some(id));
    assert_eq!(
        AccountId::parse(&id.as_uuid().simple().to_string()),
        Some(id)
    );
    assert_eq!(AccountId::parse(""), None);
    assert_eq!(AccountId::parse("not-a-uuid"), None);
    assert_eq!(AccountId::parse("../../etc/passwd"), None);
}

#[test]
fn account_ids_are_unique_per_call() {
    assert_ne!(AccountId::new(), AccountId::new());
}

#[test]
fn account_id_serializes_transparently() {
    let uuid = Uuid::from_u128(1);
    let id = AccountId::from_uuid(uuid);

    let encoded = serde_json::to_string(&id).unwrap();

    assert_eq!(encoded, format!("\"{uuid}\""));
    assert_eq!(serde_json::from_str::<AccountId>(&encoded).unwrap(), id);
}

#[test]
fn profile_round_trips_through_json() {
    let profile = AccountProfile {
        id: AccountId::from_uuid(Uuid::from_u128(7)),
        display_name: "Steve".to_owned(),
        kind: AccountKind::Microsoft,
        xuid: Some("2535400000000000".to_owned()),
    };

    let encoded = serde_json::to_string(&profile).unwrap();
    let decoded: AccountProfile = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, profile);
}

#[test]
fn profiles_written_before_the_xuid_field_still_load() {
    // The shape produced by the previous revision of this crate.
    let legacy = r#"{
        "id": "00000000-0000-0000-0000-000000000001",
        "display_name": "Alex",
        "kind": "offline"
    }"#;

    let profile: AccountProfile = serde_json::from_str(legacy).unwrap();

    assert_eq!(profile.display_name, "Alex");
    assert_eq!(profile.kind, AccountKind::Offline);
    assert_eq!(profile.xuid, None);
}

#[test]
fn absent_xuid_is_omitted_from_the_encoding() {
    let profile = AccountProfile::offline(AccountId::from_uuid(Uuid::from_u128(2)), "Alex");

    let encoded = serde_json::to_string(&profile).unwrap();

    assert!(!encoded.contains("xuid"));
    assert!(encoded.contains("\"kind\":\"offline\""));
}

#[test]
fn account_kinds_use_snake_case_names() {
    assert_eq!(
        serde_json::to_string(&AccountKind::Microsoft).unwrap(),
        "\"microsoft\""
    );
    assert_eq!(
        serde_json::to_string(&AccountKind::Offline).unwrap(),
        "\"offline\""
    );
}

#[test]
fn offline_profiles_carry_no_xbox_identity() {
    let id = AccountId::from_uuid(Uuid::from_u128(3));

    let profile = AccountProfile::offline(id, "Alex");

    assert_eq!(profile.id, id);
    assert_eq!(profile.display_name, "Alex");
    assert_eq!(profile.kind, AccountKind::Offline);
    assert_eq!(profile.xuid, None);
}
