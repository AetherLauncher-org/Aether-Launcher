use super::{PlaceholderMap, redacted};

#[test]
fn access_token_equals_form_is_redacted() {
    let result = redacted("--accessToken=abc123");
    assert_eq!(result.text, "--accessToken=<TOKEN:1>");
    assert_eq!(
        result.placeholders.restore(&result.text),
        "--accessToken=abc123"
    );
}

#[test]
fn access_token_whitespace_form_is_redacted() {
    let result = redacted("--accessToken abc123");
    assert_eq!(result.text, "--accessToken <TOKEN:1>");
}

#[test]
fn dash_started_argument_value_is_not_consumed() {
    let result = redacted("--username --uuid 123e4567-e89b-12d3-a456-426614174000");
    assert_eq!(result.text, "--username --uuid <UUID:1>");
}

#[test]
fn every_secret_argument_category_is_redacted() {
    let input = "--accessToken a --refreshToken b --clientToken c --username d --uuid e --xuid f --clientId g --userProperties h --assetIndex i";
    let result = redacted(input);
    assert!(result.text.contains("--accessToken <TOKEN:1>"));
    assert!(result.text.contains("--refreshToken <TOKEN:2>"));
    assert!(result.text.contains("--clientToken <TOKEN:3>"));
    assert!(result.text.contains("--username <USERNAME:1>"));
    assert!(result.text.contains("--uuid <UUID:1>"));
    assert!(result.text.contains("--xuid <XUID:1>"));
    assert!(result.text.contains("--clientId <CLIENT_ID:1>"));
    assert!(result.text.contains("--userProperties <USER_PROPERTIES:1>"));
    assert!(result.text.contains("--assetIndex i"));
}

#[test]
fn token_kind_is_shared_by_token_flags() {
    let result = redacted("--accessToken same --refreshToken same --clientToken same");
    assert_eq!(
        result.text,
        "--accessToken <TOKEN:1> --refreshToken <TOKEN:1> --clientToken <TOKEN:1>"
    );
}

#[test]
fn windows_backslash_user_path_is_redacted() {
    let result = redacted(r"C:\Users\Alice\AppData\Roaming");
    assert_eq!(result.text, r"C:\Users\<USERNAME:1>\AppData\Roaming");
}

#[test]
fn windows_slash_user_path_is_redacted() {
    let result = redacted("C:/Users/Alice/.minecraft");
    assert_eq!(result.text, "C:/Users/<USERNAME:1>/.minecraft");
}

#[test]
fn unix_home_path_is_redacted() {
    let result = redacted("/home/alice/.minecraft");
    assert_eq!(result.text, "/home/<USERNAME:1>/.minecraft");
}

#[test]
fn macos_user_path_is_redacted() {
    let result = redacted("/Users/alice/Library/Application Support");
    assert_eq!(
        result.text,
        "/Users/<USERNAME:1>/Library/Application Support"
    );
}

#[test]
fn path_username_number_is_reused() {
    let result = redacted("/home/alice/a C:/Users/alice/b /Users/bob/c");
    assert_eq!(
        result.text,
        "/home/<USERNAME:1>/a C:/Users/<USERNAME:1>/b /Users/<USERNAME:2>/c"
    );
}

#[test]
fn real_email_is_redacted() {
    let result = redacted("contact player@example.com now");
    assert_eq!(result.text, "contact <EMAIL:1> now");
}

#[test]
fn module_launcher_version_email_survives() {
    let input = "cpw.mods.modlauncher@10.0.8";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn bootstrap_module_version_email_survives() {
    let input = "MC-BOOTSTRAP/fmlloader@1.20.1-47.2.16";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn forge_module_version_email_survives() {
    let input = "net.minecraftforge@1.20.1-47.2.16";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn slash_and_colon_email_candidates_are_not_redacted() {
    let input = "pkg/name@example.com C:name@example.com";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn bearer_header_token_is_redacted() {
    let result = redacted("Authorization: Bearer abc.def.ghi");
    assert_eq!(result.text, "Authorization: Bearer <TOKEN:1>");
}

#[test]
fn bare_bearer_token_is_redacted() {
    let result = redacted("Bearer secret-token-value");
    assert_eq!(result.text, "Bearer <TOKEN:1>");
}

#[test]
fn jwt_shape_is_redacted() {
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4ifQ.signature_payload_long_enough_123456789";
    let result = redacted(jwt);
    assert_eq!(result.text, "<TOKEN:1>");
    assert_eq!(result.placeholders.restore(&result.text), jwt);
}

#[test]
fn short_jwt_shape_survives() {
    let input = "eyJabc.def.ghi";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn ipv4_is_redacted() {
    let result = redacted("Connection failed at 192.168.1.10");
    assert_eq!(result.text, "Connection failed at <IP:1>");
}

#[test]
fn invalid_ipv4_octet_survives() {
    let input = "999.1.1.1";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn three_group_version_survives() {
    let input = "Minecraft 1.20.1";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn mod_version_ipv4_ambiguity_survives() {
    let input = "fmlloader@1.20.1-47.2.16";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn jar_context_ipv4_survives() {
    let input = "libraries/mod.jar:1.2.3.4";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn server_with_port_is_redacted() {
    let result = redacted("wss://mc.example.com:25565");
    assert_eq!(result.text, "<SERVER:1>");
}

#[test]
fn bare_server_with_port_is_redacted() {
    let result = redacted("server mc.example.com:25565");
    assert_eq!(result.text, "server <SERVER:1>");
}

#[test]
fn connecting_context_host_is_redacted() {
    let result = redacted("Connecting to play.hypixel.net");
    assert_eq!(result.text, "Connecting to <SERVER:1>");
}

#[test]
fn dependency_hosts_are_diagnostic_and_survive() {
    let input = "https://minecraft.net https://mojang.com https://fabricmc.net https://forge.net https://neoforged.net https://maven.org https://piston-data.mojang.com https://libraries.minecraft.net";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn mod_ids_survive_byte_identically() {
    let input = "micdoodle8 xaerobetterpvp iris sodium create";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn hashes_survive_byte_identically() {
    let input = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn hexadecimal_addresses_survive_byte_identically() {
    let input = "0x00007ff6a1b2c3d4";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn texture_sizes_survive_byte_identically() {
    let input = "512x256x4";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn same_original_value_reuses_placeholder() {
    let result = redacted("--username Alice /home/Alice/x player@example.com player@example.com");
    assert_eq!(result.text.matches("<USERNAME:1>").count(), 2);
    assert_eq!(result.text.matches("<EMAIL:1>").count(), 2);
    assert_eq!(result.placeholders.pairs().len(), 2);
}

#[test]
fn restore_round_trip_covers_multiple_categories() {
    let input = "--accessToken abc --username Alice /home/Alice/x player@example.com 192.168.1.10 Connecting to play.example.net Bearer bearer-secret";
    let result = redacted(input);
    assert_ne!(result.text, input);
    assert_eq!(result.placeholders.restore(&result.text), input);
}

#[test]
fn restore_does_not_rescan_original_placeholder_like_text() {
    let map = PlaceholderMap {
        pairs: vec![("<TOKEN:1>".to_string(), "literal <TOKEN:2>".to_string())],
    };
    assert_eq!(map.restore("<TOKEN:1>"), "literal <TOKEN:2>");
}

#[test]
fn redaction_is_idempotent() {
    let first = redacted("--username Alice player@example.com 192.168.1.10");
    let second = redacted(&first.text);
    assert_eq!(second.text, first.text);
    assert_eq!(second.report.replacement_count, 0);
}

#[test]
fn report_counts_occurrences_without_secrets() {
    let result =
        redacted("--username Alice --username Alice player@example.com player@example.com");
    assert_eq!(result.report.replacement_count, 4);
    assert!(result.report.by_kind.contains(&("USERNAME".to_string(), 2)));
    assert!(result.report.by_kind.contains(&("EMAIL".to_string(), 2)));
    let serialized =
        serde_json::to_string(&result.report).expect("report serialization should work");
    assert!(!serialized.contains("Alice"));
}

#[test]
fn empty_input_is_unchanged() {
    let result = redacted("");
    assert!(result.text.is_empty());
    assert!(result.placeholders.pairs().is_empty());
    assert_eq!(result.report.replacement_count, 0);
}

#[test]
fn ordinary_diagnostic_text_is_unchanged() {
    let input = "[main/INFO] Loading 12 mods, Java 21, texture 512x256x4";
    assert_eq!(redacted(input).text, input);
}

#[test]
fn user_properties_equals_form_is_redacted() {
    let result = redacted("--userProperties={\"demo\":true}");
    assert_eq!(result.text, "--userProperties=<USER_PROPERTIES:1>");
}

#[test]
fn uuid_and_xuid_have_distinct_kinds() {
    let result = redacted("--uuid value --xuid value");
    assert_eq!(result.text, "--uuid <UUID:1> --xuid <XUID:1>");
}

#[test]
fn server_placeholder_reuse_is_consistent() {
    let result = redacted("mc.example.com:25565 mc.example.com:25565");
    assert_eq!(result.text, "<SERVER:1> <SERVER:1>");
    assert_eq!(result.placeholders.pairs().len(), 1);
}

#[test]
fn ip_placeholder_reuse_is_consistent() {
    let result = redacted("1.2.3.4 1.2.3.4");
    assert_eq!(result.text, "<IP:1> <IP:1>");
}

#[test]
fn bearer_keyword_case_is_supported() {
    let result = redacted("bearer secret");
    assert_eq!(result.text, "bearer <TOKEN:1>");
}

#[test]
fn asset_index_is_explicitly_not_secret() {
    let input = "--assetIndex 1.20.1";
    assert_eq!(redacted(input).text, input);
}
