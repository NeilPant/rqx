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

#[test]
fn a_lock_the_loop_does_not_change_is_taken_once_not_per_pass() {
    let invariant = "func f(mu *sync.Mutex, rows []int) {\n\tfor range rows {\n\t\tmu.Lock()\n\t\tmu.Unlock()\n\t}\n}\n";
    assert_eq!(
        reported(Language::Go, invariant, &[RuleId::LockInLoop]),
        [(RuleId::LockInLoop, 3)]
    );
    let per_pass = "func f(locks []*sync.Mutex, rows []int) {\n\tfor at := range rows {\n\t\tlocks[at].Lock()\n\t\tlocks[at].Unlock()\n\t}\n}\n";
    assert!(reported(Language::Go, per_pass, &[RuleId::LockInLoop]).is_empty());
}

#[test]
fn work_started_per_element_with_no_limit_is_reported() {
    let unlimited =
        "async function f(ids) {\n  return Promise.all(ids.map((id) => fetch(id)));\n}\n";
    assert_eq!(
        reported(
            Language::JavaScript,
            unlimited,
            &[RuleId::UnboundedConcurrency]
        ),
        [(RuleId::UnboundedConcurrency, 2)]
    );
    let limited = "async function f(ids, limit) {\n  return Promise.all(ids.map((id) => limit(() => fetch(id))));\n}\n";
    assert!(
        reported(
            Language::JavaScript,
            limited,
            &[RuleId::UnboundedConcurrency]
        )
        .is_empty()
    );
    let fixed = "async function f(a, b) {\n  return Promise.all([fetch(a), fetch(b)]);\n}\n";
    assert!(reported(Language::JavaScript, fixed, &[RuleId::UnboundedConcurrency]).is_empty());
}

#[test]
fn work_started_in_a_loop_counts_only_when_it_leaves_the_process() {
    let io = "func f(urls []string) {\n\tfor _, url := range urls {\n\t\tgo func() { http.Get(url) }()\n\t}\n}\n";
    assert_eq!(
        reported(Language::Go, io, &[RuleId::UnboundedConcurrency]),
        [(RuleId::UnboundedConcurrency, 3)]
    );
    let compute = "func f(rows []int) {\n\tfor _, row := range rows {\n\t\tgo func() { square(row) }()\n\t}\n}\n";
    assert!(reported(Language::Go, compute, &[RuleId::UnboundedConcurrency]).is_empty());
}

#[test]
fn a_shared_map_that_only_gains_keys_from_callers_is_an_unbounded_cache() {
    let growing =
        "const seen = new Map();\nfunction remember(key, value) {\n  seen.set(key, value);\n}\n";
    assert_eq!(
        reported(Language::JavaScript, growing, &[RuleId::UnboundedCache]),
        [(RuleId::UnboundedCache, 3)]
    );
    let trimmed = "const seen = new Map();\nfunction remember(key, value) {\n  seen.set(key, value);\n  if (seen.size > 9) seen.clear();\n}\n";
    assert!(reported(Language::JavaScript, trimmed, &[RuleId::UnboundedCache]).is_empty());
    let named = "const weakSeen = new WeakMap();\nfunction remember(key, value) {\n  weakSeen.set(key, value);\n}\n";
    assert!(reported(Language::JavaScript, named, &[RuleId::UnboundedCache]).is_empty());
}

#[test]
fn a_collection_filled_from_nothing_the_caller_chose_is_not_a_cache() {
    let counter =
        "const ids = [];\nlet next = 0;\nfunction mint() {\n  next += 1;\n  ids.push(next);\n}\n";
    assert!(reported(Language::JavaScript, counter, &[RuleId::UnboundedCache]).is_empty());
    let table = "const NAMES = ['a', 'b'];\nfunction add(name) {\n  NAMES.push(name);\n}\n";
    assert!(reported(Language::JavaScript, table, &[RuleId::UnboundedCache]).is_empty());
}

#[test]
fn a_memoizing_decorator_with_no_maximum_is_an_unbounded_cache() {
    let unbounded = "import functools\n\n@functools.lru_cache(maxsize=None)\ndef square(n):\n    return n * n\n";
    assert_eq!(
        reported(Language::Python, unbounded, &[RuleId::UnboundedCache]),
        [(RuleId::UnboundedCache, 3)]
    );
    let bounded =
        "import functools\n\n@functools.lru_cache(maxsize=128)\ndef square(n):\n    return n * n\n";
    assert!(reported(Language::Python, bounded, &[RuleId::UnboundedCache]).is_empty());
}

#[test]
fn a_defer_in_a_loop_waits_for_the_function_and_one_in_a_closure_does_not() {
    let looped = "func f(paths []string) {\n\tfor _, path := range paths {\n\t\tfile, _ := os.Open(path)\n\t\tdefer file.Close()\n\t}\n}\n";
    assert_eq!(
        reported(Language::Go, looped, &[RuleId::DeferInLoop]),
        [(RuleId::DeferInLoop, 4)]
    );
    let closure = "func f(paths []string) {\n\tfor _, path := range paths {\n\t\tfunc() {\n\t\t\tfile, _ := os.Open(path)\n\t\t\tdefer file.Close()\n\t\t}()\n\t}\n}\n";
    assert!(reported(Language::Go, closure, &[RuleId::DeferInLoop]).is_empty());
    let after = "func f(path string) {\n\tfile, _ := os.Open(path)\n\tdefer file.Close()\n}\n";
    assert!(reported(Language::Go, after, &[RuleId::DeferInLoop]).is_empty());
}

#[test]
fn a_boxed_number_stepped_in_a_loop_is_reported_and_a_primitive_is_not() {
    let boxed = "class A {\n  Integer f(java.util.List<Integer> rows) {\n    Integer total = 0;\n    for (Integer row : rows) {\n      total += row;\n    }\n    return total;\n  }\n}\n";
    assert_eq!(
        reported(Language::Java, boxed, &[RuleId::BoxingInLoop]),
        [(RuleId::BoxingInLoop, 5)]
    );
    let counted = "class A {\n  Integer f(int n) {\n    Integer total = 0;\n    for (int at = 0; at < n; at++) {\n      total++;\n    }\n    return total;\n  }\n}\n";
    assert_eq!(
        reported(Language::Java, counted, &[RuleId::BoxingInLoop]),
        [(RuleId::BoxingInLoop, 5)]
    );
    let primitive = "class A {\n  int f(java.util.List<Integer> rows) {\n    int total = 0;\n    for (Integer row : rows) {\n      total += row;\n    }\n    return total;\n  }\n}\n";
    assert!(reported(Language::Java, primitive, &[RuleId::BoxingInLoop]).is_empty());
}

#[test]
fn a_range_for_variable_copied_and_only_read_is_reported() {
    let copied = "void f(const std::vector<std::string>& xs) {\n  for (std::string x : xs) {\n    use(x);\n  }\n}\n";
    assert_eq!(
        reported(Language::Cpp, copied, &[RuleId::CopyInRangeFor]),
        [(RuleId::CopyInRangeFor, 2)]
    );
    let by_reference = "void f(const std::vector<std::string>& xs) {\n  for (const std::string& x : xs) {\n    use(x);\n  }\n}\n";
    assert!(reported(Language::Cpp, by_reference, &[RuleId::CopyInRangeFor]).is_empty());
    let edited = "void f(const std::vector<std::string>& xs) {\n  for (std::string x : xs) {\n    x += \"!\";\n  }\n}\n";
    assert!(reported(Language::Cpp, edited, &[RuleId::CopyInRangeFor]).is_empty());
    let cheap = "void f(const std::vector<int>& xs) {\n  for (int x : xs) {\n    use(x);\n  }\n}\n";
    assert!(reported(Language::Cpp, cheap, &[RuleId::CopyInRangeFor]).is_empty());
}

#[test]
fn a_container_parameter_taken_by_value_and_only_read_is_reported() {
    let read = "std::size_t f(std::vector<int> xs) {\n  return xs.size();\n}\n";
    assert_eq!(
        reported(Language::Cpp, read, &[RuleId::LargeByValue]),
        [(RuleId::LargeByValue, 1)]
    );
    let moved = "struct A {\n  A(std::vector<int> xs) : xs_(std::move(xs)) {}\n  std::vector<int> xs_;\n};\n";
    assert!(reported(Language::Cpp, moved, &[RuleId::LargeByValue]).is_empty());
    let changed = "std::string f(std::string s) {\n  s += \"!\";\n  return s;\n}\n";
    assert!(reported(Language::Cpp, changed, &[RuleId::LargeByValue]).is_empty());
    let by_reference = "std::size_t f(const std::vector<int>& xs) {\n  return xs.size();\n}\n";
    assert!(reported(Language::Cpp, by_reference, &[RuleId::LargeByValue]).is_empty());
}
