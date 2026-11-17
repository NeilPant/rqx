//! `reimplemented-existing`: a function the change adds that repeats one the base had.

mod common;
use common::Repository;

const PARSE_PORT: &str = "export function parsePort(input: string): number {
  const trimmed = input.trim();
  const value = Number(trimmed);
  if (Number.isNaN(value) || value < 1 || value > 65535) {
    throw new RangeError(`port out of range: ${trimmed}`);
  }
  return Math.floor(value);
}
";

/// The same body with every local renamed and a different function name.
const NORMALISE_PORT: &str = "export function normalisePort(raw: string): number {
  const cleaned = raw.trim();
  const parsed = Number(cleaned);
  if (Number.isNaN(parsed) || parsed < 1 || parsed > 65535) {
    throw new RangeError(`port out of range: ${cleaned}`);
  }
  return Math.floor(parsed);
}
";

const UNRELATED: &str = "export function greet(name: string): string {
  const shout = name.toUpperCase();
  const padded = shout.padEnd(12, '.');
  const line = `hello ${padded}`;
  return line;
}
";

/// A base with one function, and the working tree after `after` is committed.
fn review(
    name: &str,
    before: &[(&str, Option<&str>)],
    after: &[(&str, Option<&str>)],
) -> Vec<serde_json::Value> {
    let repository = Repository::new(name);
    repository.commit("base", before);
    repository.commit("change", after);
    repository.rqx_lines(&["check", ".", "--base", "HEAD~1", "--reporter", "json"])
}

fn reimplemented(lines: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    lines
        .iter()
        .filter(|line| line["kind"] == "finding" && line["rule"] == "reimplemented-existing")
        .collect()
}

#[test]
fn a_new_function_with_an_existing_body_names_the_function_to_call() {
    let lines = review(
        "reimpl-identical",
        &[("src/net.ts", Some(PARSE_PORT))],
        &[("src/ports.ts", Some(NORMALISE_PORT))],
    );
    let found = reimplemented(&lines);
    assert_eq!(found.len(), 1, "{lines:#?}");
    let finding = found[0];
    assert_eq!(finding["location"]["file"], "src/ports.ts");
    assert_eq!(finding["severity"], "warning");
    assert_eq!(finding["confidence"], "verified");
    assert_eq!(finding["evidence"]["added"], "normalisePort");
    assert_eq!(finding["evidence"]["existing"], "parsePort");
    assert_eq!(finding["evidence"]["existing_file"], "src/net.ts");
    assert_eq!(
        finding["fix_hint"],
        "call `parsePort` in src/net.ts:1 instead of keeping a second copy"
    );
}

#[test]
fn a_new_function_unlike_any_existing_one_is_left_alone() {
    let lines = review(
        "reimpl-unrelated",
        &[("src/net.ts", Some(PARSE_PORT))],
        &[("src/greet.ts", Some(UNRELATED))],
    );
    assert!(reimplemented(&lines).is_empty(), "{lines:#?}");
}

#[test]
fn an_edited_function_is_not_an_added_one() {
    let edited = PARSE_PORT.replace("Math.floor(value)", "Math.round(value)");
    let lines = review(
        "reimpl-edited",
        &[
            ("src/net.ts", Some(PARSE_PORT)),
            ("src/other.ts", Some(NORMALISE_PORT)),
        ],
        &[("src/net.ts", Some(&edited))],
    );
    assert!(reimplemented(&lines).is_empty(), "{lines:#?}");
}

#[test]
fn a_moved_function_is_not_a_copy_of_the_one_it_left() {
    let lines = review(
        "reimpl-moved",
        &[("src/net.ts", Some(PARSE_PORT))],
        &[("src/net.ts", None), ("src/ports.ts", Some(PARSE_PORT))],
    );
    assert!(reimplemented(&lines).is_empty(), "{lines:#?}");
}

#[test]
fn code_that_was_already_duplicated_at_the_base_is_not_reported() {
    let lines = review(
        "reimpl-already",
        &[
            ("src/net.ts", Some(PARSE_PORT)),
            ("src/ports.ts", Some(NORMALISE_PORT)),
        ],
        &[("src/greet.ts", Some(UNRELATED))],
    );
    assert!(reimplemented(&lines).is_empty(), "{lines:#?}");
}

#[test]
fn a_plain_scan_never_reports_it() {
    let repository = Repository::new("reimpl-scan");
    repository.commit(
        "all",
        &[
            ("src/net.ts", Some(PARSE_PORT)),
            ("src/ports.ts", Some(NORMALISE_PORT)),
        ],
    );
    let lines = repository.rqx_lines(&["scan", ".", "--reporter", "json"]);
    assert!(reimplemented(&lines).is_empty());
}

const VALIDATE_PRICE_TS: &str =
    "export function validatePrice(amount: number, currency: string): string {
  if (amount < 0) {
    return \"negative\";
  }
  if (amount > 1000000) {
    return \"too large\";
  }
  if (currency.toUpperCase() !== \"USD\") {
    return \"unsupported\";
  }
  if (currency.toUpperCase() !== \"EUR\") {
    return \"unsupported\";
  }
  return \"ok\";
}
";

const VALIDATE_PRICE_PY: &str = "def validate_price(amount, currency):
    if amount < 0:
        return \"negative\"
    if amount > 1000000:
        return \"too large\"
    if currency.upper() != \"USD\":
        return \"unsupported\"
    if currency.upper() != \"EUR\":
        return \"unsupported\"
    return \"ok\"
";

/// The same checks, calling a different member than the base copy does.
const VALIDATE_COST_TS: &str =
    "export function validateCost(amount: number, currency: string): string {
  if (amount < 0) {
    return \"negative\";
  }
  if (amount > 1000000) {
    return \"too large\";
  }
  if (currency.trim() !== \"USD\") {
    return \"unsupported\";
  }
  if (currency.trim() !== \"EUR\") {
    return \"unsupported\";
  }
  return \"ok\";
}
";

#[test]
fn the_same_checks_with_other_member_names_match_at_a_lower_confidence() {
    let lines = review(
        "reimpl-same-logic",
        &[("src/price.ts", Some(VALIDATE_PRICE_TS))],
        &[("src/cost.ts", Some(VALIDATE_COST_TS))],
    );
    let found = reimplemented(&lines);
    assert_eq!(found.len(), 1, "{lines:#?}");
    assert_eq!(found[0]["confidence"], "heuristic");
    assert_eq!(found[0]["evidence"]["likeness"], "same_logic");
    assert_eq!(found[0]["evidence"]["existing"], "validatePrice");
}

#[test]
fn logic_ported_from_another_language_is_named_with_low_confidence() {
    let lines = review(
        "reimpl-ported",
        &[("src/price.ts", Some(VALIDATE_PRICE_TS))],
        &[("tools/price.py", Some(VALIDATE_PRICE_PY))],
    );
    let found = reimplemented(&lines);
    assert_eq!(found.len(), 1, "{lines:#?}");
    assert_eq!(found[0]["confidence"], "low");
    assert_eq!(found[0]["evidence"]["likeness"], "ported");
    assert_eq!(found[0]["evidence"]["existing_file"], "src/price.ts");
}
