use super::normalize_uuid;

#[test]
fn accepts_plain_and_dashed_uuids_and_lowercases_them() {
    let plain = "0123456789ABCDEF0123456789abcdef";
    let dashed = "01234567-89ab-cdef-0123-456789ABCDEF";
    assert_eq!(
        normalize_uuid(plain).unwrap(),
        "0123456789abcdef0123456789abcdef"
    );
    assert_eq!(
        normalize_uuid(dashed).unwrap(),
        "0123456789abcdef0123456789abcdef"
    );
    assert_eq!(
        normalize_uuid(&format!("  {plain}  ")).unwrap(),
        "0123456789abcdef0123456789abcdef"
    );
}

#[test]
fn rejects_malformed_uuids() {
    for bad in [
        "",
        "not-a-uuid",
        "0123456789abcdef0123456789abcde",
        "0123456789abcdef0123456789abcdefg",
        "0123456-789ab-cdef-0123-456789abcdef0",
        "01234567-89ab-cdef-0123-456789abcdeg",
    ] {
        assert!(normalize_uuid(bad).is_err(), "{bad:?} should be rejected");
    }
}
