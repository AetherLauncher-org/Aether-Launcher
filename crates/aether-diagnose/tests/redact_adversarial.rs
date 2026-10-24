//! Adversarial integration tests for the first-layer redaction contract.
//!
//! Every test uses realistic fragments of Minecraft / Forge / Fabric crash
//! logs and launch command lines (shapes taken from `docs/design/crash-analysis.md`
//! and the HMCL corpus evaluation described there), and tries to break the
//! implementation: over-redaction of `module@version` strings, mod ids and
//! hashes, under-redaction of launch-argument secrets, and roundtrip failures.

use aether_diagnose::redact::redacted;

/// Roundtrip: `restore(redacted text) == original`, placeholders not leaked.
fn check_roundtrip(input: &str) {
    let r = redacted(input);
    let restored = r.placeholders.restore(&r.text);
    assert_eq!(
        restored, input,
        "restore() roundtrip broke.\ninput:\n{input}\nredacted:\n{}\nrestored:\n{restored}",
        r.text
    );
}

/// The secret value must not appear in the redacted text or the debug report.
fn check_secret_removed(input: &str, secret: &str) {
    let r = redacted(input);
    assert!(
        !r.text.contains(secret),
        "secret value survived redaction.\nsecret: {secret}\ninput:\n{input}\nredacted:\n{}",
        r.text
    );
    let report_dbg = format!("{:?}", r.report);
    assert!(
        !report_dbg.contains(secret),
        "secret value leaked into report debug output.\nsecret: {secret}\ninput:\n{input}\nreport: {report_dbg}"
    );
}

/// A value that must survive byte-identical inside the redacted text.
fn check_survives(input: &str, kept: &str) {
    let r = redacted(input);
    assert!(
        r.text.contains(kept),
        "over-redaction: value that must survive was changed.\nmust keep: {kept}\ninput:\n{input}\nredacted:\n{}",
        r.text
    );
}

// ---------------------------------------------------------------------------
// Rule 1: launch-argument secrets in --k=v and --k v forms.
// ---------------------------------------------------------------------------

const ACCESS_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N5Zh7Bh4LtG";
const REFRESH_TOKEN: &str = "8f14e45fceea167a5a36dedd4bea2543f9a3c1d4e8f7b2a6c0d5e3f1a9b8c7d6e";
const CLIENT_TOKEN: &str = "c0ffeec0ffeec0ffeec0ffeec0ffeec0";
const PLAYER_UUID: &str = "069a79f444e94726a5befca90e38aaf5";

#[test]
fn launch_args_mixed_equals_and_space_forms() {
    let input = format!(
        "java -Xmx4G -cp libraries/net/minecraftforge/fmlloader/1.20.1-47.2.16/fmlloader-1.20.1-47.2.16.jar \
net.minecraft.client.main.Main --username Steve --version 1.20.1 \
--gameDir . --assetsDir assets --accessToken {ACCESS_TOKEN} \
--refreshToken={REFRESH_TOKEN} --clientToken {CLIENT_TOKEN} \
--uuid={PLAYER_UUID} --xuid 2535414328385137 --clientId 4b1d0e23-5f6a-4c2b-9d3e-8a7f6b5c4d3e \
--userType msa --versionType Forge"
    );
    check_roundtrip(&input);
    for secret in [
        ACCESS_TOKEN,
        REFRESH_TOKEN,
        CLIENT_TOKEN,
        PLAYER_UUID,
        "4b1d0e23-5f6a-4c2b-9d3e-8a7f6b5c4d3e",
    ] {
        check_secret_removed(&input, secret);
    }
    // Player display name counts as a launch-argument secret after --username.
    check_secret_removed(&input, "Steve");
    // The mod loader coordinates must survive.
    check_survives(&input, "fmlloader-1.20.1-47.2.16.jar");
    check_survives(&input, "--versionType Forge");
}

#[test]
fn launch_args_flag_dash_immediately_after_value_flag() {
    // A real flag directly after --k must not be swallowed as the value, and
    // the values after --username and --uuid are real secrets and must go.
    let input = format!(
        "--accessToken -Xmx4G --username Alex --uuid {PLAYER_UUID} --clientId -Djava.library.path=libs"
    );
    check_roundtrip(&input);
    check_secret_removed(&input, "Alex");
    check_secret_removed(&input, PLAYER_UUID);
}

#[test]
fn launch_args_xuid_and_username_equals_form() {
    let input = "--username=Herobrine0451 --xuid=2535414328385137 --gameDir run";
    check_roundtrip(input);
    check_secret_removed(input, "Herobrine0451");
    check_secret_removed(input, "2535414328385137");
    check_survives(input, "--gameDir run");
}

#[test]
fn launch_args_quoted_windows_command_line() {
    let input = format!(
        "\"C:\\Program Files\\Java\\jdk-17\\bin\\javaw.exe\" -XX:+UseG1GC --username \"Notch Test\" --accessToken {ACCESS_TOKEN} --width 854 --height 480"
    );
    check_roundtrip(&input);
    check_secret_removed(&input, ACCESS_TOKEN);
    check_survives(&input, "-XX:+UseG1GC");
    check_survives(&input, "--width 854");
}

#[test]
fn launch_args_token_at_very_end_of_input() {
    // Secret value at the end of the buffer with no trailing newline.
    let input = format!("Main --accessToken {ACCESS_TOKEN}");
    check_roundtrip(&input);
    check_secret_removed(&input, ACCESS_TOKEN);
}

#[test]
fn launch_args_repeated_flag_redacts_both() {
    let input = format!("--accessToken {ACCESS_TOKEN} ... retry ... --accessToken {ACCESS_TOKEN}");
    check_roundtrip(&input);
    check_secret_removed(&input, ACCESS_TOKEN);
    // Rule 8: same original twice -> same placeholder.
    let r = redacted(&input);
    let matching = r
        .placeholders
        .pairs()
        .iter()
        .filter(|(_, orig)| orig == ACCESS_TOKEN)
        .count();
    assert_eq!(
        matching, 1,
        "same secret redacted twice must map to ONE placeholder pair.\ninput:\n{input}"
    );
}

// ---------------------------------------------------------------------------
// Rule 2: user names in paths.
// ---------------------------------------------------------------------------

#[test]
fn windows_path_username_backslash() {
    let input = "java.lang.RuntimeException: Mod loading failed\tat app//net.minecraftforge.fml.loading.modloading.WorkExecutor.run(WorkExecutor.java:65)\n\
at java.base@17.0.9/java.lang.Thread.run(Thread.java:833)\n\
java.io.FileNotFoundException: C:\\Users\\mr_smith\\AppData\\Roaming\\.minecraft\\logs\\latest.log";
    check_roundtrip(input);
    check_secret_removed(input, "mr_smith");
    check_survives(input, "AppData\\Roaming\\.minecraft\\logs\\latest.log");
    check_survives(input, "java.base@17.0.9");
}

#[test]
fn windows_path_username_forward_slash() {
    let input = "at TRANSFORMER/micdoodle8.mods.galacticraft.core.client.ClientProxy.init(ClientProxy.java:212)\n\
loading config from C:/Users/Zhang Wei/AppData/Roaming/.minecraft/config/iris.properties";
    check_roundtrip(input);
    check_secret_removed(input, "Zhang Wei");
    check_survives(
        input,
        "micdoodle8.mods.galacticraft.core.client.ClientProxy",
    );
}

#[test]
fn unix_home_paths() {
    let input = "/home/steam/.local/share/multimc/instances/1.20.1/.minecraft/logs/latest.log\n\
/Users/alan.turing/Library/Application Support/minecraft/crash-reports/crash-2026-10-24_11.04.12-server.txt";
    check_roundtrip(input);
    check_secret_removed(input, "alan.turing");
    check_survives(input, "crash-2026-10-24_11.04.12-server.txt");
}

#[test]
fn username_in_path_and_in_flag_is_same_secret() {
    // The user name appears both as a path segment and as --username value.
    let input = "--username devon42 --gameDir C:\\Users\\devon42\\AppData\\Roaming\\.minecraft";
    check_roundtrip(input);
    check_secret_removed(input, "devon42");
    let r = redacted(input);
    let count = r
        .placeholders
        .pairs()
        .iter()
        .filter(|(_, orig)| orig == "devon42")
        .count();
    assert_eq!(
        count, 1,
        "same value as --username and path user must share one placeholder.\ninput:\n{input}"
    );
}

#[test]
fn unicode_username_path_no_panic_exact_restore() {
    let input = "Crash report saved to: C:\\Users\\李明\\AppData\\Roaming\\.minecraft\\crash-reports\\crash-2026-10-24_11.04.12-client.txt";
    let r = redacted(input);
    check_roundtrip(input);
    assert!(
        !r.text.contains("李明"),
        "unicode user name must be redacted.\ninput:\n{input}\nredacted:\n{}",
        r.text
    );
}

#[test]
fn unicode_username_mixed_script() {
    let input = "Failed to read /Users/Владислав-Ω/Library/Application Support/minecraft/options.txt: file locked";
    check_roundtrip(input);
    check_secret_removed(input, "Владислав-Ω");
}

// ---------------------------------------------------------------------------
// Rule 3: emails vs module@version.
// ---------------------------------------------------------------------------

#[test]
fn module_version_strings_survive() {
    let input = "        [18:42:11] [main/INFO]: Loading Minecraft Forge 47.2.16 with modules:\n\
  MC-BOOTSTRAP/fmlloader@1.20.1-47.2.16.jar loaded\n\
  MC-BOOTSTRAP/cpw.mods.modlauncher@10.0.8 loaded from library\n\
  net.minecraftforge@1.20.1-47.2.16\n\
  org.ow2.asm@9.5\n\
  com.mojang:brigadier@1.1.8";
    check_roundtrip(input);
    for kept in [
        "MC-BOOTSTRAP/fmlloader@1.20.1-47.2.16",
        "cpw.mods.modlauncher@10.0.8",
        "net.minecraftforge@1.20.1-47.2.16",
        "org.ow2.asm@9.5",
        "brigadier@1.1.8",
    ] {
        check_survives(input, kept);
    }
}

#[test]
fn genuine_email_is_redacted() {
    let input = "[18:42:11] [main/INFO]: Contact: steve.harrington1985@example.com reported the crash on 1.20.1";
    check_roundtrip(input);
    check_secret_removed(input, "steve.harrington1985@example.com");
    check_survives(input, "1.20.1");
}

#[test]
fn email_and_module_version_on_adjacent_lines() {
    let input = "[INFO]: MC-BOOTSTRAP/fmlloader@1.20.1-47.2.16\n[INFO]: reporter email: notch@mojang.example";
    check_roundtrip(input);
    check_survives(input, "MC-BOOTSTRAP/fmlloader@1.20.1-47.2.16");
    check_secret_removed(input, "notch@mojang.example");
}

#[test]
fn module_version_inside_brace_and_dev_line() {
    let input = "Mod File: fmlloader-1.20.1-47.2.16.jar (META-INF/mods.toml: net.minecraftforge@1.20.1-47.2.16)\n\
dev marker: fmlloader@1.20.1-47.2.16+mc";
    check_roundtrip(input);
    check_survives(input, "net.minecraftforge@1.20.1-47.2.16");
    check_survives(input, "fmlloader@1.20.1-47.2.16+mc");
}

// ---------------------------------------------------------------------------
// Rule 4: Bearer tokens and JWTs.
// ---------------------------------------------------------------------------

#[test]
fn bearer_token_redacted() {
    let jwt =
        "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJhZXRoZXIifQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let input = format!(
        "GET /session/minecraft/join HTTP/1.1\r\nAuthorization: Bearer {jwt}\r\nHost: sessionserver.mojang.com"
    );
    check_roundtrip(&input);
    check_secret_removed(&input, jwt);
    check_survives(&input, "sessionserver.mojang.com");
}

#[test]
fn long_jwt_shaped_bare_token_is_redacted() {
    let input = "token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VybmFtZSI6IlN0ZXZlIn0.dozjgNryP4J3jVmNHl0w5N5Z end";
    check_roundtrip(input);
    check_secret_removed(
        input,
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VybmFtZSI6IlN0ZXZlIn0.dozjgNryP4J3jVmNHl0w5N5Z",
    );
}

#[test]
fn short_eyj_string_not_redacted() {
    // Two dot-separated segments under 60 chars are not a JWT; an exception
    // class name must survive.
    let input = "Caused by: eyJ.short.token at cpw.mods.modlauncher@10.0.8";
    check_roundtrip(input);
    check_survives(input, "eyJ.short.token");
}

// ---------------------------------------------------------------------------
// Rule 5: IPv4 vs mod versions.
// ---------------------------------------------------------------------------

#[test]
fn ipv4_address_redacted() {
    let input =
        "[19:03:44] [Server thread/INFO]: Testing connection to 192.168.1.42:25565 (ping 23ms)";
    check_roundtrip(input);
    check_secret_removed(input, "192.168.1.42");
}

#[test]
fn mod_version_three_groups_survive() {
    let input = "Forge mod load: 1.20.1, mappings channel: official, version 47.2.16, fmlloader@1.20.1-47.2.16";
    check_roundtrip(input);
    check_survives(input, "1.20.1");
    check_survives(input, "47.2.16");
    check_survives(input, "fmlloader@1.20.1-47.2.16");
}

#[test]
fn ipv4_inside_mod_version_not_mangled() {
    // The version suffix 47.2.16 is 3 groups; 1.20.1-47.2.16 contains no
    // 4-group run, and the whole string must stay byte-identical.
    let input = "MC-BOOTSTRAP/fmlloader@1.20.1-47.2.16 net.minecraftforge@1.20.1-47.2.16";
    check_roundtrip(input);
    check_survives(input, "fmlloader@1.20.1-47.2.16");
    check_survives(input, "net.minecraftforge@1.20.1-47.2.16");
}

#[test]
fn java_version_four_groups_is_version_not_ip() {
    // Java 17.0.9 style update strings use 4 dot groups but are versions.
    let input = "java.base@17.0.9, java.version=17.0.9, JRE 17.0.9+8 LTS";
    check_roundtrip(input);
    check_survives(input, "17.0.9");
}

// ---------------------------------------------------------------------------
// Rule 6: server addresses vs well-known diagnostic hosts.
// ---------------------------------------------------------------------------

#[test]
fn server_host_port_redacted() {
    let input = "[Server thread/INFO]: Connection established: /play.example.net:25565";
    check_roundtrip(input);
    check_secret_removed(input, "play.example.net");
}

#[test]
fn websocket_url_redacted() {
    let input =
        "[main/INFO]: [Fabric] Connecting to wss://eu-west-3.mc-hypixel.example.net:443/v1/socket";
    check_roundtrip(input);
    check_secret_removed(input, "eu-west-3.mc-hypixel.example.net");
}

#[test]
fn well_known_hosts_survive() {
    let input = "Downloading libraries.minecraft.net/libraries/net/minecraftforge/fmlloader/1.20.1-47.2.16/fmlloader-1.20.1-47.2.16.jar\n\
Fetching piston-data.mojang.com/v1/packages/da39a3ee5e6b4b0d3255bfef95601890afd80709.json\n\
Resolving maven.fabricmc.net:443 ok";
    check_roundtrip(input);
    check_survives(input, "libraries.minecraft.net");
    check_survives(input, "piston-data.mojang.com");
    check_survives(input, "maven.fabricmc.net");
}

// ---------------------------------------------------------------------------
// Rule 7: forbidden high-entropy heuristics.
// ---------------------------------------------------------------------------

#[test]
fn mod_ids_sha1_hex_and_sizes_survive() {
    let input = "        mod micdoodle8 loaded (galacticraft core)\n\
        mod xaerobetterpvp loaded\n\
        mod iris loaded, shaderpack: ComplementaryUnbound_r5.2.2\n\
        mod sodium loaded\n\
        assets verified: da39a3ee5e6b4b0d3255bfef95601890afd80709 (sha1)\n\
        md5 d41d8cd98f00b204e9800998ecf8427e\n\
        native crash at 0x00007ff6a1b2c3d4\n\
        texture atlas 512x256x4, mipmaps 4";
    check_roundtrip(input);
    for kept in [
        "micdoodle8",
        "xaerobetterpvp",
        "iris",
        "sodium",
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
        "d41d8cd98f00b204e9800998ecf8427e",
        "0x00007ff6a1b2c3d4",
        "512x256x4",
    ] {
        check_survives(input, kept);
    }
}

#[test]
fn mod_id_suffix_digits_survive() {
    let input = "[main/INFO]: Loaded 128 mods including micdoodle8, xaerobetterpvp, xaerominimap, sodium, iris";
    check_roundtrip(input);
    check_survives(input, "micdoodle8");
    check_survives(input, "xaerobetterpvp");
}

// ---------------------------------------------------------------------------
// Rule 8/9 + edge cases: crash report shape, mixed newlines, degenerate inputs,
// idempotency, report leaks.
// ---------------------------------------------------------------------------

#[test]
fn crash_report_excerpt_multi_rule() {
    let input = format!(
        "---- Minecraft Crash Report ----\r\n\
// Why did you do that?\r\n\
Time: 2026-10-24 11:04:12 +08:00\r\n\
Description: Rendering overlay\r\n\
\r\n\
java.lang.IllegalStateException: Render thread failed\r\n\
    at app//net.minecraftforge.client.ForgeHooksClient.renderMainThreadOverlay(ForgeHooksClient.java:212)\r\n\
    at TRANSFORMER/iris.shaders.ShaderProcessor.render(ShaderProcessor.java:88)\r\n\
Caused by: java.io.IOException: Cannot read C:\\Users\\kaito_92\\AppData\\Roaming\\.minecraft\\shaderpacks\\ComplementaryUnbound_r5.2.2.zip\r\n\
    at java.base@17.0.9/java.io.FileInputStream.readBytes(Native Method)\r\n\
\r\n\
-- System Details --\r\n\
Graphics card #0 name: NVIDIA GeForce RTX 3060, vendor: NVIDIA Corporation, OpenGL version: 4.6.0 NVIDIA 552.44\r\n\
Server: mc.hypixel.example.net:25565 (address redacted by launcher?)\r\n\
Auth token in context: {ACCESS_TOKEN}\r\n"
    );
    check_roundtrip(&input);
    check_secret_removed(&input, "kaito_92");
    check_secret_removed(&input, ACCESS_TOKEN);
    check_secret_removed(&input, "mc.hypixel.example.net");
    check_survives(&input, "OpenGL version: 4.6.0 NVIDIA 552.44");
    check_survives(&input, "java.base@17.0.9");
    check_survives(&input, "TRANSFORMER/iris.shaders.ShaderProcessor.render");
}

#[test]
fn mixed_crlf_and_lf_restore_exact() {
    let input = "line one\r\nC:\\Users\\oméga\\logs\\latest.log\ncrlf and bare lf mixed\r\n--accessToken ab7f6b5c4d3e8a7f6b5c4d3e8a7f6b5c end";
    check_roundtrip(input);
    check_secret_removed(input, "oméga");
    check_secret_removed(input, "ab7f6b5c4d3e8a7f6b5c4d3e8a7f6b5c");
}

#[test]
fn empty_input() {
    let r = redacted("");
    assert_eq!(r.text, "", "empty input must stay empty: {:?}", r.text);
    check_roundtrip("");
}

#[test]
fn literal_placeholder_lookalike_passthrough() {
    // Input that already looks like OUR placeholder must not confuse restore.
    let input = "got <TOKEN:1> and ⟨RED:2⟩ from a chat message, cpw.mods.modlauncher@10.0.8 nearby";
    let r = redacted(input);
    let restored = r.placeholders.restore(&r.text);
    assert_eq!(
        restored, input,
        "placeholder-lookalike input must roundtrip exactly.\ninput:\n{input}\nredacted:\n{}\nrestored:\n{restored}",
        r.text
    );
}

#[test]
fn one_megabyte_input_completes_and_restores() {
    let mut big = String::with_capacity(1 << 20);
    let line = "[12:00:00] [main/INFO]: cpw.mods.modlauncher@10.0.8 tick, C:\\Users\\bulkuser\\run\\logs\\latest.log line\r\n";
    while big.len() + line.len() < (1 << 20) {
        big.push_str(line);
    }
    big.push_str(&format!("--accessToken {ACCESS_TOKEN}"));
    let r = redacted(&big);
    let restored = r.placeholders.restore(&r.text);
    assert_eq!(
        restored.len(),
        big.len(),
        "1MB input restore must reproduce input exactly (len mismatch)"
    );
    assert_eq!(restored, big, "1MB input restore mismatch");
    assert!(
        !r.text.contains(ACCESS_TOKEN),
        "token leaked in 1MB input redaction"
    );
}

#[test]
fn idempotent_redaction() {
    let input = format!(
        "C:\\Users\\idem\\logs --accessToken {ACCESS_TOKEN} fmlloader@1.20.1-47.2.16 Bearer {ACCESS_TOKEN}"
    );
    let once = redacted(&input);
    let twice = redacted(&once.text);
    assert_eq!(
        twice.text, once.text,
        "redaction must be idempotent.\nfirst pass:\n{}\nsecond pass:\n{}",
        once.text, twice.text
    );
}

#[test]
fn report_debug_never_contains_secrets() {
    let input = format!(
        "--accessToken {ACCESS_TOKEN} --username secretuname --uuid {PLAYER_UUID} C:\\Users\\secretuname\\logs Bearer {ACCESS_TOKEN} a@b.example 10.0.0.7"
    );
    let r = redacted(&input);
    let report_dbg = format!("{:?}", r.report);
    for secret in [
        ACCESS_TOKEN,
        "secretuname",
        PLAYER_UUID,
        "a@b.example",
        "10.0.0.7",
    ] {
        assert!(
            !report_dbg.contains(secret),
            "report Debug leaked secret {secret}.\ninput:\n{input}\nreport: {report_dbg}"
        );
    }
}

#[test]
fn same_email_twice_one_placeholder() {
    let input = "contact: admin@example.org, again admin@example.org";
    check_roundtrip(input);
    check_secret_removed(input, "admin@example.org");
    let r = redacted(input);
    let count = r
        .placeholders
        .pairs()
        .iter()
        .filter(|(_, orig)| orig == "admin@example.org")
        .count();
    assert_eq!(
        count, 1,
        "repeated email must map to one placeholder pair.\ninput:\n{input}"
    );
}

#[test]
fn hs_err_native_crash_fragment() {
    let input = "#\n\
# A fatal error has been detected by the Java Runtime Environment:\n\
#  SIGSEGV (0xb) at pc=0x00007ff6a1b2c3d4, pid=18244, tid=18412\n\
# JRE version: OpenJDK Runtime Environment Temurin-17.0.9+9 (17.0.9+9)\n\
# Java VM: OpenJDK 64-Bit Server VM Temurin-17.0.9+9 windows-amd64\n\
# Problematic frame:\n\
# C  [ig75icd64.dll+0x1b2c3d4]\n\
#\n\
Core dump written to C:\\Users\\poisson.d\\AppData\\Local\\Temp\\hs_err_pid18244.mdmp";
    check_roundtrip(input);
    check_secret_removed(input, "poisson.d");
    check_survives(input, "0x00007ff6a1b2c3d4");
    check_survives(input, "17.0.9+9");
    check_survives(input, "Temurin-17.0.9+9");
}

#[test]
fn fabric_log_fragment_with_env_and_paths() {
    let input = "[main/INFO]: Loading Fabric 0.15.11 for Minecraft 1.20.1\n\
[main/INFO]: Loading 128 mods:\n\
\tfabricloader 0.15.11\n\
\tsodium 0.5.8\n\
\tiris 1.7.0+1.20.1\n\
[main/INFO]: Launching on /home/fabricmaster/.minecraft with Java 17.0.9\n\
[main/INFO]: Session: notch_dev@example.com, connecting wss://gw.example.net:443";
    check_roundtrip(input);
    check_secret_removed(input, "fabricmaster");
    check_secret_removed(input, "notch_dev@example.com");
    check_survives(input, "sodium 0.5.8");
    check_survives(input, "iris 1.7.0+1.20.1");
    check_survives(input, "Fabric 0.15.11");
}

#[test]
fn placeholder_text_is_not_the_secret_and_not_empty() {
    let input = format!("--accessToken {ACCESS_TOKEN} --uuid {PLAYER_UUID}");
    let r = redacted(&input);
    assert!(
        !r.placeholders.pairs().is_empty(),
        "secrets present but no placeholder pairs recorded.\ninput:\n{input}"
    );
    for (ph, orig) in r.placeholders.pairs() {
        assert_ne!(ph, orig, "placeholder equals the original value: {ph}");
        assert!(!ph.is_empty(), "empty placeholder for original {orig:?}");
    }
}

#[test]
fn report_counts_replacements_without_values() {
    let input = format!("--accessToken {ACCESS_TOKEN} C:\\Users\\countuser\\logs a@b.example");
    let r = redacted(&input);
    let dbg = format!("{:?}", r.report);
    assert!(
        !dbg.contains(ACCESS_TOKEN) && !dbg.contains("countuser") && !dbg.contains("a@b.example"),
        "report leaked values: {dbg}"
    );
    check_roundtrip(&input);
}

#[test]
fn greedy_secret_cannot_eat_neighboring_text() {
    // The username value "uuid" must not confuse the following flag parsing,
    // and the redacted text must still contain the surrounding flags verbatim.
    let input = "--username uuid --uuid 069a79f444e94726a5befca90e38aaf5 --version 1.20.1";
    check_roundtrip(input);
    check_secret_removed(input, "069a79f444e94726a5befca90e38aaf5");
    check_survives(input, "--version 1.20.1");
}

#[test]
fn multiline_stack_with_repeated_user_path() {
    let input = "at app//net.minecraft.server.WorldLoader.load(WorldLoader.java:45)\n\
at app//net.minecraft.server.WorldLoader.pack(WorldLoader.java:61)\n\
Caused by: java.nio.file.NoSuchFileException: C:\\Users\\repeated\\AppData\\Roaming\\.minecraft\\config\\foodeffecttooltips-client.toml\n\
and C:\\Users\\repeated\\AppData\\Roaming\\.minecraft\\config\\iris.properties";
    check_roundtrip(input);
    check_secret_removed(input, "repeated");
    check_survives(input, "foodeffecttooltips-client.toml");
}
