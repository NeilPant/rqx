//! What a config may say about one rule: `rules.<slug>` with a severity, a switch, the
//! paths it applies to, a confidence and the thresholds it reads.
//!
//! These are read by hand rather than derived because the error a typo deserves cannot
//! come out of a derive. A misspelt `disabld:` under `rules` would otherwise read as a
//! rule called `disabld` with a list for options, and say so in serde's words. Here it
//! says what the key could have been.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use globset::{Glob, GlobSet, GlobSetBuilder};
use schemars::JsonSchema;
use serde::de::{self, DeserializeSeed, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::{Rules, Thresholds};
use crate::finding::{Confidence, Finding, Severity};
use crate::rule::{Category, RuleId};
use crate::suggest;

/// The keys of `rules` that are lists of names. Everything else under it is a rule.
pub const LIST_KEYS: [&str; 3] = ["disabled", "enabled", "categories"];

/// The keys of a rule's options that are not a threshold.
pub const OPTION_KEYS: [&str; 4] = ["severity", "enabled", "paths", "confidence"];

/// Settings for one rule. Anything a rule's own threshold would say belongs here too:
/// `max_params: 6` under `long-parameter-list` is the threshold of that name, written
/// where the reader is already looking at the rule.
#[derive(Clone, Debug, Default, Hash, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleOptions {
    /// Report this rule's findings at this severity instead of the catalogue's.
    pub severity: Option<Severity>,
    /// `false` switches the rule off and `true` switches it on, even when it ships
    /// off. It is the most specific word on the rule, so it wins over the rule being
    /// named in `disabled` or `enabled`, or filtered out by `categories`.
    pub enabled: Option<bool>,
    /// Globs, relative to the scanned directory. When given, the rule only reports in
    /// files they match.
    pub paths: Vec<String>,
    /// Report this rule's findings at this confidence. Only `verified` findings count
    /// towards the health score.
    pub confidence: Option<Confidence>,
    /// Thresholds this rule reads, by key. The loader moves them into `thresholds`
    /// before the config is built, so this is empty in a resolved one.
    #[serde(flatten)]
    #[schemars(with = "BTreeMap<String, u32>")]
    pub thresholds: BTreeMap<String, u32>,
    #[serde(skip)]
    #[schemars(skip)]
    matcher: PathMatcher,
}

impl RuleOptions {
    /// Whether a finding in `file` is one this rule was asked to report.
    pub fn covers(&self, file: &str) -> bool {
        self.matcher.0.as_ref().is_none_or(|set| set.is_match(file))
    }

    fn with_paths(mut self, paths: Vec<String>) -> Result<Self, globset::Error> {
        let mut builder = GlobSetBuilder::new();
        for glob in &paths {
            builder.add(Glob::new(glob)?);
        }
        self.matcher = if paths.is_empty() {
            PathMatcher(None)
        } else {
            PathMatcher(Some(Arc::new(builder.build()?)))
        };
        self.paths = paths;
        Ok(self)
    }
}

/// The compiled form of `paths`. Derived from it, so it adds nothing to a hash.
#[derive(Clone, Default)]
struct PathMatcher(Option<Arc<GlobSet>>);

impl fmt::Debug for PathMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() { "compiled" } else { "none" })
    }
}

impl Hash for PathMatcher {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl Rules {
    fn options_of(&self, rule: RuleId) -> Option<&RuleOptions> {
        self.options.get(rule.spec().slug)
    }

    /// Whether the rule's own switch overrides the lists, and which way.
    pub(crate) fn switched(&self, rule: RuleId) -> Option<bool> {
        self.options_of(rule).and_then(|options| options.enabled)
    }

    /// Whether a finding of this rule in this file is wanted: the rule is on, and the
    /// file is one its `paths` allow.
    pub fn admits(&self, rule: RuleId, file: &str) -> bool {
        self.enabled(rule)
            && self
                .options_of(rule)
                .is_none_or(|options| options.covers(file))
    }

    /// Applies the severity and confidence the config asked for.
    pub fn adjust(&self, finding: &mut Finding) {
        let Some(options) = self.options_of(finding.rule) else {
            return;
        };
        if let Some(severity) = options.severity {
            finding.severity = severity;
        }
        if let Some(confidence) = options.confidence {
            finding.confidence = confidence;
        }
    }
}

impl<'de> Deserialize<'de> for Rules {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RulesVisitor)
    }
}

struct RulesVisitor;

impl<'de> Visitor<'de> for RulesVisitor {
    type Value = Rules;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a mapping of `disabled`, `enabled`, `categories` and rule slugs")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Rules, A::Error> {
        let mut rules = Rules::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "disabled" => rules.disabled = map.next_value()?,
                "enabled" => rules.enabled = map.next_value()?,
                "categories" => rules.categories = map.next_value()?,
                slug => {
                    let rule = RuleId::parse(slug)
                        .ok_or_else(|| de::Error::custom(unknown_rule_key(slug)))?;
                    let options = map.next_value_seed(OptionsOf(rule))?;
                    rules.options.insert(slug.to_owned(), options);
                }
            }
        }
        Ok(rules)
    }
}

fn unknown_rule_key(key: &str) -> String {
    if Category::parse(key).is_some() {
        return format!(
            "`{key}` is a family, not a rule. Name a family in `categories`, `disabled` or \
             `enabled`, and write `rules.<rule>` only for a single rule"
        );
    }
    let known = LIST_KEYS
        .into_iter()
        .chain(RuleId::ALL.iter().map(|rule| rule.spec().slug));
    match suggest::closest(key, known) {
        Some(close) => format!("`{key}` is not a rule. Did you mean `{close}`?"),
        None => format!(
            "`{key}` is not a rule, and `rules` holds `disabled`, `enabled`, `categories` \
             and rule slugs. `rqx rules` lists every rule"
        ),
    }
}

/// Reads one rule's options, knowing which rule it is so a threshold can be checked
/// against the ones that rule reads.
struct OptionsOf(RuleId);

impl<'de> DeserializeSeed<'de> for OptionsOf {
    type Value = RuleOptions;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<RuleOptions, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for OptionsOf {
    type Value = RuleOptions;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the options of `{}`", self.0.spec().slug)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RuleOptions, A::Error> {
        let mut options = RuleOptions::default();
        let mut paths = Vec::new();
        let tuned = Thresholds::tuned_by(self.0);
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "severity" => options.severity = map.next_value()?,
                "enabled" => options.enabled = map.next_value()?,
                "confidence" => options.confidence = map.next_value()?,
                "paths" => paths = map.next_value()?,
                threshold if tuned.contains(&threshold) => {
                    options
                        .thresholds
                        .insert(threshold.to_owned(), map.next_value()?);
                }
                other => return Err(de::Error::custom(self.unknown_option(other, &tuned))),
            }
        }
        options
            .with_paths(paths)
            .map_err(|error| de::Error::custom(format!("`paths` holds an invalid glob: {error}")))
    }
}

impl OptionsOf {
    fn unknown_option(&self, key: &str, tuned: &[&'static str]) -> String {
        let slug = self.0.spec().slug;
        let known: Vec<&str> = OPTION_KEYS
            .into_iter()
            .chain(tuned.iter().copied())
            .collect();
        let hint = suggest::closest(key, known.iter().copied())
            .map_or_else(String::new, |close| format!(" Did you mean `{close}`?"));
        let reads = if tuned.is_empty() {
            "it reads no threshold".to_owned()
        } else {
            format!("its thresholds are {}", tuned.join(", "))
        };
        format!(
            "`{key}` is not an option of `{slug}`.{hint} Options are {}; {reads}",
            OPTION_KEYS.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RqxConfig;

    fn rules(yaml: &str) -> Result<Rules, String> {
        serde_saphyr::from_str(yaml).map_err(|error| error.to_string())
    }

    #[test]
    fn a_rule_takes_a_severity_a_switch_paths_and_a_confidence() {
        let parsed = rules(
            "disabled: [magic-number]\nlong-function:\n  severity: error\n  confidence: heuristic\n  paths: [\"src/core/**\"]\n  enabled: true\n",
        )
        .expect("parses");
        let options = &parsed.options["long-function"];
        assert_eq!(options.severity, Some(Severity::Error));
        assert_eq!(options.confidence, Some(Confidence::Heuristic));
        assert!(options.covers("src/core/a.ts"));
        assert!(!options.covers("src/ui/a.ts"));
        assert!(parsed.disabled.contains("magic-number"));
    }

    #[test]
    fn a_rule_may_carry_the_threshold_it_reads_and_only_that_one() {
        let parsed = rules("long-parameter-list:\n  max_params: 6\n").expect("parses");
        assert_eq!(
            parsed.options["long-parameter-list"].thresholds["max_params"],
            6
        );

        let error = rules("long-parameter-list:\n  nesting_depth: 6\n").expect_err("refused");
        assert!(
            error.contains("not an option of `long-parameter-list`"),
            "{error}"
        );
        assert!(error.contains("max_params"), "{error}");
    }

    #[test]
    fn a_misspelt_key_under_rules_says_what_it_could_have_been() {
        let error = rules("disabld: [magic-number]\n").expect_err("refused");
        assert!(error.contains("Did you mean `disabled`?"), "{error}");
        let error = rules("long-functon:\n  severity: error\n").expect_err("refused");
        assert!(error.contains("Did you mean `long-function`?"), "{error}");
    }

    #[test]
    fn a_misspelt_option_says_what_it_could_have_been() {
        let error = rules("long-function:\n  severty: error\n").expect_err("refused");
        assert!(error.contains("Did you mean `severity`?"), "{error}");
    }

    #[test]
    fn a_family_is_pointed_at_the_lists_rather_than_taken_for_a_rule() {
        let error = rules("size:\n  severity: error\n").expect_err("refused");
        assert!(error.contains("is a family"), "{error}");
    }

    #[test]
    fn a_glob_that_does_not_compile_is_refused_where_it_is_written() {
        let error = rules("long-function:\n  paths: [\"src/[\"]\n").expect_err("refused");
        assert!(error.contains("invalid glob"), "{error}");
    }

    #[test]
    fn the_rules_own_switch_wins_over_the_lists_and_paths_narrow_it() {
        let parsed = rules(
            "disabled: [long-function]\nlong-function:\n  enabled: true\n  paths: [\"src/**\"]\nmagic-number:\n  enabled: false\n",
        )
        .expect("parses");
        assert!(parsed.enabled(RuleId::LongFunction));
        assert!(parsed.admits(RuleId::LongFunction, "src/a.ts"));
        assert!(!parsed.admits(RuleId::LongFunction, "lib/a.ts"));
        assert!(!parsed.enabled(RuleId::MagicNumber));
    }

    #[test]
    fn a_rule_that_ships_off_is_switched_on_by_its_own_option() {
        let rule = RuleId::ALL
            .iter()
            .copied()
            .find(|rule| rule.spec().off_by_default)
            .expect("a rule that ships off");
        let parsed = rules(&format!("{}:\n  enabled: true\n", rule.spec().slug)).expect("parses");
        assert!(parsed.enabled(rule));
        assert!(!Rules::default().enabled(rule));
    }

    #[test]
    fn severity_and_confidence_are_applied_to_a_finding() {
        use crate::finding::{Evidence, SourceRange};
        let parsed =
            rules("long-function:\n  severity: error\n  confidence: low\n").expect("parses");
        let mut finding = Finding {
            rule: RuleId::LongFunction,
            location: SourceRange {
                file: "a.ts".into(),
                start: 0,
                end: 1,
                line: 1,
                column: 1,
                end_line: 1,
            },
            severity: Severity::Info,
            confidence: Confidence::Verified,
            evidence: Evidence::StatementCount {
                count: 50,
                threshold: 40,
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
        };
        parsed.adjust(&mut finding);
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.confidence, Confidence::Low);
    }

    #[test]
    fn options_inside_a_whole_config_are_read_and_change_the_hash() {
        use std::hash::{DefaultHasher, Hash as _};
        let hash = |config: &RqxConfig| {
            let mut hasher = DefaultHasher::new();
            config.rules.hash(&mut hasher);
            hasher.finish()
        };
        let plain: RqxConfig = serde_saphyr::from_str("rules:\n  disabled: []\n").expect("plain");
        let tuned: RqxConfig =
            serde_saphyr::from_str("rules:\n  long-function:\n    severity: error\n")
                .expect("tuned");
        assert_ne!(hash(&plain), hash(&tuned), "the cache key must see it");
    }
}
