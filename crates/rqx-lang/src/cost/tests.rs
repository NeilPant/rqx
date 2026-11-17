//! The extraction end to end: source in, the facts the cache holds out.

use rqx_core::Language;
use rqx_core::config::Performance;
use rqx_core::cost::{CalleeRef, Conf, Cost, FileCost, FunctionCost, Shrink};

use super::testing::tree_of;
use super::{Inputs, extract, local, tables::CostTables};

fn facts(language: Language, source: &str) -> FileCost {
    let tree = tree_of(language, source);
    let tables = CostTables::built_in();
    let performance = Performance::default();
    extract(
        crate::support(language).profile,
        source,
        tree.root_node(),
        &Inputs {
            tables: &tables,
            performance: &performance,
            module: None,
        },
    )
}

fn function<'f>(file: &'f FileCost, name: &str) -> &'f FunctionCost {
    file.functions
        .iter()
        .find(|function| function.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no cost recorded for {name}: {file:#?}"))
}

/// The function's own time, with calls to its other functions left out.
fn time_of(language: Language, source: &str, name: &str) -> String {
    let file = facts(language, source);
    local::time(function(&file, name), &CostTables::built_in()).to_string()
}

fn space_of(language: Language, source: &str, name: &str) -> String {
    let file = facts(language, source);
    local::space(function(&file, name), &CostTables::built_in()).to_string()
}

#[test]
fn a_function_that_does_nothing_costly_is_not_recorded() {
    let file = facts(
        Language::TypeScript,
        "function add(a: number, b: number) { return a + b; }\nclass A { get x() { return 1; } }\n",
    );
    assert!(file.is_empty(), "{file:?}");
}

#[test]
fn nested_loops_multiply_and_sequential_ones_do_not() {
    let source = "function pairs(a: number[], b: number[]) {\n  for (const x of a) { for (const y of b) { sink(x, y); } }\n}\n\
                  function twice(a: number[], b: number[]) {\n  for (const x of a) { sink(x); }\n  for (const y of b) { sink(y); }\n}\n\
                  function square(a: number[]) {\n  for (const x of a) { for (const y of a) { sink(x, y); } }\n}\n";
    assert_eq!(time_of(Language::TypeScript, source, "pairs"), "O(n·m)");
    assert_eq!(time_of(Language::TypeScript, source, "twice"), "O(n + m)");
    assert_eq!(time_of(Language::TypeScript, source, "square"), "O(n²)");
}

#[test]
fn a_callback_to_a_higher_order_method_is_a_loop_over_the_receiver() {
    let source = "function f(xs: number[], ys: number[]) {\n  xs.forEach((x) => { ys.forEach((y) => sink(x, y)); });\n  const doubled = xs.filter((x) => x > 0).map((x) => x * 2);\n}\n";
    assert_eq!(time_of(Language::TypeScript, source, "f"), "O(n·m)");
}

#[test]
fn a_typed_receiver_is_verified_and_a_guessed_one_is_not() {
    let typed = "function f(xs: number[], values: number[]) {\n  for (const v of values) { xs.includes(v); }\n}\n";
    let file = facts(Language::TypeScript, typed);
    assert_eq!(function(&file, "f").conf, Conf::Verified);
    assert_eq!(time_of(Language::TypeScript, typed, "f"), "O(n·m)");

    let untyped = "function f(xs, values) {\n  for (const v of values) { xs.includes(v); }\n}\n";
    let file = facts(Language::JavaScript, untyped);
    assert_eq!(function(&file, "f").conf, Conf::Heuristic);
    assert_eq!(time_of(Language::JavaScript, untyped, "f"), "O(n·m)");
}

#[test]
fn a_lookup_in_a_keyed_collection_costs_nothing() {
    let by_type = "function f(values: number[]) {\n  const seen = new Set<number>();\n  for (const v of values) { if (!seen.has(v)) { seen.add(v); } }\n}\n";
    assert_eq!(time_of(Language::TypeScript, by_type, "f"), "O(n)");
    let by_name = "function f(values, lookup) {\n  for (const v of values) { lookup.has(v); lookup.indexOf(v); }\n}\n";
    // `has` on a name that sounds keyed is skipped, and so is `indexOf` on it.
    assert_eq!(time_of(Language::JavaScript, by_name, "f"), "O(n)");
    let in_python = "def f(values: list[int]):\n    seen: set[int] = set()\n    for v in values:\n        if v in seen:\n            pass\n";
    assert_eq!(time_of(Language::Python, in_python, "f"), "O(n)");
}

#[test]
fn membership_in_a_list_is_a_scan_in_python() {
    let source = "def f(values: list[int], other: list[int]):\n    for v in values:\n        if v in other:\n            pass\n";
    assert_eq!(time_of(Language::Python, source, "f"), "O(n·m)");
}

#[test]
fn a_list_added_to_in_a_loop_is_no_longer_than_the_loop() {
    let source = "function dedupe(values: number[]) {\n  const unique: number[] = [];\n  for (const v of values) { if (!unique.includes(v)) { unique.push(v); } }\n  return unique;\n}\n";
    assert_eq!(time_of(Language::TypeScript, source, "dedupe"), "O(n²)");
    assert_eq!(space_of(Language::TypeScript, source, "dedupe"), "O(n)");
}

#[test]
fn a_sort_in_a_loop_multiplies_by_its_own_size() {
    let source = "def rank(queries: list[int], pool: list[int]):\n    for q in queries:\n        ordered = sorted(pool)\n        ordered.index(q)\n";
    assert_eq!(time_of(Language::Python, source, "rank"), "O(n·m log m)");
}

#[test]
fn a_binary_search_is_logarithmic_whichever_way_it_closes_in() {
    let source = "def search(xs: list[int], target: int):\n    lo = 0\n    hi = len(xs) - 1\n    while lo <= hi:\n        mid = (lo + hi) // 2\n        if xs[mid] < target:\n            lo = mid + 1\n        else:\n            hi = mid - 1\n    return -1\n";
    assert_eq!(time_of(Language::Python, source, "search"), "O(log n)");
}

#[test]
fn a_loop_of_unknown_length_is_heuristic() {
    let source = "function f() {\n  while (true) { step(); }\n}\n";
    let file = facts(Language::TypeScript, source);
    assert_eq!(function(&file, "f").conf, Conf::Heuristic);
}

#[test]
fn calls_to_functions_of_the_file_are_recorded_with_their_loop_depth() {
    let source = "function helper(items: number[]) { for (const i of items) { sink(i); } }\n\
                  function caller(rows: number[][]) {\n  helper(rows[0]);\n  for (const row of rows) { helper(row); }\n}\n";
    let file = facts(Language::TypeScript, source);
    let caller = function(&file, "caller");
    assert_eq!(caller.calls.len(), 2);
    assert_eq!(
        caller.calls[0].callee,
        CalleeRef::Local("helper".to_owned())
    );
    assert_eq!(caller.calls[0].in_loop, 0);
    assert_eq!(caller.calls[1].in_loop, 1);
    // The call is not charged in the function's own time.
    assert!(matches!(caller.time, Cost::Seq(_) | Cost::Loop { .. }));
    assert_eq!(time_of(Language::TypeScript, source, "caller"), "O(n)");
}

#[test]
fn recursion_is_classified_by_how_it_shrinks_its_argument() {
    let fib = "function fib(n: number): number { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }\n";
    let rec = function(&facts(Language::TypeScript, fib), "fib")
        .rec
        .clone()
        .expect("recursive");
    assert_eq!((rec.calls, rec.shrink, rec.memo), (2, Shrink::Dec1, false));

    let memo = "def fib(n, memo={}):\n    if n in memo:\n        return memo[n]\n    memo[n] = fib(n - 1, memo) + fib(n - 2, memo)\n    return memo[n]\n";
    let rec = function(&facts(Language::Python, memo), "fib")
        .rec
        .clone()
        .unwrap();
    assert!(rec.memo);

    let cached = "from functools import lru_cache\n@lru_cache(maxsize=None)\ndef fib(n):\n    return fib(n - 1) + fib(n - 2)\n";
    let rec = function(&facts(Language::Python, cached), "fib")
        .rec
        .clone()
        .unwrap();
    assert!(rec.memo);

    let halving = "int search(const std::vector<int>& xs, int n) { return n <= 1 ? 0 : search(xs, n / 2) + 1; }\n";
    let rec = function(&facts(Language::Cpp, halving), "search")
        .rec
        .clone()
        .unwrap();
    assert_eq!((rec.calls, rec.shrink), (1, Shrink::Half));
}

#[test]
fn a_slice_that_copies_is_noticed_when_the_parameter_is_named() {
    let copying = "def total(xs):\n    return 0 if not xs else xs[0] + total(xs[1:])\n";
    let file = facts(Language::Python, copying);
    let rec = function(&file, "total").rec.clone().unwrap();
    assert_eq!(rec.shrink, Shrink::Dec1);
    assert!(rec.copies_arg);
}

#[test]
fn a_round_trip_in_a_loop_is_counted_by_the_loops_around_it() {
    let source = "async function load(ids: string[]) {\n  for (const id of ids) { await db.user.findOne(id); }\n}\n";
    let file = facts(Language::TypeScript, source);
    let load = function(&file, "load");
    assert!(
        matches!(load.ind.round_trips, Cost::Loop { .. }),
        "{:?}",
        load.ind
    );
}

#[test]
fn consecutive_independent_awaits_are_counted() {
    let source = "async function f(a: string, b: string) {\n  const x = await fetchOne(a);\n  const y = await fetchTwo(b);\n  return [x, y];\n}\n\
                  async function g(a: string) {\n  const x = await fetchOne(a);\n  const y = await fetchTwo(x);\n  return y;\n}\n";
    let file = facts(Language::TypeScript, source);
    assert_eq!(function(&file, "f").ind.seq_awaits, 1);
    assert!(
        file.functions
            .iter()
            .find(|function| function.name.as_deref() == Some("g"))
            .is_none_or(|function| function.ind.seq_awaits == 0)
    );
}

#[test]
fn locks_in_a_loop_and_unbounded_fan_out_are_flagged() {
    let source = "func f(xs []int, mu *sync.Mutex) {\n\tfor _, x := range xs {\n\t\tmu.Lock()\n\t\tgo work(x)\n\t\tmu.Unlock()\n\t}\n}\n";
    let file = facts(Language::Go, &format!("package a\n{source}"));
    let f = function(&file, "f");
    assert!(f.ind.locks_in_loop);
    assert!(f.ind.unbounded_fanout);

    let gather = "async def f(xs):\n    return await asyncio.gather(*[work(x) for x in xs])\n";
    let file = facts(Language::Python, gather);
    assert!(function(&file, "f").ind.unbounded_fanout);

    let all = "async function f(xs: number[]) { return Promise.all(xs.map((x) => work(x))); }\n";
    let file = facts(Language::TypeScript, all);
    assert!(function(&file, "f").ind.unbounded_fanout);
}

#[test]
fn a_blocking_call_in_an_async_function_is_flagged() {
    let source = "import fs from 'fs';\nasync function f(p: string) { return fs.readFileSync(p); }\nfunction g(p: string) { return fs.readFileSync(p); }\n";
    let file = facts(Language::TypeScript, source);
    assert!(function(&file, "f").ind.blocking_async);
    assert!(
        file.functions
            .iter()
            .find(|function| function.name.as_deref() == Some("g"))
            .is_none_or(|function| !function.ind.blocking_async)
    );
}

#[test]
fn allocations_per_pass_are_counted_in_the_innermost_loop() {
    let source = "function f(xs: number[]) {\n  for (const x of xs) {\n    const a = [x];\n    const b = { x };\n    const c = new Map();\n  }\n}\n";
    let file = facts(Language::TypeScript, source);
    assert_eq!(function(&file, "f").ind.allocs_per_iter, 3);
}

#[test]
fn a_shared_collection_that_a_function_adds_to_is_named() {
    let source = "const REGISTRY = new Map<string, number>();\nfunction remember(key: string) { REGISTRY.set(key, 1); }\n";
    let file = facts(Language::TypeScript, source);
    assert_eq!(function(&file, "remember").ind.static_growth, ["REGISTRY"]);
}

#[test]
fn methods_are_named_by_their_class_and_see_its_members() {
    let source = "class Index {\n  private items: string[] = [];\n  private lookup = new Map<string, number>();\n  find(names: string[]) {\n    for (const n of names) { this.items.includes(n); this.lookup.get(n); }\n  }\n}\n";
    let file = facts(Language::TypeScript, source);
    let find = function(&file, "find");
    assert_eq!(find.qual, "Index.find");
    assert_eq!(find.conf, Conf::Verified);
    assert_eq!(
        local::time(find, &CostTables::built_in()).to_string(),
        "O(n·m)"
    );
    assert!(find.vars.iter().any(|var| var.name == "this.items"));
}

#[test]
fn a_collection_made_with_a_size_is_that_large() {
    let go = "package a\nfunc f(values []int) {\n\tordered := make([]int, len(values))\n\tbuffer := make([]int, 0, len(values))\n\tfor _, o := range ordered { sink(o) }\n\tfor _, b := range buffer { sink(b) }\n}\n";
    assert_eq!(time_of(Language::Go, go, "f"), "O(n)");

    let java = "class A { void f(List<Integer> xs) {\n  Set<Integer> copy = new HashSet<>(xs);\n  for (int c : copy) { sink(c); }\n} }\n";
    assert_eq!(time_of(Language::Java, java, "f"), "O(n)");

    let typescript = "function f(n: number) {\n  const slots = new Array(n);\n  for (const s of slots) { sink(s); }\n}\n";
    assert_eq!(time_of(Language::TypeScript, typescript, "f"), "O(n)");
}

#[test]
fn names_set_together_are_set_from_the_values_in_their_places() {
    let python = "def f(xs):\n    lo, hi = 0, len(xs) - 1\n    while lo <= hi:\n        mid = (lo + hi) // 2\n        lo = mid + 1\n        hi = mid - 1\n";
    assert_eq!(time_of(Language::Python, python, "f"), "O(log n)");
    let go = "package a\nfunc f(xs []int) {\n\tlow, high := 0, len(xs)\n\tfor i := low; i < high; i++ { sink(i) }\n}\n";
    assert_eq!(time_of(Language::Go, go, "f"), "O(n)");
}

#[test]
fn a_constant_the_file_names_in_lower_case_is_still_a_constant() {
    let go = "package a\nconst window = 8\nfunc f(xs []int) {\n\tfor i := 0; i < window; i++ { sink(xs[i]) }\n}\n";
    assert_eq!(time_of(Language::Go, go, "f"), "O(1)");
    let not_a_constant = "package a\nvar window = 8\nfunc f(xs []int) {\n\tfor i := 0; i < window; i++ { sink(xs[i]) }\n}\n";
    assert_ne!(time_of(Language::Go, not_a_constant, "f"), "O(1)");
}

#[test]
fn a_method_called_on_a_literal_is_still_costed() {
    let python = "def f(parts: list[str]):\n    return \",\".join(parts)\n";
    assert_eq!(time_of(Language::Python, python, "f"), "O(n)");
    let negated = "def f(values: list[int], other: list[int]):\n    for v in values:\n        if v not in other:\n            pass\n";
    assert_eq!(time_of(Language::Python, negated, "f"), "O(n·m)");
}

#[test]
fn an_extraction_is_the_same_each_time() {
    let source = "function f(a: number[], b: number[]) {\n  for (const x of a) { for (const y of b) { sink(x, y); } }\n}\n";
    assert_eq!(
        facts(Language::TypeScript, source),
        facts(Language::TypeScript, source)
    );
}
