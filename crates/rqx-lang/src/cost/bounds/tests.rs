use rqx_core::Language;
use rqx_core::cost::Bound;

use super::*;
use crate::cost::bindings::{bind_assignment, bind_declaration, bind_parameters};
use crate::cost::testing::{names_of, tree_of};
use crate::tree::{descendants, kind_of};

/// The bound of every loop in the first function of `source`, in source order, with
/// the declarations above each loop already in the environment: `xs` for a loop over
/// `xs`, `log n`, `1` for a constant, `?` when nothing is known.
fn bounds_in(language: Language, source: &str) -> Vec<String> {
    let tree = tree_of(language, source);
    let names = names_of(language, source);
    let profile = names.profile;
    let function = descendants(tree.root_node())
        .into_iter()
        .find(|node| profile.is_function(*node))
        .expect("a function");
    let mut env = Env::default();
    let mut bounds = Bounds::new(names, &mut env);
    bind_parameters(&mut bounds, function);
    let mut found = Vec::new();
    for node in descendants(function) {
        let kind = kind_of(node);
        if profile.declarator_kinds.contains(&kind) || profile.local_declaring_kinds.contains(&kind)
        {
            bind_declaration(&mut bounds, node);
        } else if profile.assignment_kinds.contains(&kind) {
            bind_assignment(&mut bounds, node);
        }
        let bound = if profile.loop_kinds.contains(&kind) {
            bounds.of_loop(node)
        } else if let Some((_, field)) = profile
            .loops
            .clauses
            .iter()
            .find(|(clause, _)| *clause == kind)
        {
            node.child_by_field_name(field)
                .map(|iterable| bounds.iterable_bound(iterable))
        } else {
            None
        };
        if let Some(bound) = bound {
            found.push(bound);
        }
    }
    let vars = env.vars();
    found
        .into_iter()
        .map(|bound| match bound {
            Bound::Const => "1".to_owned(),
            Bound::Size(var) => vars[usize::from(var)].name.clone(),
            Bound::Log(var) => format!("log {}", vars[usize::from(var)].name),
            Bound::Unknown => "?".to_owned(),
        })
        .collect()
}

#[test]
fn typescript_loops_are_sized_by_what_they_run_over() {
    let source = "function f(xs: number[], n: number, m: Map<string, number>) {
  for (const x of xs) {}
  for (let i = 0; i < xs.length; i++) {}
  for (let i = 0; i < 10; i++) {}
  for (let i = 0; i < MAX; i++) {}
  for (let i = n; i > 0; i = i / 2) {}
  for (let i = 1; i < n; i *= 2) {}
  while (n > 0) { n = n >> 1; }
  for (const key of Object.keys(m)) {}
  const total = xs.length;
  for (let i = 0; i < total; i++) {}
  const copy = xs.slice(1);
  for (const c of copy) {}
  for (const w of ['a', 'b']) {}
  for (let i = 0; i < n - 1; i++) {}
  while (true) {}
  for (const c of unknown()) {}
}";
    assert_eq!(
        bounds_in(Language::TypeScript, source),
        [
            "xs", "xs", "1", "1", "log n", "log n", "log n", "m", "xs", "xs", "1", "n", "?", "?"
        ]
    );
}

#[test]
fn python_loops_are_sized_by_what_they_run_over() {
    let source = "def f(xs: list[int], n: int, m: dict[str, int]):
    for x in xs: pass
    for i in range(len(xs)): pass
    for i in range(10): pass
    for i in range(n): pass
    for i in range(1, n): pass
    for k, v in m.items(): pass
    for i, x in enumerate(xs): pass
    while n > 0:
        n //= 2
    hi = len(xs) - 1
    lo = 0
    while lo <= hi:
        mid = (lo + hi) // 2
        if xs[mid] < 3:
            lo = mid + 1
        else:
            hi = mid - 1
    total = len(xs)
    for i in range(total): pass
    ys = sorted(xs)
    for y in ys: pass
    for z in (1, 2, 3): pass
    squares = [a * a for a in xs for b in m]
";
    assert_eq!(
        bounds_in(Language::Python, source),
        [
            "xs", "xs", "1", "n", "n", "m", "xs", "log n", "log xs", "xs", "xs", "1", "xs", "m"
        ]
    );
}

#[test]
fn java_loops_are_sized_by_what_they_run_over() {
    let source = "class A { int f(List<Integer> xs, int n, Map<String, Integer> m) {
  for (int x : xs) {}
  for (int i = 0; i < xs.size(); i++) {}
  for (int i = n; i > 0; i /= 2) {}
  while (n > 0) { n = n / 2; }
  for (String k : m.keySet()) {}
  for (int i = 0; i < 100; i++) {}
  int[] fixed = {1, 2, 3};
  for (int v : fixed) {}
  for (int j = 0; j < arr.length; j++) {}
} }";
    assert_eq!(
        bounds_in(Language::Java, source),
        ["xs", "xs", "log n", "log n", "m", "1", "1", "arr"]
    );
}

#[test]
fn go_loops_are_sized_by_what_they_run_over() {
    let source = "package a
func f(xs []int, n int, m map[string]int) {
	for _, x := range xs {}
	for i := 0; i < len(xs); i++ {}
	for i := n; i > 0; i /= 2 {}
	for n > 0 { n /= 2 }
	for i := range 10 {}
	for k := range m {}
	for {}
}";
    assert_eq!(
        bounds_in(Language::Go, source),
        ["xs", "xs", "log n", "log n", "1", "m", "?"]
    );
}

#[test]
fn cpp_loops_are_sized_by_what_they_run_over() {
    let source =
        "int f(const std::vector<int>& xs, int n, std::unordered_map<std::string, int>& m) {
  for (int x : xs) {}
  for (std::size_t i = 0; i < xs.size(); ++i) {}
  for (int i = n; i > 0; i /= 2) {}
  while (n > 0) { n /= 2; }
  for (auto& kv : m) {}
  for (int i = 0; i < 8; ++i) {}
}";
    assert_eq!(
        bounds_in(Language::Cpp, source),
        ["xs", "xs", "log n", "log n", "m", "1"]
    );
}
