//! The places the cross-file rules report, read from real source composed as one file.

use std::path::Path;

use rqx_core::config::RqxConfig;
use rqx_core::cost::Conf;
use rqx_core::cost_sites::CrossSite;
use rqx_core::{Language, RuleId};

use super::{Unit, compose_with_sites};
use crate::cost::tables::CostTables;
use crate::module_graph::ResolvedRequests;

fn sites(language: Language, source: &str, complete: bool) -> Vec<CrossSite> {
    let analysis = crate::analyze_as(language, "snippet", source, &RqxConfig::default())
        .expect("the snippet parses");
    let units = [Unit {
        name: "snippet",
        path: Path::new("/p/snippet"),
        facts: &analysis.module,
        cost: &analysis.cost,
    }];
    compose_with_sites(
        &units,
        &ResolvedRequests::new(),
        &CostTables::built_in(),
        complete,
        true,
    )
    .sites
}

fn found(language: Language, source: &str) -> Vec<(RuleId, u32)> {
    sites(language, source, true)
        .iter()
        .map(|site| (site.rule, site.line))
        .collect()
}

const QUADRATIC: &str = "function indexOfItem(list: string[], wanted: string) {\n  for (let i = 0; i < list.length; i++) { if (list[i] === wanted) { return i; } }\n  return -1;\n}\nexport function dedupe(items: string[]) {\n  const out: string[] = [];\n  for (const item of items) {\n    if (indexOfItem(items, item) === out.length) { out.push(item); }\n  }\n  return out;\n}\n";

#[test]
fn a_loop_over_a_collection_around_a_call_that_scans_it_is_quadratic() {
    let found = sites(Language::TypeScript, QUADRATIC, true);
    assert_eq!(found.len(), 1, "{found:?}");
    let site = &found[0];
    assert_eq!(site.rule, RuleId::QuadraticViaCall);
    assert_eq!(site.line, 8);
    assert_eq!(site.subject, "indexOfItem");
    assert!(
        site.message
            .contains("O(n²): loop :7 \u{2192} indexOfItem O(n) :1"),
        "{}",
        site.message
    );
    assert_eq!(site.conf, Conf::Verified);
}

#[test]
fn a_loop_over_another_collection_is_a_product_and_not_a_square() {
    let source = QUADRATIC
        .replace("indexOfItem(items, item)", "indexOfItem(other, item)")
        .replace(
            "dedupe(items: string[])",
            "dedupe(items: string[], other: string[])",
        );
    assert!(found(Language::TypeScript, &source).is_empty());
}

#[test]
fn a_call_that_does_not_depend_on_the_collection_is_not_reported() {
    let source = "function pick(x: string) { return x; }\nexport function f(items: string[]) {\n  for (const item of items) { pick(item); }\n}\n";
    assert!(found(Language::TypeScript, source).is_empty());
}

#[test]
fn a_scan_that_did_not_read_the_whole_project_reports_nothing() {
    assert!(sites(Language::TypeScript, QUADRATIC, false).is_empty());
}

#[test]
fn a_call_with_the_same_arguments_on_every_pass_is_repeated_and_not_also_quadratic() {
    let source = "function measure(items: string[]) {\n  let n = 0;\n  for (const i of items) { n += i.length; }\n  return n;\n}\nexport function f(items: string[], keys: string[]) {\n  const out: number[] = [];\n  for (const key of keys) {\n    const total = measure(items);\n    out.push(total + key.length);\n  }\n  return out;\n}\n";
    assert_eq!(
        found(Language::TypeScript, source),
        [(RuleId::RepeatedComputationInLoop, 9)]
    );
    assert_eq!(
        sites(Language::TypeScript, source, true)[0].conf,
        Conf::Heuristic
    );
}

#[test]
fn a_call_that_costs_nothing_is_not_a_repeat() {
    let source = "function id(items: string[]) { return items; }\nexport function f(items: string[], keys: string[]) {\n  for (const key of keys) {\n    const same = id(items);\n    sink(same, key);\n  }\n}\n";
    assert!(found(Language::TypeScript, source).is_empty());
}

#[test]
fn a_recursion_one_level_per_unit_of_its_argument_is_deep() {
    let python = "def total(n):\n    if n == 0:\n        return 0\n    return n + total(n - 1)\n";
    assert_eq!(
        found(Language::Python, python),
        [(RuleId::DeepRecursion, 4)]
    );

    let halving = "def depth(n):\n    if n <= 1:\n        return 0\n    return 1 + depth(n // 2)\n";
    assert!(found(Language::Python, halving).is_empty());

    let branching =
        "def fib(n):\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n";
    assert!(found(Language::Python, branching).is_empty());
}

#[test]
fn two_loops_comparing_their_elements_are_a_pair_scan() {
    let source = "export function join(orders: Order[], users: User[]) {\n  for (const order of orders) {\n    for (const user of users) {\n      if (order.userId === user.id) { sink(order, user); }\n    }\n  }\n}\n";
    assert_eq!(
        found(Language::TypeScript, source),
        [(RuleId::PairScanCouldHash, 4)]
    );
}
