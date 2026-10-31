use super::*;

const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
const HASH: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

fn encode(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | (u32::from(*byte) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
        for _ in chunk.len()..3 {
            out.push('=');
        }
    }
    out
}

fn profile_with(textures: &str) -> String {
    format!(
        r#"{{"id":"{UUID}","name":"Notch","properties":[{{"name":"textures","value":"{}"}}]}}"#,
        encode(textures)
    )
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

#[test]
fn base64_is_decoded_with_or_without_padding() {
    for text in ["", "a", "ab", "abc", "abcd", "hello world", "{\"a\":1}"] {
        let encoded = encode(text);
        assert_eq!(base64_decode(&encoded, 100).unwrap(), text.as_bytes(), "{text}");
        assert_eq!(
            base64_decode(encoded.trim_end_matches('='), 100).unwrap(),
            text.as_bytes()
        );
    }
}

#[test]
fn bad_base64_is_refused() {
    assert_eq!(base64_decode("ab!d", 100), None);
    assert_eq!(base64_decode("a", 100), None);
    assert_eq!(base64_decode("aGVsbG8gd29ybGQ=", 5), None);
    assert_eq!(base64_decode("a b c", 100), None);
}

#[test]
fn only_a_lower_case_uuid_makes_a_profile_url() {
    assert_eq!(
        profile_url(UUID).as_deref(),
        Some("https://sessionserver.mojang.com/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5")
    );
    for bad in ["", "../x", "069A79F444E94726A5BEFCA90E38AAF5", "069a79f4-44e9-4726-a5be-fca90e38aaf5", "069a79f444e94726a5befca90e38aaf"] {
        assert_eq!(profile_url(bad), None, "{bad}");
    }
}

#[test]
fn texture_addresses_must_be_exactly_the_texture_host() {
    let good = format!("http://textures.minecraft.net/texture/{HASH}");
    assert_eq!(
        trusted_texture_url(&good).as_deref(),
        Some(format!("https://textures.minecraft.net/texture/{HASH}").as_str())
    );
    assert!(trusted_texture_url(&good.replace("http://", "https://")).is_some());
    for bad in [
        format!("http://textures.minecraft.net.evil.test/texture/{HASH}"),
        format!("http://textures.minecraft.net@evil.test/texture/{HASH}"),
        format!("http://textures.minecraft.net:8080/texture/{HASH}"),
        format!("http://evil.test/textures.minecraft.net/texture/{HASH}"),
        format!("http://textures.minecraft.net/other/{HASH}"),
        format!("http://textures.minecraft.net/texture/{HASH}?x=1"),
        format!("http://textures.minecraft.net/texture/{HASH}/../x"),
        "http://textures.minecraft.net/texture/".to_owned(),
        "http://textures.minecraft.net/texture/ZZ".to_owned(),
        format!("ftp://textures.minecraft.net/texture/{HASH}"),
        String::new(),
    ] {
        assert_eq!(trusted_texture_url(&bad), None, "{bad}");
    }
}

#[test]
fn the_skin_address_is_found_in_the_profile() {
    let body = profile_with(&format!(
        r#"{{"timestamp":1,"textures":{{"SKIN":{{"url":"http://textures.minecraft.net/texture/{HASH}"}}}}}}"#
    ));
    assert_eq!(
        skin_url_from_profile(&body).unwrap().as_deref(),
        Some(format!("https://textures.minecraft.net/texture/{HASH}").as_str())
    );
}

#[test]
fn a_player_without_a_skin_has_none() {
    assert_eq!(skin_url_from_profile(&profile_with(r#"{"textures":{}}"#)), Ok(None));
    assert_eq!(skin_url_from_profile(&profile_with(r#"{}"#)), Ok(None));
    assert_eq!(
        skin_url_from_profile(&format!(r#"{{"id":"{UUID}","name":"x","properties":[]}}"#)),
        Ok(None)
    );
}

#[test]
fn a_skin_on_another_host_is_not_returned() {
    let body = profile_with(r#"{"textures":{"SKIN":{"url":"http://evil.test/texture/abc"}}}"#);
    assert_eq!(skin_url_from_profile(&body), Ok(None));
}

#[test]
fn a_damaged_or_oversized_profile_is_refused() {
    assert_eq!(skin_url_from_profile("nope"), Err(ContentError::Malformed));
    assert_eq!(
        skin_url_from_profile(r#"{"properties":[{"name":"textures","value":"!!!!"}]}"#),
        Err(ContentError::Malformed)
    );
    assert_eq!(
        skin_url_from_profile(&profile_with("not json")),
        Err(ContentError::Malformed)
    );
    let huge = format!(r#"{{"x":"{}"}}"#, "a".repeat(MAX_PROFILE_BYTES));
    assert_eq!(
        skin_url_from_profile(&huge),
        Err(ContentError::TooLarge(MAX_PROFILE_BYTES))
    );
}

#[test]
fn a_skin_picture_must_be_a_small_png_of_the_right_size() {
    assert_eq!(validate_skin_png(&png(64, 64)), Ok(()));
    assert_eq!(validate_skin_png(&png(64, 32)), Ok(()));
    for (width, height) in [(128, 128), (64, 63), (32, 32), (0, 0), (64, 4096), (65_536, 65_536)] {
        assert_eq!(validate_skin_png(&png(width, height)), Err(ContentError::Malformed));
    }
}

#[test]
fn a_skin_picture_that_is_not_a_png_is_refused() {
    assert_eq!(validate_skin_png(b""), Err(ContentError::Malformed));
    assert_eq!(validate_skin_png(b"<svg/>"), Err(ContentError::Malformed));
    let mut jpeg = vec![0xFF, 0xD8, 0xFF];
    jpeg.resize(100, 0);
    assert_eq!(validate_skin_png(&jpeg), Err(ContentError::Malformed));
    let mut wrong_chunk = png(64, 64);
    wrong_chunk[12..16].copy_from_slice(b"IDAT");
    assert_eq!(validate_skin_png(&wrong_chunk), Err(ContentError::Malformed));
    let mut big = png(64, 64);
    big.resize(MAX_SKIN_BYTES + 1, 0);
    assert_eq!(validate_skin_png(&big), Err(ContentError::TooLarge(MAX_SKIN_BYTES)));
}
