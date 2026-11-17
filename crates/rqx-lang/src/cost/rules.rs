//! The performance rules the cost analysis feeds, read through a full analysis of a
//! snippet: which rule reports which line.

use rqx_core::config::RqxConfig;
use rqx_core::finding::Confidence;
use rqx_core::{Language, RuleId};

use crate::analyze_as;

/// The defaults, with the rules that ship off switched on.
fn config() -> RqxConfig {
    let mut config = RqxConfig::default();
    config
        .rules
        .enabled
        .extend(["allocation-in-loop", "missing-capacity"].map(String::from));
    config
}

/// `(rule, line)` for every finding of the given rules, in the order reported.
fn reported(language: Language, source: &str, rules: &[RuleId]) -> Vec<(RuleId, u32)> {
    let config = config();
    let analysis = analyze_as(language, "snippet", source, &config).expect("the snippet parses");
    analysis
        .findings
        .iter()
        .filter(|finding| rules.contains(&finding.rule))
        .map(|finding| (finding.rule, finding.location.line))
        .collect()
}

const IN_LOOP: [RuleId; 3] = [
    RuleId::LinearOpInLoop,
    RuleId::SortInLoop,
    RuleId::ScanInLoop,
];

#[test]
fn a_call_scan_in_loop_reports_is_not_reported_again() {
    let source = "export function f(groups: number[][], base: number[]) {\n\
                  for (const group of groups) {\n\
                  group.push(base.indexOf(3));\n\
                  const copy = base.slice().sort();\n\
                  }\n}\n";
    let found = reported(Language::TypeScript, source, &IN_LOOP);
    let lines: Vec<u32> = found.iter().map(|(_, line)| *line).collect();
    let mut unique = lines.clone();
    unique.dedup();
    assert!(!found.is_empty());
    assert_eq!(lines, unique, "one rule per call: {found:?}");
    assert!(
        found.iter().all(|(rule, _)| *rule == RuleId::ScanInLoop),
        "{found:?}"
    );
}

#[test]
fn a_shift_on_an_array_the_source_types_is_a_linear_op_in_a_loop() {
    let typed = "function f(items: number[]) {\n  const queue: number[] = [...items];\n  while (queue.length > 0) {\n    queue.shift();\n  }\n}\n";
    assert_eq!(
        reported(Language::TypeScript, typed, &[RuleId::LinearOpInLoop]),
        [(RuleId::LinearOpInLoop, 4)]
    );
}

#[test]
fn a_receiver_the_source_does_not_type_is_not_charged() {
    let untyped = "function f(queue) {\n  while (queue.length > 0) {\n    queue.shift();\n  }\n}\n";
    assert!(reported(Language::JavaScript, untyped, &[RuleId::LinearOpInLoop]).is_empty());
}

#[test]
fn a_sort_of_what_the_loop_changes_is_not_a_repeated_sort() {
    let loop_variable =
        "def f(groups: list[list[int]]):\n    for group in groups:\n        group.sort()\n";
    assert!(reported(Language::Python, loop_variable, &[RuleId::SortInLoop]).is_empty());

    let grown = "def f(groups: list[list[int]], base: list[int]):\n    for group in groups:\n        base.append(1)\n        base.sort()\n";
    assert!(reported(Language::Python, grown, &[RuleId::SortInLoop]).is_empty());

    let unchanged = "def f(groups: list[list[int]], base: list[int]):\n    for group in groups:\n        base.sort()\n";
    assert_eq!(
        reported(Language::Python, unchanged, &[RuleId::SortInLoop]),
        [(RuleId::SortInLoop, 3)]
    );
}

fn confidences(language: Language, source: &str, rule: RuleId) -> Vec<Confidence> {
    let config = config();
    let analysis = analyze_as(language, "snippet", source, &config).expect("the snippet parses");
    analysis
        .findings
        .iter()
        .filter(|finding| finding.rule == rule)
        .map(|finding| finding.confidence)
        .collect()
}

#[test]
fn text_the_source_declares_is_a_copy_the_rule_is_sure_of() {
    let declared = "class A {\n  String join(java.util.List<String> parts) {\n    String text = \"\";\n    for (String part : parts) {\n      text += part;\n    }\n    return text;\n  }\n}\n";
    assert_eq!(
        confidences(Language::Java, declared, RuleId::AccumulateByCopy),
        [Confidence::Verified]
    );

    let inferred = "function join(parts) {\n  let flat = [];\n  for (const part of parts) {\n    flat = [...flat, ...part];\n  }\n  return flat;\n}\n";
    assert_eq!(
        confidences(Language::JavaScript, inferred, RuleId::AccumulateByCopy),
        [Confidence::Heuristic]
    );
}

#[test]
fn a_pass_that_mostly_allocates_is_reported_and_one_small_allocation_is_not() {
    let busy = "function f(rows) {\n  for (const row of rows) {\n    const a = [row];\n    const b = { row };\n    const c = [row, row];\n    use(a, b, c);\n  }\n}\n";
    assert_eq!(
        reported(Language::JavaScript, busy, &[RuleId::AllocationInLoop]),
        [(RuleId::AllocationInLoop, 3)]
    );
    let light = "function f(rows) {\n  for (const row of rows) {\n    const a = [row];\n    use(a);\n  }\n}\n";
    assert!(reported(Language::JavaScript, light, &[RuleId::AllocationInLoop]).is_empty());
}

#[test]
fn a_large_fixed_allocation_is_reported_and_a_small_one_is_not() {
    let large = "func f(rows []int) {\n\tfor range rows {\n\t\t_ = make([]byte, 65536)\n\t}\n}\n";
    assert_eq!(
        reported(Language::Go, large, &[RuleId::AllocationInLoop]),
        [(RuleId::AllocationInLoop, 3)]
    );
    let small = "func f(rows []int) {\n\tfor range rows {\n\t\t_ = make([]byte, 16)\n\t}\n}\n";
    assert!(reported(Language::Go, small, &[RuleId::AllocationInLoop]).is_empty());
}

#[test]
fn an_empty_collection_filled_in_a_loop_over_known_input_wants_a_capacity() {
    let empty = "func f(names []string) []string {\n\tout := []string{}\n\tfor _, name := range names {\n\t\tout = append(out, name)\n\t}\n\treturn out\n}\n";
    assert_eq!(
        reported(Language::Go, empty, &[RuleId::MissingCapacity]),
        [(RuleId::MissingCapacity, 4)]
    );
    let sized = "func f(names []string) []string {\n\tout := make([]string, 0, len(names))\n\tfor _, name := range names {\n\t\tout = append(out, name)\n\t}\n\treturn out\n}\n";
    assert!(reported(Language::Go, sized, &[RuleId::MissingCapacity]).is_empty());

    let reserved = "#include <vector>\nvoid f(const std::vector<int>& xs) {\n  std::vector<int> out;\n  out.reserve(xs.size());\n  for (int x : xs) {\n    out.push_back(x);\n  }\n}\n";
    assert!(reported(Language::Cpp, reserved, &[RuleId::MissingCapacity]).is_empty());
}

#[test]
fn a_language_whose_lists_have_no_capacity_is_never_asked_for_one() {
    let python = "def f(names: list[str]):\n    out = []\n    for name in names:\n        out.append(name)\n";
    assert!(reported(Language::Python, python, &[RuleId::MissingCapacity]).is_empty());
}
