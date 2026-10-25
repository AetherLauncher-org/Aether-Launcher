use super::{CatalogError, MAX_CATALOG_BYTES, parse_version_catalog};

const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

fn sample() -> String {
    format!(
        r#"{{
          "latest": {{"release": "1.21.1", "snapshot": "24w14a"}},
          "versions": [
            {{"id": "1.21.1", "type": "release", "sha1": "{SHA1}",
              "url": "https://piston-meta.mojang.com/v1/packages/hash/1.21.1.json",
              "releaseTime": "2024-08-08T00:00:00+00:00", "complianceLevel": 1}},
            {{"id": "24w14a", "type": "snapshot", "sha1": "{SHA1}",
              "url": "https://piston-meta.mojang.com/v1/packages/hash/24w14a.json",
              "releaseTime": "2024-04-01T00:00:00+00:00"}}
          ]
        }}"#
    )
}

#[test]
fn parses_official_v2_shape_and_ignores_future_fields() {
    let catalog = parse_version_catalog(sample().as_bytes()).unwrap();
    assert_eq!(catalog.latest.release, "1.21.1");
    assert_eq!(catalog.versions.len(), 2);
    assert_eq!(catalog.versions[0].kind, "release");
    assert_eq!(catalog.versions[1].kind, "snapshot");
    let value = serde_json::to_value(&catalog.versions[0]).unwrap();
    assert_eq!(value["type"], "release");
    assert_eq!(value["releaseTime"], "2024-08-08T00:00:00+00:00");
}

#[test]
fn rejects_malformed_and_oversized_catalogs() {
    assert!(matches!(
        parse_version_catalog(b"{"),
        Err(CatalogError::InvalidJson(_))
    ));
    assert!(matches!(
        parse_version_catalog(&vec![b' '; MAX_CATALOG_BYTES + 1]),
        Err(CatalogError::TooLarge)
    ));
}

#[test]
fn rejects_missing_latest_unsafe_ids_and_untrusted_urls() {
    let valid = sample();
    for broken in [
        valid.replacen("\"snapshot\": \"24w14a\"", "\"snapshot\": \"missing\"", 1),
        valid.replace("\"1.21.1\"", "\"../escape\""),
        valid.replace("https://piston-meta.mojang.com/", "http://piston-meta.mojang.com/"),
        valid.replace("https://piston-meta.mojang.com/", "https://evil.example/"),
        valid.replace(SHA1, "not-a-sha1"),
    ] {
        assert!(matches!(
            parse_version_catalog(broken.as_bytes()),
            Err(CatalogError::InvalidEntries)
        ));
    }
}

#[test]
fn rejects_duplicate_versions_and_empty_lists() {
    let valid = sample();
    let duplicate = valid.replace("\"id\": \"24w14a\"", "\"id\": \"1.21.1\"");
    assert!(matches!(
        parse_version_catalog(duplicate.as_bytes()),
        Err(CatalogError::InvalidEntries)
    ));
    let empty = valid.replace(&valid[valid.find("[").unwrap()..=valid.rfind("]").unwrap()], "[]");
    assert!(matches!(
        parse_version_catalog(empty.as_bytes()),
        Err(CatalogError::InvalidEntries)
    ));
}
