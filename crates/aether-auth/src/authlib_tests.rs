use super::*;
use crate::http::HttpResponse;

fn ok(body: &str) -> HttpResponse {
    HttpResponse::new(200, body)
}

const GOOD: &str = r#"{"build_number":56,"version":"1.2.8","release_time":"2024-01-01T00:00:00Z","download_url":"https://example.invalid/jar.jar","checksums":{"sha256":"9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10"}}"#;

#[test]
fn parses_a_well_formed_manifest_and_ignores_download_url() {
    let manifest = parse_artifact_manifest(&ok(GOOD)).unwrap();
    assert_eq!(manifest.build_number, 56);
    assert_eq!(manifest.version, "1.2.8");
    assert_eq!(
        manifest.sha256,
        "9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10"
    );
}

#[test]
fn rejects_a_wrong_hash_length() {
    let body = GOOD.replace(
        "9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10",
        "9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b",
    );
    assert_eq!(parse_artifact_manifest(&ok(&body)), Err(ERR_INVALID));
}

#[test]
fn rejects_an_uppercase_or_non_hex_hash() {
    let upper = GOOD.replace('9', "A");
    assert_eq!(parse_artifact_manifest(&ok(&upper)), Err(ERR_INVALID));
    let nonhex = GOOD.replace('9', "g");
    assert_eq!(parse_artifact_manifest(&ok(&nonhex)), Err(ERR_INVALID));
}

#[test]
fn rejects_a_bad_version_shape() {
    for bad in [
        "",
        "2",
        "123456789.1",
        "1.2.8.0.0",
        "v1.2.8",
        "1.2-beta",
        "1..2",
        "1.2.",
        ".1",
    ] {
        let body = format!(
            r#"{{"build_number":56,"version":"{bad}","checksums":{{"sha256":"9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10"}}}}"#
        );
        assert_eq!(
            parse_artifact_manifest(&ok(&body)),
            Err(ERR_INVALID),
            "{bad}"
        );
    }
    for version in ["2.1", "2.1.3", "2.1.3.4"] {
        assert!(parse_artifact_manifest(&ok(&GOOD.replace("1.2.8", version))).is_ok());
    }
    assert_eq!(
        parse_artifact_manifest(&ok(&GOOD.replace("56", "1000000"))),
        Err(ERR_INVALID)
    );
}

#[test]
fn rejects_a_zero_or_absent_build_number() {
    let zero = GOOD.replace("\"build_number\":56", "\"build_number\":0");
    assert_eq!(parse_artifact_manifest(&ok(&zero)), Err(ERR_INVALID));
    let absent = GOOD.replace("\"build_number\":56,", "");
    assert_eq!(parse_artifact_manifest(&ok(&absent)), Err(ERR_INVALID));
}

#[test]
fn rejects_a_huge_body_and_a_non_success_status() {
    let huge = format!(
        r#"{{"build_number":56,"version":"1.2.8","checksums":{{"sha256":"9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10"}},"pad":"{}"}}"#,
        "x".repeat(MAX_MANIFEST_BODY)
    );
    assert_eq!(parse_artifact_manifest(&ok(&huge)), Err(ERR_TOO_LARGE));
    assert_eq!(
        parse_artifact_manifest(&HttpResponse::new(500, GOOD)),
        Err(ERR_STATUS)
    );
}
