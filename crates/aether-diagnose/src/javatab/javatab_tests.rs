use super::*;

#[test]
fn every_table_entry_resolves() {
    for entry in PARAMETER_MIN_JAVA {
        assert_eq!(min_java_for_argument(entry.parameter), Some(entry.min_java));
    }
}

#[test]
fn equals_form_resolves() {
    assert_eq!(
        min_java_for_argument("--sun-misc-unsafe-memory-access=allow"),
        Some(23)
    );
    assert_eq!(
        min_java_for_argument("--add-opens=java.base/java.lang=ALL-UNNAMED"),
        Some(9)
    );
}

#[test]
fn split_equals_form() {
    assert_eq!(
        split_argument("--add-opens=java.base/java.lang"),
        ("--add-opens", Some("java.base/java.lang"))
    );
}

#[test]
fn split_bare_form() {
    assert_eq!(
        split_argument("--enable-preview"),
        ("--enable-preview", None)
    );
}

#[test]
fn space_form_uses_flag_token() {
    assert_eq!(min_java_for_argument("--enable-preview"), Some(12));
    assert_eq!(min_java_for_argument("-XX:+UseZGC"), Some(11));
}

#[test]
fn use_zgc_plus_resolves() {
    assert_eq!(min_java_for_argument("-XX:+UseZGC"), Some(11));
}

#[test]
fn use_zgc_minus_resolves() {
    assert_eq!(min_java_for_argument("-XX:-UseZGC"), Some(11));
}

#[test]
fn other_xx_polarity_resolves() {
    assert_eq!(min_java_for_argument("-XX:-UseG1GC"), Some(7));
}

#[test]
fn prefix_is_used_for_equals_values() {
    assert_eq!(min_java_for_argument("-XX:MaxRAMPercentage=75"), Some(10));
}

#[test]
fn longest_prefix_wins() {
    assert_eq!(
        min_java_for_argument("--add-opens=java.base/java.lang"),
        Some(9)
    );
}

#[test]
fn unknown_argument_is_none() {
    assert_eq!(min_java_for_argument("--not-a-real-jvm-flag"), None);
}

#[test]
fn xmx_is_not_gated() {
    assert_eq!(min_java_for_argument("-Xmx2G"), None);
}

#[test]
fn xms_is_not_gated() {
    assert_eq!(min_java_for_argument("-Xms2G"), None);
}

#[test]
fn arbitrary_property_is_not_gated() {
    assert_eq!(min_java_for_argument("-Dfoo=bar"), None);
}

#[test]
fn minecraft_game_dir_is_not_gated() {
    assert_eq!(min_java_for_argument("--gameDir"), None);
}

#[test]
fn minecraft_assets_dir_is_not_gated() {
    assert_eq!(min_java_for_argument("--assetsDir"), None);
}

#[test]
fn minecraft_asset_index_is_not_gated() {
    assert_eq!(min_java_for_argument("--assetIndex"), None);
}

#[test]
fn minecraft_version_is_not_gated() {
    assert_eq!(min_java_for_argument("--version"), None);
}

#[test]
fn minecraft_access_token_is_not_gated() {
    assert_eq!(min_java_for_argument("--accessToken"), None);
}

#[test]
fn preflight_flags_unsafe_memory_on_java_21() {
    let arguments = vec!["--sun-misc-unsafe-memory-access=allow".to_owned()];
    let issues = preflight_jvm_arguments(&arguments, Some(21));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].min_java, 23);
    assert_eq!(issues[0].selected_java, Some(21));
}

#[test]
fn preflight_java_23_has_no_issue_for_unsafe_memory() {
    let arguments = vec!["--sun-misc-unsafe-memory-access".to_owned()];
    assert!(preflight_jvm_arguments(&arguments, Some(23)).is_empty());
}

#[test]
fn preflight_unknown_java_has_no_issues() {
    let arguments = vec!["--sun-misc-unsafe-memory-access".to_owned()];
    assert!(preflight_jvm_arguments(&arguments, None).is_empty());
}

#[test]
fn preflight_order_is_stable() {
    let arguments = vec![
        "-XX:+UseCompactObjectHeaders".to_owned(),
        "--enable-preview".to_owned(),
        "-XX:+ZGenerational".to_owned(),
    ];
    let issues = preflight_jvm_arguments(&arguments, Some(6));
    assert_eq!(
        issues
            .iter()
            .map(|issue| issue.argument.as_str())
            .collect::<Vec<_>>(),
        vec![
            "-XX:+UseCompactObjectHeaders",
            "--enable-preview",
            "-XX:+ZGenerational",
        ]
    );
}

#[test]
fn preflight_deduplicates_identical_arguments() {
    let arguments = vec!["--enable-preview".to_owned(), "--enable-preview".to_owned()];
    assert_eq!(preflight_jvm_arguments(&arguments, Some(11)).len(), 1);
}

#[test]
fn preflight_message_contains_required_and_selected_numbers() {
    let arguments = vec!["--enable-preview".to_owned()];
    let issues = preflight_jvm_arguments(&arguments, Some(11));
    assert!(issues[0].message_zh.contains("12"));
    assert!(issues[0].message_zh.contains("11"));
}
