//! 版本 JSON 的构造与解析辅助（`pub(crate)`）。
//!
//! 供 fabric / quilt / forge / neoforge / installer 各模块共用：
//!
//! - [`loader_library`] 构造单条 `library`（Maven 坐标 + 可选仓库 URL）；
//! - [`inherited_version`] 构造继承型版本清单（`id` / `inheritsFrom` / `type` /
//!   `mainClass` / `libraries`）；
//! - [`parse_json`] 把响应文本解析成 [`serde_json::Value`]，失败转
//!   [`MetaError::InvalidJson`]；
//! - [`string_field`] / [`opt_string`] / [`opt_bool`] 是取值 helper，区分「缺字段」
//!   与「类型不对」；
//! - [`safe_version_segment`] 校验版本 id 的**单个路径段**，挡住目录穿越与
//!   Windows 保留设备名。

use serde_json::{Map, Value, json};

use crate::transport::MetaError;

/// Windows 保留设备名的主干（不含扩展名，比较时大小写不敏感）。
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 路径段里不允许出现的字符（Windows 文件名保留字符 + 路径分隔符）。
const FORBIDDEN_CHARS: [char; 9] = [':', '<', '>', '"', '|', '?', '*', '/', '\\'];

/// 构造一条加载器库：`{"name": coordinate}`，给了仓库就变成
/// `{"name": coordinate, "url": repository}`。
///
/// 真实 profile 里的库常常只有 `name` + `url`（没有 `downloads`），启动器按
/// Maven 坐标自行拼下载地址。
pub(crate) fn loader_library(coordinate: &str, repository: Option<&str>) -> Value {
    match repository {
        Some(url) => json!({ "name": coordinate, "url": url }),
        None => json!({ "name": coordinate }),
    }
}

/// 构造继承型版本清单。
///
/// 逻辑上 key 顺序是 `id` → `inheritsFrom` → `type` → `mainClass` → `libraries`；
/// 但本 crate 的 `serde_json` **没有开启 `preserve_order`**，`Map` 实际是
/// `BTreeMap`，序列化时 key 按字典序输出（`id, inheritsFrom, libraries,
/// mainClass, type`）。这里仍按逻辑顺序插入以保持可读，读者不要依赖字面顺序。
pub(crate) fn inherited_version(
    id: &str,
    inherits_from: &str,
    main_class: &str,
    libraries: &[Value],
) -> Value {
    inherited_version_with_arguments(id, inherits_from, main_class, libraries, Value::Null)
}

/// 同 [`inherited_version`]，但可以带上 `arguments`（例如 Fabric 的
/// `-DFabricMcEmu= net.minecraft.client.main.Main`）。
///
/// `arguments` 为 `Value::Null` 时不写这个字段 —— 空对象会让启动器误以为
/// 「有参数但为空」，覆盖掉被继承版本自己的参数。
pub(crate) fn inherited_version_with_arguments(
    id: &str,
    inherits_from: &str,
    main_class: &str,
    libraries: &[Value],
    arguments: Value,
) -> Value {
    // 注：`Map` 的插入顺序不保证与输出顺序一致（见函数文档）。
    let mut map = Map::new();
    map.insert("id".to_owned(), Value::String(id.to_owned()));
    map.insert(
        "inheritsFrom".to_owned(),
        Value::String(inherits_from.to_owned()),
    );
    map.insert("type".to_owned(), Value::String("release".to_owned()));
    map.insert("mainClass".to_owned(), Value::String(main_class.to_owned()));
    if !arguments.is_null() {
        map.insert("arguments".to_owned(), arguments);
    }
    map.insert("libraries".to_owned(), Value::Array(libraries.to_vec()));
    Value::Object(map)
}

/// 解析 JSON 文本，失败时带上 `url_hint` 返回 [`MetaError::InvalidJson`]。
pub(crate) fn parse_json(url_hint: &str, input: &str) -> Result<Value, MetaError> {
    serde_json::from_str(input).map_err(|err| MetaError::InvalidJson {
        url: url_hint.to_owned(),
        reason: err.to_string(),
    })
}

/// 取必需字符串字段：字段缺失**或**类型不是字符串都返回
/// [`MetaError::MissingField`]（`what` 由调用方给出人类可读名称）。
pub(crate) fn string_field<'a>(
    v: &'a Value,
    key: &str,
    what: &'static str,
) -> Result<&'a str, MetaError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or(MetaError::MissingField { what })
}

/// 取可选字符串字段：只有 JSON 字符串才算数，其它类型一律 `None`。
pub(crate) fn opt_string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// 取可选布尔字段：只有 JSON 布尔才算数，其它类型一律 `None`。
pub(crate) fn opt_bool(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

/// 判断版本 id 的单个路径段能否安全地当文件名用。
///
/// 要求：非空；不含 `: < > " | ? * / \`；不含 ASCII 控制字符；不以 `.` 或空格
/// 结尾；第一个 `.` 之前的主干不是 Windows 保留设备名（`CON` `PRN` `AUX` `NUL`
/// `COM1`-`COM9` `LPT1`-`LPT9`，大小写不敏感，所以 `con.txt` 也会被拦住）。
///
/// 段落里的 `-` 和内部 `.` 是合法的，`fabric-loader-0.16.0-1.21.1` 这种真实 id
/// 必须通过。
pub(crate) fn safe_version_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    if segment.contains(FORBIDDEN_CHARS) {
        return false;
    }
    if segment.chars().any(|c| c.is_ascii_control()) {
        return false;
    }
    if segment.ends_with('.') || segment.ends_with(' ') {
        return false;
    }
    let stem = segment.split_once('.').map_or(segment, |(head, _)| head);
    !RESERVED_STEMS
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

#[cfg(test)]
#[path = "json/json_tests.rs"]
mod tests;
