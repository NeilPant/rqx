//! `rqx config explain KEY`: what a setting is, what it defaults to, and what it is now.
//!
//! The words come from the JSON Schema, which is derived from the config types, so the
//! description here is the one an editor shows on hover and cannot drift from it. The
//! current value comes from the resolved config and the file or preset that wrote it
//! from the same provenance `rqx config` prints.

use std::io::Write;

use anyhow::{Result, bail};
use rqx_core::config::{Thresholds, json_schema};
use rqx_core::rule::RuleId;
use rqx_core::suggest;
use serde_json::Value;

use crate::brand::{Ink, Palette, paint};
use crate::config::Resolved;

/// A schema node with its reference followed: the words on the node itself win over
/// the words on what it points at, because a field's own doc comment is more specific
/// than the type's.
struct Described<'a> {
    shape: &'a Value,
    description: Option<&'a str>,
    default: Option<&'a Value>,
}

impl<'a> Described<'a> {
    fn of(node: &'a Value, root: &'a Value) -> Self {
        let shape = settle(node, root);
        Self {
            shape,
            description: node["description"]
                .as_str()
                .or_else(|| shape["description"].as_str()),
            default: node.get("default").or_else(|| shape.get("default")),
        }
    }

    fn child(&self, name: &str, root: &'a Value) -> Option<Self> {
        self.shape["properties"]
            .get(name)
            .map(|node| Self::of(node, root))
    }

    fn keys(&self) -> Vec<&'a str> {
        self.shape["properties"]
            .as_object()
            .map(|properties| properties.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// Follows `$ref`, and an optional field's `anyOf: [thing, null]` down to the thing.
fn settle<'a>(node: &'a Value, root: &'a Value) -> &'a Value {
    if let Some(reference) = node["$ref"].as_str()
        && let Some(target) = root["$defs"].get(reference.trim_start_matches("#/$defs/"))
    {
        return settle(target, root);
    }
    let alternatives = node["anyOf"].as_array();
    if let Some([only]) = alternatives
        .map(|all| {
            all.iter()
                .filter(|one| one["type"] != "null")
                .collect::<Vec<_>>()
        })
        .as_deref()
    {
        return settle(only, root);
    }
    node
}

pub fn run(key: &str, settings: &Resolved, palette: Palette, out: &mut dyn Write) -> Result<()> {
    let schema = json_schema();
    let mut at = Described::of(&schema, &schema);
    let mut walked = Vec::new();
    for part in key.split('.') {
        let Some(next) = at.child(part, &schema) else {
            bail!("{}", unknown(key, &walked, part, &at));
        };
        walked.push(part);
        at = next;
    }

    let shown = rendered(key, &at, settings);
    let (bold, dim) = (palette.bold(Ink::Text), palette.style(Ink::Muted));
    writeln!(out, "\n  {}", paint(bold, key))?;
    if let Some(description) = at.description {
        for line in description.lines() {
            writeln!(out, "    {line}")?;
        }
    }
    writeln!(out)?;
    writeln!(
        out,
        "    {}  {}",
        paint(dim, "type   "),
        type_of(&at, &schema)
    )?;
    if let Some(default) = &shown.default {
        writeln!(out, "    {}  {default}", paint(dim, "default"))?;
    }
    if let Some(current) = &shown.current {
        writeln!(
            out,
            "    {}  {}  {}",
            paint(dim, "current"),
            paint(palette.style(Ink::Accent), current),
            paint(dim, &shown.source)
        )?;
    }
    let keys = at.keys();
    if !keys.is_empty() {
        writeln!(out, "    {}  {}", paint(dim, "keys   "), keys.join(", "))?;
    }
    writeln!(out)?;
    Ok(())
}

fn unknown(key: &str, walked: &[&str], part: &str, at: &Described<'_>) -> String {
    let keys = at.keys();
    let here = if walked.is_empty() {
        "the config".to_owned()
    } else {
        format!("`{}`", walked.join("."))
    };
    let hint = suggest::closest(part, keys.iter().copied()).map_or_else(String::new, |close| {
        let corrected: Vec<&str> = walked.iter().copied().chain([close]).collect();
        format!(" Did you mean `{}`?", corrected.join("."))
    });
    let listed = if keys.is_empty() {
        format!("{here} has no keys beneath it")
    } else {
        format!("{here} holds {}", keys.join(", "))
    };
    format!("`{key}` is not a setting: `{part}` is not one of its keys.{hint} {listed}.")
}

/// The default, the value in force, and where it came from.
struct Shown {
    default: Option<String>,
    current: Option<String>,
    source: String,
}

fn rendered(key: &str, at: &Described<'_>, settings: &Resolved) -> Shown {
    let default = rule_default(key).or_else(|| at.default.map(compact));
    let current = settings_value(key, settings).map(|value| compact(&value));
    let current = current.or_else(|| default.clone());
    let source = match settings.origin_of_path(key) {
        Some(origin) => origin.label(),
        None if current != default => format!("level:{}", settings.level()),
        None => "default".to_owned(),
    };
    Shown {
        default,
        current,
        source,
    }
}

fn settings_value(key: &str, settings: &Resolved) -> Option<Value> {
    let mut at = serde_json::to_value(&settings.config).ok()?;
    for part in key.split('.') {
        at = at.get(part)?.clone();
    }
    Some(at)
}

/// What a rule's own option falls back to when the config says nothing: the
/// catalogue's severity and confidence, and whether the rule ships on.
fn rule_default(key: &str) -> Option<String> {
    let mut parts = key.split('.');
    let (Some("rules"), Some(slug), Some(option), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let rule = RuleId::parse(slug)?;
    let spec = rule.spec();
    match option {
        "severity" => Some(spec.severity.label().to_owned()),
        "confidence" => serde_json::to_value(spec.confidence)
            .ok()
            .map(|value| compact(&value)),
        "enabled" => Some((!spec.off_by_default).to_string()),
        "paths" => Some("[]".to_owned()),
        threshold => Some(Thresholds::default_of(threshold)?.to_string()),
    }
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn type_of(at: &Described<'_>, root: &Value) -> String {
    let shape = at.shape;
    if let Some(choices) = choices_of(shape) {
        return format!("one of {}", choices.join(", "));
    }
    match &shape["type"] {
        Value::String(name) => describe(name, shape, root),
        Value::Array(names) => {
            let named: Vec<&str> = names
                .iter()
                .filter_map(Value::as_str)
                .filter(|name| *name != "null")
                .collect();
            named.first().map_or_else(
                || "value".to_owned(),
                |name| format!("{}, or unset", describe(name, shape, root)),
            )
        }
        _ => "value".to_owned(),
    }
}

fn describe(name: &str, shape: &Value, root: &Value) -> String {
    match name {
        "integer" if shape["minimum"] == 0 => "whole number, 0 or more".to_owned(),
        "integer" => "whole number".to_owned(),
        "array" => {
            let items = Described::of(&shape["items"], root);
            let inner = choices_of(items.shape)
                .map(|choices| format!("one of {}", choices.join(", ")))
                .unwrap_or_else(|| type_of(&items, root));
            format!("list of {inner}")
        }
        "object" if shape["properties"].is_object() => "mapping".to_owned(),
        "object" => "mapping".to_owned(),
        other => other.to_owned(),
    }
}

/// The values of an enum, whether schemars wrote it as `enum` or, when its variants
/// carry doc comments, as a list of `const`s.
fn choices_of(shape: &Value) -> Option<Vec<String>> {
    if let Some(all) = shape["enum"].as_array() {
        return Some(all.iter().map(compact).collect());
    }
    let alternatives = shape["oneOf"].as_array()?;
    let consts: Vec<String> = alternatives
        .iter()
        .filter_map(|one| one.get("const").map(compact))
        .collect();
    (consts.len() == alternatives.len()).then_some(consts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::{ColorDepth, Palette};
    use rqx_core::RqxConfig;

    fn explained(key: &str, config: RqxConfig) -> Result<String> {
        let mut out = Vec::new();
        run(
            key,
            &Resolved::of(config),
            Palette::with_depth(ColorDepth::Monochrome),
            &mut out,
        )?;
        Ok(String::from_utf8(out).expect("utf-8"))
    }

    #[test]
    fn a_threshold_says_what_it_is_what_it_defaults_to_and_what_it_is() {
        let mut config = RqxConfig::default();
        config.thresholds.max_params = 6;
        let said = explained("thresholds.max_params", config).expect("explains");
        assert!(said.contains("Parameters one function may take"), "{said}");
        assert!(said.contains("whole number, 0 or more"), "{said}");
        assert!(said.contains("default  4"), "{said}");
        assert!(said.contains("current  6"), "{said}");
    }

    #[test]
    fn an_enum_lists_its_choices() {
        let said = explained("gate.fail_on", RqxConfig::default()).expect("explains");
        assert!(
            said.contains("one of error, warning, info, never"),
            "{said}"
        );
        assert!(said.contains("default  error"), "{said}");
    }

    #[test]
    fn the_fix_confidence_floor_explains_itself() {
        let said = explained("fix.min_confidence", RqxConfig::default()).expect("explains");
        assert!(said.contains("lowest finding confidence"), "{said}");
        assert!(said.contains("one of low, heuristic, verified"), "{said}");
        assert!(said.contains("default  verified"), "{said}");
    }

    #[test]
    fn a_table_lists_the_keys_beneath_it() {
        let said = explained("gate", RqxConfig::default()).expect("explains");
        assert!(said.contains("keys"), "{said}");
        assert!(
            said.contains("fail_on") && said.contains("min_health"),
            "{said}"
        );
    }

    #[test]
    fn a_rules_own_option_falls_back_to_the_catalogue() {
        let said =
            explained("rules.unreachable-code.severity", RqxConfig::default()).expect("explains");
        assert!(said.contains("default  error"), "{said}");
        assert!(said.contains("current  error"), "{said}");

        let threshold = explained("rules.long-parameter-list.max_params", RqxConfig::default())
            .expect("explains");
        assert!(threshold.contains("default  4"), "{threshold}");
    }

    #[test]
    fn a_misspelt_key_is_refused_with_the_one_it_was_close_to() {
        let error = explained("thresholds.max_param", RqxConfig::default())
            .expect_err("refused")
            .to_string();
        assert!(
            error.contains("Did you mean `thresholds.max_params`?"),
            "{error}"
        );
        let error = explained("treshold", RqxConfig::default())
            .expect_err("refused")
            .to_string();
        assert!(error.contains("Did you mean `thresholds`?"), "{error}");
    }

    #[test]
    fn an_optional_table_is_followed_to_its_keys() {
        let said = explained("locked.keys", RqxConfig::default()).expect("explains");
        assert!(said.contains("list of string"), "{said}");
    }
}
