//! Named sets of settings a config can build on with `extends`.
//!
//! A preset is a YAML fragment merged under the file that names it, so it moves
//! defaults and never overrides a value the file states. Levels are presets too: `strict`
//! is the strict level, and `relaxed`, `standard` and `pristine` resolve to theirs, so
//! one name means one set of settings whichever way it is asked for.

use crate::config::Level;

/// The presets that ship, by name. `strict` is the strict level under its other name;
/// the rest are written here.
pub const PRESETS: [(&str, &str); 5] = [
    ("recommended", include_str!("presets/recommended.yaml")),
    ("strict", Level::Strict.layer()),
    (
        "legacy-adoption",
        include_str!("presets/legacy-adoption.yaml"),
    ),
    ("ci-gate", include_str!("presets/ci-gate.yaml")),
    ("perf", include_str!("presets/perf.yaml")),
];

/// The YAML behind a name, or `None` when nothing is called that.
pub fn lookup(name: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|(preset, _)| *preset == name)
        .map(|(_, yaml)| *yaml)
        .or_else(|| Level::parse(name).map(Level::layer))
}

/// Every name `lookup` answers, for an error that says what could have been meant.
pub fn names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = PRESETS.iter().map(|(name, _)| *name).collect();
    for level in Level::ALL {
        if !names.contains(&level.slug()) {
            names.push(level.slug());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RqxConfig;

    #[test]
    fn every_preset_parses_and_names_only_things_that_exist() {
        for name in names() {
            let yaml = lookup(name).expect(name);
            let config: RqxConfig =
                serde_saphyr::from_str(yaml).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(
                config.unknown_names().is_empty(),
                "{name}: {:?}",
                config.unknown_names()
            );
        }
    }

    /// A preset is a layer of defaults. A policy pointer, a lock or a subtree override
    /// has a file to be relative to, which a preset does not have.
    #[test]
    fn a_preset_carries_nothing_that_needs_a_file_to_mean_something() {
        for name in names() {
            let config: RqxConfig = serde_saphyr::from_str(lookup(name).expect(name)).expect(name);
            assert!(config.policy.is_none(), "{name} points at a policy");
            assert!(config.locked.is_none(), "{name} locks keys");
            assert!(config.overrides.is_empty(), "{name} carries overrides");
        }
    }

    #[test]
    fn a_level_is_addressable_by_name_and_so_is_a_preset() {
        for level in Level::ALL {
            assert_eq!(lookup(level.slug()), Some(level.layer()), "{level}");
        }
        assert!(lookup("recommended").is_some());
        assert!(lookup("no-such-preset").is_none());
    }

    #[test]
    fn the_names_list_every_preset_and_every_level_once() {
        let listed = names();
        assert_eq!(listed.len(), 8, "{listed:?}");
        for expected in [
            "recommended",
            "strict",
            "legacy-adoption",
            "ci-gate",
            "perf",
            "relaxed",
            "standard",
            "pristine",
        ] {
            assert!(listed.contains(&expected), "{expected}");
        }
    }

    #[test]
    fn perf_raises_every_performance_rule_and_nothing_else() {
        use crate::rule::{Category, RuleId};
        let perf: RqxConfig = serde_saphyr::from_str(lookup("perf").expect("perf")).unwrap();
        let raised: Vec<&str> = perf.rules.options.keys().map(String::as_str).collect();
        for rule in Category::Performance.rules() {
            assert!(raised.contains(&rule.spec().slug), "{rule} is not raised");
        }
        for slug in raised {
            let rule = RuleId::parse(slug).expect("a rule");
            assert_eq!(rule.spec().category, Category::Performance, "{slug}");
        }
    }

    #[test]
    fn perf_switches_on_the_performance_rules_that_ship_off() {
        use crate::rule::{Category, RuleId};
        let perf: RqxConfig = serde_saphyr::from_str(lookup("perf").expect("perf")).unwrap();
        let off: Vec<RuleId> = Category::Performance
            .rules()
            .filter(|rule| rule.spec().off_by_default)
            .collect();
        assert!(!off.is_empty());
        for rule in off {
            assert!(perf.rules.enabled(rule), "{rule} is off under perf");
        }
    }

    #[test]
    fn ci_gate_gates_and_recommended_does_not_move_anything() {
        let gate: RqxConfig = serde_saphyr::from_str(lookup("ci-gate").expect("ci-gate")).unwrap();
        assert_eq!(gate.gate.min_health, 70);
        let defaults = RqxConfig::default();
        let recommended: RqxConfig =
            serde_saphyr::from_str(lookup("recommended").expect("recommended")).unwrap();
        assert_eq!(recommended.gate.min_health, defaults.gate.min_health);
        assert_eq!(
            recommended.thresholds.max_params,
            defaults.thresholds.max_params
        );
    }
}
