//! Deterministic, first-layer redaction for Minecraft diagnostics.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

static ARG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?m)(--accessToken|--refreshToken|--clientToken|--username|--uuid|--xuid|--clientId|--userProperties)(?:(=)([^\s\r\n]+)|([^\S\r\n]+)([^\s\r\n-][^\s\r\n]*))",
).expect("valid argument regex")
});
static PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([a-z]:[\\/]+Users[\\/]|/home/|/Users/)([^\\/\r\n]+?)([\\/])")
        .expect("valid path regex")
});
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^A-Za-z0-9.+_%\-/:])([A-Za-z0-9][A-Za-z0-9.+_%\-]*@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})").expect("valid email regex")
});
static BEARER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bBearer[ \t]+([^\s<>]+)").expect("valid bearer regex"));
static JWT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").expect("valid jwt regex")
});
static IPV4_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("valid ipv4 regex"));
static SERVER_PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:wss?://)?([a-z][a-z0-9-]*(?:\.[a-z][a-z0-9-]*)+):[0-9]{1,5}")
        .expect("valid server port regex")
});
static SERVER_HOST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z][a-z0-9-]*(?:\.[a-z][a-z0-9-]*)+").expect("valid server host regex")
});
static PLACEHOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<[A-Z][A-Z_]*:[0-9]+>$").expect("valid placeholder regex"));

#[derive(Debug, Clone, Default)]
pub struct PlaceholderMap {
    pairs: Vec<(String, String)>,
}
impl PlaceholderMap {
    #[must_use]
    pub fn restore(&self, text: &str) -> String {
        if self.pairs.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0;
        while cursor < text.len() {
            let Some((ph, original)) = self
                .pairs
                .iter()
                .find(|(ph, _)| text[cursor..].starts_with(ph))
            else {
                let next = text[cursor..]
                    .chars()
                    .next()
                    .map_or(text.len(), |c| cursor + c.len_utf8());
                out.push_str(&text[cursor..next]);
                cursor = next;
                continue;
            };
            out.push_str(original);
            cursor += ph.len();
        }
        out
    }
    #[must_use]
    pub fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionReport {
    pub replacement_count: usize,
    pub by_kind: Vec<(String, usize)>,
}

#[derive(Debug, Clone)]
pub struct Redactor {
    pub text: String,
    pub placeholders: PlaceholderMap,
    pub report: RedactionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Kind {
    Token,
    Username,
    Uuid,
    Xuid,
    ClientId,
    UserProperties,
    Email,
    Ip,
    Server,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Token => "TOKEN",
            Self::Username => "USERNAME",
            Self::Uuid => "UUID",
            Self::Xuid => "XUID",
            Self::ClientId => "CLIENT_ID",
            Self::UserProperties => "USER_PROPERTIES",
            Self::Email => "EMAIL",
            Self::Ip => "IP",
            Self::Server => "SERVER",
        }
    }
}

struct RedactionBuilder {
    numbers: BTreeMap<(Kind, String), usize>,
    next: BTreeMap<Kind, usize>,
    pairs: Vec<(String, String)>,
    counts: BTreeMap<String, usize>,
}
impl RedactionBuilder {
    fn new() -> Self {
        Self {
            numbers: BTreeMap::new(),
            next: BTreeMap::new(),
            pairs: Vec::new(),
            counts: BTreeMap::new(),
        }
    }
    fn replacement(&mut self, kind: Kind, original: &str) -> String {
        let key = (kind, original.to_owned());
        let number = if let Some(n) = self.numbers.get(&key) {
            *n
        } else {
            let n = self.next.entry(kind).and_modify(|v| *v += 1).or_insert(1);
            let n = *n;
            self.numbers.insert(key, n);
            self.pairs
                .push((format!("<{}:{}>", kind.name(), n), original.to_owned()));
            n
        };
        *self.counts.entry(kind.name().to_owned()).or_default() += 1;
        format!("<{}:{}>", kind.name(), number)
    }
    fn finish(self) -> (PlaceholderMap, RedactionReport) {
        let count = self.counts.values().sum();
        (
            PlaceholderMap { pairs: self.pairs },
            RedactionReport {
                replacement_count: count,
                by_kind: self.counts.into_iter().collect(),
            },
        )
    }
}

fn replace_matches<F>(input: &str, regex: &Regex, mut replacement: F) -> String
where
    F: FnMut(&Captures<'_>, usize) -> Option<String>,
{
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    for captures in regex.captures_iter(input) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let Some(value) = replacement(&captures, whole.start()) else {
            continue;
        };
        output.push_str(&input[cursor..whole.start()]);
        output.push_str(&value);
        cursor = whole.end();
    }
    output.push_str(&input[cursor..]);
    output
}

fn is_placeholder(value: &str) -> bool {
    PLACEHOLDER_RE.is_match(value)
}
fn argument_kind(flag: &str) -> Kind {
    match flag {
        "--username" => Kind::Username,
        "--uuid" => Kind::Uuid,
        "--xuid" => Kind::Xuid,
        "--clientId" => Kind::ClientId,
        "--userProperties" => Kind::UserProperties,
        _ => Kind::Token,
    }
}

fn redact_arguments(input: &str, b: &mut RedactionBuilder) -> String {
    replace_matches(input, &ARG_RE, |c, _| {
        let flag = c.get(1)?.as_str();
        let (sep, value) = c
            .get(2)
            .zip(c.get(3))
            .map(|(a, v)| (a.as_str(), v.as_str()))
            .or_else(|| {
                c.get(4)
                    .zip(c.get(5))
                    .map(|(a, v)| (a.as_str(), v.as_str()))
            })?;
        if sep != "=" && value.starts_with('-') || is_placeholder(value) {
            return None;
        }
        Some(format!(
            "{flag}{sep}{}",
            b.replacement(argument_kind(flag), value)
        ))
    })
}
fn redact_paths(input: &str, b: &mut RedactionBuilder) -> String {
    replace_matches(input, &PATH_RE, |c, _| {
        let prefix = c.get(1)?.as_str();
        let name = c.get(2)?.as_str();
        let sep = c.get(3)?.as_str();
        if is_placeholder(name) {
            return None;
        }
        Some(format!(
            "{prefix}{}{sep}",
            b.replacement(Kind::Username, name)
        ))
    })
}
fn javaish_package_local(local: &str) -> bool {
    let s: Vec<_> = local.split('.').collect();
    s.len() >= 3
        && s.iter().all(|x| {
            !x.is_empty()
                && x.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}
fn redact_emails(input: &str, b: &mut RedactionBuilder) -> String {
    replace_matches(input, &EMAIL_RE, |c, _| {
        let boundary = c.get(1)?.as_str();
        let candidate = c.get(2)?.as_str();
        let (local, domain) = candidate.split_once('@')?;
        if domain
            .chars()
            .all(|x| x.is_ascii_digit() || matches!(x, '.' | '-'))
            || javaish_package_local(local)
        {
            return None;
        }
        Some(format!(
            "{boundary}{}",
            b.replacement(Kind::Email, candidate)
        ))
    })
}
fn redact_bearer(input: &str, b: &mut RedactionBuilder) -> String {
    replace_matches(input, &BEARER_RE, |c, _| {
        let whole = c.get(0)?.as_str();
        let token = c.get(1)?.as_str();
        if is_placeholder(token) {
            return None;
        }
        Some(format!(
            "{}{}",
            &whole[..whole.len() - token.len()],
            b.replacement(Kind::Token, token)
        ))
    })
}
fn redact_jwts(input: &str, b: &mut RedactionBuilder) -> String {
    replace_matches(input, &JWT_RE, |c, _| {
        let token = c.get(0)?.as_str();
        if token.len() < 60 || is_placeholder(token) {
            None
        } else {
            Some(b.replacement(Kind::Token, token))
        }
    })
}
fn valid_ipv4(address: &str) -> bool {
    address.split('.').count() == 4
        && address
            .split('.')
            .all(|x| x.parse::<u16>().is_ok_and(|n| n <= 255))
}
fn redact_ipv4(input: &str, b: &mut RedactionBuilder) -> String {
    replace_matches(input, &IPV4_RE, |c, start| {
        let address = c.get(0)?.as_str();
        let before = &input[..start];
        let line = before.rsplit_once('\n').map_or(before, |(_, tail)| tail);
        let prev = before.chars().next_back();
        // Lower-casing the whole prefix on every match is quadratic on a large
        // log, which turned the 1 MiB fixture into a minute of work. The only
        // suffix that matters is the literal `.jar:`, so a bounded,
        // case-insensitive comparison of the line tail replaces it.
        let looks_like_jar = line.len() >= 5
            && line.is_char_boundary(line.len() - 5)
            && line[line.len() - 5..].eq_ignore_ascii_case(".jar:");
        if !valid_ipv4(address)
            || matches!(prev, Some('@' | '/'))
            || looks_like_jar
            || line.ends_with('@')
        {
            return None;
        }
        Some(b.replacement(Kind::Ip, address))
    })
}
fn blocked_server_host(host: &str) -> bool {
    // `format!(".{x}")` per host per call allocated on the hot path; a manual
    // suffix check keeps the same semantics without allocating.
    const BLOCKED: [&str; 8] = [
        "minecraft.net",
        "mojang.com",
        "fabricmc.net",
        "forge.net",
        "neoforged.net",
        "maven.org",
        "piston-data.mojang.com",
        "libraries.minecraft.net",
    ];
    BLOCKED.iter().any(|x| {
        host.eq_ignore_ascii_case(x)
            || (host.len() > x.len()
                && host.is_char_boundary(host.len() - x.len())
                && host[host.len() - x.len()..].eq_ignore_ascii_case(x)
                && host.as_bytes()[host.len() - x.len() - 1] == b'.')
    })
}

/// File suffixes that make a dotted token a source file, not a host name.
///
/// Stack frames read `at Package.Type.method(File.java:105)`, which is
/// indistinguishable from `host:port` to a naive pattern. Treating it as a
/// server address destroys the most diagnostic part of the trace, so any
/// candidate ending in a known source/asset suffix is rejected outright.
fn looks_like_file_name(token: &str) -> bool {
    let lowered = token.to_ascii_lowercase();
    [
        ".java",
        ".class",
        ".jar",
        ".txt",
        ".log",
        ".json",
        ".toml",
        ".cfg",
        ".xml",
        ".yml",
        ".yaml",
        ".png",
        ".ogg",
        ".zip",
        ".nbt",
        ".dat",
        ".properties",
    ]
    .iter()
    .any(|suffix| lowered.ends_with(suffix))
}

/// Whether the line at `offset` is a stack frame, where no server may appear.
///
/// A stack frame is `\tat Package.Type.method(File.java:105)`: the prefix before
/// the candidate ends with an opening `(` and the line itself started with
/// `at `. A standalone address on its own line has an empty or unrelated
/// prefix and is therefore not skipped.
fn is_stack_frame_line(text: &str, offset: usize) -> bool {
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let prefix = text[line_start..offset].trim_start();
    prefix.starts_with("at ")
}

fn redact_server_addresses(input: &str, b: &mut RedactionBuilder) -> String {
    let with_ports = replace_matches(input, &SERVER_PORT_RE, |c, start| {
        let host = c.get(1)?.as_str();
        if blocked_server_host(host)
            || looks_like_file_name(host)
            || is_stack_frame_line(input, start)
        {
            return None;
        }
        Some(b.replacement(Kind::Server, c.get(0)?.as_str()))
    });
    replace_matches(&with_ports, &SERVER_HOST_RE, |c, start| {
        let host = c.get(0)?.as_str();
        if blocked_server_host(host)
            || is_placeholder(host)
            || looks_like_file_name(host)
            || is_stack_frame_line(&with_ports, start)
        {
            return None;
        }
        // Lower-casing the whole prefix per candidate is quadratic on a large
        // log. Only the current line can contain the "Connecting to" marker,
        // so scope the case-insensitive search to that line.
        let line_start = with_ports[..start].rfind('\n').map_or(0, |index| index + 1);
        let line = &with_ports[line_start..start];
        let has_marker = line
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| {
                pair[0].eq_ignore_ascii_case("connecting") && pair[1].eq_ignore_ascii_case("to")
            });
        if !has_marker {
            return None;
        }
        Some(b.replacement(Kind::Server, host))
    })
}

#[must_use]
pub fn redacted(text: &str) -> Redactor {
    let mut b = RedactionBuilder::new();
    let text = redact_arguments(text, &mut b);
    let text = redact_paths(&text, &mut b);
    let text = redact_emails(&text, &mut b);
    let text = redact_bearer(&text, &mut b);
    let text = redact_jwts(&text, &mut b);
    let text = redact_ipv4(&text, &mut b);
    let text = redact_server_addresses(&text, &mut b);
    let (placeholders, report) = b.finish();
    Redactor {
        text,
        placeholders,
        report,
    }
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
