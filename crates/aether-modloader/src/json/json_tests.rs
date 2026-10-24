//! `json.rs` 的单元测试。

use serde_json::{json, to_value};

use super::{
    inherited_version, loader_library, opt_bool, opt_string, parse_json, safe_version_segment,
    string_field,
};
use crate::transport::MetaError;

#[test]
fn loader_library_without_repository_has_only_name() {
    let value = loader_library("net.fabricmc:fabric-loader:0.16.0", None);
    assert_eq!(value, json!({"name": "net.fabricmc:fabric-loader:0.16.0"}));
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 1);
    assert!(object.get("url").is_none());
}

#[test]
fn loader_library_with_repository_has_name_and_url() {
    let value = loader_library(
        "net.fabricmc:intermediary:1.21.1",
        Some("https://maven.fabricmc.net/"),
    );
    assert_eq!(
        value,
        json!({"name": "net.fabricmc:intermediary:1.21.1", "url": "https://maven.fabricmc.net/"})
    );
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 2);
    assert_eq!(object["name"], json!("net.fabricmc:intermediary:1.21.1"));
    assert_eq!(object["url"], json!("https://maven.fabricmc.net/"));
}

#[test]
fn inherited_version_has_expected_field_values() {
    let libraries = [
        loader_library(
            "net.fabricmc:intermediary:1.21.1",
            Some("https://maven.fabricmc.net/"),
        ),
        loader_library(
            "net.fabricmc:fabric-loader:0.16.0",
            Some("https://maven.fabricmc.net/"),
        ),
    ];
    let value = inherited_version(
        "fabric-loader-0.16.0-1.21.1",
        "1.21.1",
        "net.fabricmc.loader.impl.launch.knot.KnotClient",
        &libraries,
    );

    assert_eq!(value["id"], json!("fabric-loader-0.16.0-1.21.1"));
    assert_eq!(value["inheritsFrom"], json!("1.21.1"));
    assert_eq!(value["type"], json!("release"));
    assert_eq!(
        value["mainClass"],
        json!("net.fabricmc.loader.impl.launch.knot.KnotClient")
    );
    assert_eq!(value["libraries"], json!(libraries));
    // 没有多余字段（例如不要 `arguments`）。
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 5);
}

#[test]
fn inherited_version_round_trips_through_json_text() {
    let value = inherited_version(
        "quilt-loader-0.20.0-beta.9-1.21.1",
        "1.21.1",
        "org.quiltmc.loader.impl.launch.knot.KnotClient",
        &[loader_library(
            "org.quiltmc:quilt-loader:0.20.0-beta.9",
            None,
        )],
    );

    let rendered = serde_json::to_string(&value).unwrap();
    // 注：crate 未启用 serde_json 的 `preserve_order`，`Map` 是 `BTreeMap`，
    // 输出 key 按字典序（id, inheritsFrom, libraries, mainClass, type），
    // 所以这里断言的是内容而不是字面顺序。
    for key in ["id", "inheritsFrom", "type", "mainClass", "libraries"] {
        assert!(
            rendered.contains(&format!("\"{key}\":")),
            "missing {key}: {rendered}"
        );
    }
    let reparsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(reparsed, value);
}

#[test]
fn parse_json_accepts_valid_input() {
    let value = parse_json(
        "https://a/game.json",
        r#"[{"version":"1.21.1","stable":true}]"#,
    )
    .unwrap();
    assert_eq!(value, json!([{"version": "1.21.1", "stable": true}]));
}

#[test]
fn parse_json_rejects_invalid_input_with_invalid_json_error() {
    let err = parse_json("https://a/broken.json", "{").unwrap_err();
    match err {
        MetaError::InvalidJson { url, reason } => {
            assert_eq!(url, "https://a/broken.json");
            assert!(!reason.is_empty());
        }
        other => panic!("expected InvalidJson, got {other:?}"),
    }
}

#[test]
fn string_field_returns_present_value() {
    let value = json!({"version": "1.21.1"});
    assert_eq!(
        string_field(&value, "version", "game version").unwrap(),
        "1.21.1"
    );
}

#[test]
fn string_field_missing_or_wrong_type_is_missing_field() {
    let absent = json!({"other": 1});
    let err = string_field(&absent, "version", "game version").unwrap_err();
    assert_eq!(
        err,
        MetaError::MissingField {
            what: "game version"
        }
    );

    let wrong_type = json!({"version": 42});
    let err = string_field(&wrong_type, "version", "game version").unwrap_err();
    assert_eq!(
        err,
        MetaError::MissingField {
            what: "game version"
        }
    );

    let not_an_object = json!(["version"]);
    let err = string_field(&not_an_object, "version", "game version").unwrap_err();
    assert_eq!(
        err,
        MetaError::MissingField {
            what: "game version"
        }
    );
}

#[test]
fn opt_string_returns_none_on_type_mismatch() {
    let value = json!({"s": "text", "n": 1, "b": true, "arr": [], "obj": {}, "null": null});
    assert_eq!(opt_string(&value, "s").as_deref(), Some("text"));
    assert_eq!(opt_string(&value, "n"), None);
    assert_eq!(opt_string(&value, "b"), None);
    assert_eq!(opt_string(&value, "arr"), None);
    assert_eq!(opt_string(&value, "obj"), None);
    assert_eq!(opt_string(&value, "null"), None);
    assert_eq!(opt_string(&value, "missing"), None);
}

#[test]
fn opt_bool_returns_none_on_type_mismatch() {
    let value = json!({"t": true, "f": false, "s": "true", "n": 1, "null": null});
    assert_eq!(opt_bool(&value, "t"), Some(true));
    assert_eq!(opt_bool(&value, "f"), Some(false));
    assert_eq!(opt_bool(&value, "s"), None);
    assert_eq!(opt_bool(&value, "n"), None);
    assert_eq!(opt_bool(&value, "null"), None);
    assert_eq!(opt_bool(&value, "missing"), None);
}

#[test]
fn safe_version_segment_truth_table() {
    let accepted = [
        "fabric-loader-0.16.0-1.21.1",
        "quilt-loader-0.20.0-beta.9-1.21.1",
        "1.21.1",
        "47.4.28",
        "26.1.0.0-alpha.1",
        "a",
        "COM0",
        "LPT10",
        "console",
    ];
    for segment in accepted {
        assert!(safe_version_segment(segment), "{segment:?} should be safe");
    }

    let rejected = [
        "",
        // Windows 保留设备名（大小写不敏感，扩展名前进也算）。
        "CON", "con", "con.txt", "COM5.sys", "LPT9", "nul.json",
        // 结尾是点或空格。
        "a.", "a ", // 路径分隔符与 Windows 保留字符。
        "a/b", "a:b", "a\\b", "a<b", "a>b", "a\"b", "a|b", "a?b", "a*b",
        // 控制字符。
        "a\nb", "a\u{7f}b", "a\u{0}b",
    ];
    for segment in rejected {
        assert!(
            !safe_version_segment(segment),
            "{segment:?} should be unsafe"
        );
    }
}

#[test]
fn safe_version_segment_checks_the_stem_before_the_first_dot() {
    // 主干命中的保留名要拦住，哪怕带扩展名。
    assert!(!safe_version_segment("AUX.log"));
    // 主干不命中就放行，哪怕后面出现类似保留名的片段。
    assert!(safe_version_segment("x.CON"));
    assert!(safe_version_segment("notcon"));
}

#[test]
fn inherited_version_libraries_are_serialized_as_objects() {
    let value = inherited_version("id", "parent", "Main", &[loader_library("g:a:1", None)]);
    let array = value["libraries"].as_array().unwrap();
    assert_eq!(array.len(), 1);
    assert_eq!(to_value(&array[0]).unwrap(), json!({"name": "g:a:1"}));
}
