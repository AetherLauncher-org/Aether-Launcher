use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

/// 规则要匹配的运行环境。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Environment {
    /// 清单里的系统名：`windows`、`linux`、`osx`
    pub os_name: String,
    /// 清单里的架构名，例如 `x86`
    pub os_arch: String,
    /// 系统版本字符串，例如 Windows 的 `10.0`。用于和系统规则里的 `version` 正则匹配；
    /// 未知时留空（空字符串）。
    pub os_version: String,
    /// 启用的特性，例如 `is_demo_user`、`has_custom_resolution`
    pub features: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    /// 正则表达式，匹配 [`Environment::os_version`]（例如 `^10\.`）。
    ///
    /// 编译失败（非法正则）视为不匹配，绝不 panic；规则本身缺少这个字段时不做版本检查。
    /// `os_version` 为空字符串时，除能匹配空串的正则外都不会匹配。
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

impl Rule {
    pub fn matches(&self, environment: &Environment) -> bool {
        let os_matches = match &self.os {
            Some(os) => os.matches(environment),
            None => true,
        };
        os_matches && features_match(&self.features, environment)
    }
}

impl OsRule {
    fn matches(&self, environment: &Environment) -> bool {
        option_matches(self.name.as_deref(), &environment.os_name)
            && option_matches(self.arch.as_deref(), &environment.os_arch)
            && version_matches(self.version.as_deref(), &environment.os_version)
    }
}

/// 没有 `version` 时不做限制；有 `version` 时按正则匹配系统版本，非法正则视为不匹配。
fn version_matches(pattern: Option<&str>, os_version: &str) -> bool {
    let Some(pattern) = pattern else {
        return true;
    };
    match Regex::new(pattern) {
        Ok(regex) => regex.is_match(os_version),
        Err(_) => false,
    }
}

fn option_matches(expected: Option<&str>, actual: &str) -> bool {
    expected.is_none_or(|value| value == actual)
}

fn features_match(required: &BTreeMap<String, bool>, environment: &Environment) -> bool {
    required
        .iter()
        .all(|(name, expected)| environment.features.contains(name) == *expected)
}

/// 没有规则时允许；有规则时，最后一条匹配的规则决定结果，一条都不匹配则不允许。
pub fn rules_allow(rules: &[Rule], environment: &Environment) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(environment) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Environment, OsRule, Rule, RuleAction, rules_allow};

    const ALLOW_ALL_EXCEPT_OSX: &str = r#"
    [
      {"action": "allow"},
      {"action": "disallow", "os": {"name": "osx"}}
    ]
    "#;

    const ALLOW_ONLY_WINDOWS: &str = r#"
    [{"action": "allow", "os": {"name": "windows"}}]
    "#;

    const ALLOW_DEMO_ONLY: &str = r#"
    [{"action": "allow", "features": {"is_demo_user": true}}]
    "#;

    const ALLOW_WINDOWS_X86: &str = r#"
    [{"action": "allow", "os": {"name": "windows", "arch": "x86"}}]
    "#;

    const DISALLOW_BY_VERSION: &str = r#"
    [
      {"action": "allow"},
      {"action": "disallow", "os": {"version": "^10\\."}}
    ]
    "#;

    const DISALLOW_BY_BAD_VERSION: &str = r#"
    [
      {"action": "allow"},
      {"action": "disallow", "os": {"version": "("}}
    ]
    "#;

    fn environment(os_name: &str, features: &[&str]) -> Environment {
        let mut environment = Environment {
            os_name: os_name.to_owned(),
            os_arch: "x86_64".to_owned(),
            ..Environment::default()
        };
        for feature in features {
            environment.features.insert((*feature).to_owned());
        }
        environment
    }

    /// 在上面的环境基础上设置系统版本字符串（例如 `10.0`）。
    fn environment_with_version(os_name: &str, os_version: &str) -> Environment {
        Environment {
            os_version: os_version.to_owned(),
            ..environment(os_name, &[])
        }
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn no_rules_means_allowed() {
        assert!(rules_allow(&[], &environment("windows", &[])));
    }

    #[test]
    fn disallow_rule_after_allow_all_excludes_that_system() {
        let rules = rules(ALLOW_ALL_EXCEPT_OSX);
        assert!(rules_allow(&rules, &environment("windows", &[])));
        assert!(!rules_allow(&rules, &environment("osx", &[])));
    }

    #[test]
    fn allow_rule_for_one_system_excludes_the_others() {
        let rules = rules(ALLOW_ONLY_WINDOWS);
        assert!(rules_allow(&rules, &environment("windows", &[])));
        assert!(!rules_allow(&rules, &environment("linux", &[])));
    }

    #[test]
    fn feature_rules_need_the_feature_to_be_enabled() {
        let rules = rules(ALLOW_DEMO_ONLY);
        let with_demo = environment("linux", &["is_demo_user"]);
        let without_demo = environment("linux", &[]);
        assert!(rules_allow(&rules, &with_demo));
        assert!(!rules_allow(&rules, &without_demo));
    }

    #[test]
    fn architecture_must_match_when_given() {
        let rules = rules(ALLOW_WINDOWS_X86);
        assert!(!rules_allow(&rules, &environment("windows", &[])));
        let mut matching = environment("windows", &[]);
        matching.os_arch = "x86".to_owned();
        assert!(rules_allow(&rules, &matching));
    }

    #[test]
    fn system_rules_with_a_version_match_the_os_version() {
        let rules = rules(DISALLOW_BY_VERSION);
        // os_version 为空时，`^10\.` 匹配不到，规则不生效，于是允许。
        assert!(rules_allow(&rules, &environment("windows", &[])));
        // Windows 10 命中正则，disallow 规则生效。
        assert!(!rules_allow(
            &rules,
            &environment_with_version("windows", "10.0")
        ));
        // Windows 7 不命中正则，仍然允许。
        assert!(rules_allow(
            &rules,
            &environment_with_version("windows", "6.1")
        ));
    }

    #[test]
    fn invalid_version_regex_does_not_match_and_does_not_panic() {
        let rules = rules(DISALLOW_BY_BAD_VERSION);
        assert!(rules_allow(
            &rules,
            &environment_with_version("windows", "10.0")
        ));
    }

    #[test]
    fn os_rule_matches_versions_directly() {
        let rule = Rule {
            action: RuleAction::Disallow,
            os: Some(OsRule {
                name: Some("windows".to_owned()),
                arch: None,
                version: Some("^10\\.".to_owned()),
            }),
            features: BTreeMap::new(),
        };

        assert!(rule.matches(&environment_with_version("windows", "10.0")));
        assert!(!rule.matches(&environment_with_version("windows", "6.1")));
        assert!(!rule.matches(&environment_with_version("linux", "10.0")));
        assert!(!rule.matches(&environment("windows", &[])));
    }
}
