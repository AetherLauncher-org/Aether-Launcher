//! Offline, deliberately conservative JVM-parameter requirements.
//!
//! A missing table entry never produces a wrong warning. The authoritative
//! source for re-deriving this table is each JDK's `java --help-extra` output;
//! entries should be reviewed against the relevant JEP or JDK release.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Minimum Java major version required by a JVM argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterRequirement {
    /// The exact parameter or parameter prefix, e.g. `-XX:+UseZGC`.
    pub parameter: &'static str,
    /// The lowest Java major version that accepts it.
    pub min_java: u16,
    /// Where the requirement comes from, e.g. the JEP or the JDK release.
    pub note: &'static str,
}

/// The offline parameter table, looked up by exact match and then longest
/// declared prefix match.
pub const PARAMETER_MIN_JAVA: &[ParameterRequirement] = &[
    ParameterRequirement {
        parameter: "--sun-misc-unsafe-memory-access",
        min_java: 23,
        note: "JDK 23 / JEP 471",
    },
    ParameterRequirement {
        parameter: "--enable-preview",
        min_java: 12,
        note: "JEP 12, Java 12",
    },
    ParameterRequirement {
        parameter: "-XX:+UseZGC",
        min_java: 11,
        note: "JEP 333, Java 11",
    },
    ParameterRequirement {
        parameter: "-XX:+UseShenandoahGC",
        min_java: 12,
        note: "JEP 189, Java 12",
    },
    ParameterRequirement {
        parameter: "-XX:+ZGenerational",
        min_java: 21,
        note: "JEP 439, Java 21",
    },
    ParameterRequirement {
        parameter: "-XX:+UseCompactObjectHeaders",
        min_java: 24,
        note: "JEP 450, Java 24 (experimental)",
    },
    ParameterRequirement {
        parameter: "--add-opens",
        min_java: 9,
        note: "JEP 261, Java 9",
    },
    ParameterRequirement {
        parameter: "--add-modules",
        min_java: 9,
        note: "JEP 261, Java 9",
    },
    ParameterRequirement {
        parameter: "--add-exports",
        min_java: 9,
        note: "JEP 261, Java 9",
    },
    ParameterRequirement {
        parameter: "--patch-module",
        min_java: 9,
        note: "JEP 261, Java 9",
    },
    ParameterRequirement {
        parameter: "-XX:+UseG1GC",
        min_java: 7,
        note: "JEP 248 lineage; flag exists since Java 7u4",
    },
    ParameterRequirement {
        parameter: "-XX:MaxRAMPercentage",
        min_java: 10,
        note: "conservative: accepted since 8u191, treated as 10",
    },
    ParameterRequirement {
        parameter: "-XX:+AlwaysPreTouch",
        min_java: 1,
        note: "long-standing HotSpot flag, baseline Java 1",
    },
    ParameterRequirement {
        parameter: "-XX:+UseNUMA",
        min_java: 1,
        note: "long-standing HotSpot flag, baseline Java 1",
    },
    ParameterRequirement {
        parameter: "-Djava.library.path",
        min_java: 1,
        note: "long-standing Java system property, baseline Java 1",
    },
];

/// Splits an argument into its flag part and its value, handling both
/// `--flag=value` and `--flag value` forms.
#[must_use]
pub fn split_argument(argument: &str) -> (&str, Option<&str>) {
    argument
        .split_once('=')
        .map_or((argument, None), |(flag, value)| (flag, Some(value)))
}

/// Lowest Java major version a single argument needs, if known.
///
/// The flag is normalized by splitting at the first `=` and by converting
/// `-XX:-Name` to `-XX:+Name`. Matching tries an exact entry first, then the
/// longest declared prefix entry. A separate space-delimited value is not part
/// of `argument`; callers should pass each JVM argument token independently.
#[must_use]
pub fn min_java_for_argument(argument: &str) -> Option<u16> {
    let (flag, _) = split_argument(argument);
    let normalized = flag.strip_prefix("-XX:-");
    let normalized_storage;
    let flag = if let Some(name) = normalized {
        if name.is_empty() {
            flag
        } else {
            normalized_storage = format!("-XX:+{name}");
            normalized_storage.as_str()
        }
    } else {
        flag
    };

    PARAMETER_MIN_JAVA
        .iter()
        .find(|entry| entry.parameter == flag)
        .or_else(|| {
            PARAMETER_MIN_JAVA
                .iter()
                .filter(|entry| flag.starts_with(entry.parameter))
                .max_by_key(|entry| entry.parameter.len())
        })
        .map(|entry| entry.min_java)
}

/// A launch-before preflight finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightIssue {
    /// The argument as it appears on the command line.
    pub argument: String,
    /// The lowest Java major version that accepts it.
    pub min_java: u16,
    /// The Java the launcher selected, when known.
    pub selected_java: Option<u16>,
    /// A short Chinese explanation ready for the UI.
    pub message_zh: String,
}

/// Checks JVM arguments against the selected Java major version.
///
/// Returns one issue per distinct argument whose requirement the selected Java
/// cannot satisfy. An unknown selected version produces no issues: the
/// launcher only warns when it is sure.
#[must_use]
pub fn preflight_jvm_arguments(
    arguments: &[String],
    selected_java_major: Option<u16>,
) -> Vec<PreflightIssue> {
    let Some(selected_java) = selected_java_major else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    arguments
        .iter()
        .filter(|argument| seen.insert(argument.as_str()))
        .filter_map(|argument| {
            let min_java = min_java_for_argument(argument)?;
            (min_java > selected_java).then(|| PreflightIssue {
                argument: argument.clone(),
                min_java,
                selected_java: Some(selected_java),
                message_zh: format!(
                    "参数 {} 需要 Java {}，当前选择的是 Java {}。",
                    split_argument(argument).0,
                    min_java,
                    selected_java
                ),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "javatab_tests.rs"]
mod tests;
