//! A threshold written under a rule is the threshold of that name.
//!
//! `rules.long-parameter-list.max_params: 6` and `thresholds.max_params: 6` say the
//! same thing, and the engine reads the second. Rather than teaching every rule to look
//! in two places, the per-rule form is moved into `thresholds` as each file is read, so
//! layering, locks and the `from` column all see one key.

use anyhow::{Result, bail};
use rqx_core::rule_options::{LIST_KEYS, OPTION_KEYS};
use serde_json::{Map, Value};

/// Moves every threshold under `rules.<rule>` into `thresholds` of the same file or
/// override entry. Two different values for one key in one place is an error: which
/// wins would depend on an order nobody wrote down.
pub fn thresholds(value: &mut Value, source: &str) -> Result<()> {
    let moved = take_rule_thresholds(value);
    if moved.is_empty() {
        return Ok(());
    }
    let Some(table) = value.as_object_mut() else {
        return Ok(());
    };
    let thresholds = table
        .entry("thresholds")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(thresholds) = thresholds.as_object_mut() else {
        return Ok(());
    };
    for (key, number, rule) in moved {
        if let Some(existing) = thresholds.get(&key)
            && *existing != number
        {
            bail!(
                "{source} sets `{key}` to {existing} under `thresholds` and to {number} \
                 under `rules.{rule}`. Keep one of them."
            );
        }
        thresholds.insert(key, number);
    }
    Ok(())
}

fn take_rule_thresholds(value: &mut Value) -> Vec<(String, Value, String)> {
    let Some(rules) = value.get_mut("rules").and_then(Value::as_object_mut) else {
        return Vec::new();
    };
    let mut moved = Vec::new();
    for (rule, options) in rules.iter_mut() {
        if LIST_KEYS.contains(&rule.as_str()) {
            continue;
        }
        let Some(options) = options.as_object_mut() else {
            continue;
        };
        let keys: Vec<String> = options
            .keys()
            .filter(|key| !OPTION_KEYS.contains(&key.as_str()))
            .cloned()
            .collect();
        for key in keys {
            if let Some(number) = options.remove(&key) {
                moved.push((key, number, rule.clone()));
            }
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_threshold_under_a_rule_becomes_the_threshold_of_that_name() {
        let mut value = json!({
            "rules": { "long-parameter-list": { "max_params": 6, "severity": "error" }, "disabled": [] },
            "thresholds": { "nesting_depth": 2 },
        });
        thresholds(&mut value, "rqx.yaml").expect("moves");
        assert_eq!(value["thresholds"]["max_params"], 6);
        assert_eq!(value["thresholds"]["nesting_depth"], 2);
        assert_eq!(
            value["rules"]["long-parameter-list"],
            json!({ "severity": "error" })
        );
        assert_eq!(value["rules"]["disabled"], json!([]));
    }

    #[test]
    fn the_same_value_twice_is_fine_and_two_values_are_not() {
        let mut agree = json!({
            "rules": { "long-parameter-list": { "max_params": 6 } },
            "thresholds": { "max_params": 6 },
        });
        assert!(thresholds(&mut agree, "rqx.yaml").is_ok());

        let mut clash = json!({
            "rules": { "long-parameter-list": { "max_params": 6 } },
            "thresholds": { "max_params": 4 },
        });
        let error = thresholds(&mut clash, "rqx.yaml")
            .expect_err("refused")
            .to_string();
        assert!(
            error.contains("max_params") && error.contains("long-parameter-list"),
            "{error}"
        );
    }

    #[test]
    fn a_value_with_no_rules_is_left_alone() {
        let mut value = json!({ "gate": { "fail_on": "error" } });
        thresholds(&mut value, "rqx.yaml").expect("fine");
        assert_eq!(value, json!({ "gate": { "fail_on": "error" } }));
    }
}
