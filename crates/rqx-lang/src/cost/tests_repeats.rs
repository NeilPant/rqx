//! The facts the cross-file rules read off a function's syntax: calls in a loop that
//! the loop cannot change the answer of, and pairs of loops that compare with `==`.

use rqx_core::Language;
use rqx_core::config::Performance;
use rqx_core::cost::{FunctionCost, Invoked};

use super::testing::tree_of;
use super::{Inputs, extract, tables::CostTables};

fn function(language: Language, source: &str, name: &str) -> FunctionCost {
    let tree = tree_of(language, source);
    let tables = CostTables::built_in();
    let performance = Performance::default();
    let file = extract(
        crate::support(language).profile,
        source,
        tree.root_node(),
        &Inputs {
            tables: &tables,
            performance: &performance,
            module: None,
        },
    );
    file.functions
        .into_iter()
        .find(|function| function.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no cost recorded for {name}"))
}

fn repeated(language: Language, source: &str) -> Vec<(u32, String)> {
    function(language, source, "f")
        .repeats
        .into_iter()
        .map(|repeat| (repeat.line, repeat.subject))
        .collect()
}

#[test]
fn a_call_on_names_the_loop_never_assigns_is_a_repeat() {
    let source = "function f(items: string[], keys: string[]) {\n  const out = [];\n  for (const key of keys) {\n    const total = measure(items);\n    out.push(total + key);\n  }\n}\nfunction measure(items: string[]) { for (const i of items) { sink(i); } }\n";
    assert_eq!(
        repeated(Language::TypeScript, source),
        [(4, "measure".to_owned())]
    );
}

#[test]
fn a_call_on_the_loop_variable_or_on_something_the_loop_changes_is_not() {
    let variable = "function f(items: string[], keys: string[]) {\n  for (const key of keys) {\n    const total = measure(key);\n  }\n}\nfunction measure(x: string) { return x; }\n";
    assert!(repeated(Language::TypeScript, variable).is_empty());

    let grown = "function f(keys: string[]) {\n  const seen: string[] = [];\n  for (const key of keys) {\n    seen.push(key);\n    const total = measure(seen);\n  }\n}\nfunction measure(x: string[]) { return x.length; }\n";
    assert!(repeated(Language::TypeScript, grown).is_empty());

    let reassigned = "function f(keys: string[]) {\n  let acc = [];\n  for (const key of keys) {\n    acc = [key];\n    const total = measure(acc);\n  }\n}\nfunction measure(x: string[]) { return x.length; }\n";
    assert!(repeated(Language::TypeScript, reassigned).is_empty());
}

#[test]
fn a_call_made_for_what_it_does_is_not_a_repeat() {
    let source = "function f(items: string[], keys: string[]) {\n  for (const key of keys) {\n    save(items);\n  }\n}\nfunction save(items: string[]) { for (const i of items) { sink(i); } }\n";
    assert!(repeated(Language::TypeScript, source).is_empty());
}

#[test]
fn a_call_in_a_callback_is_left_to_the_loop_it_belongs_to() {
    let source = "function f(items: string[], keys: string[]) {\n  keys.forEach((key) => { const total = measure(items); sink(total, key); });\n}\nfunction measure(items: string[]) { return items.length; }\n";
    assert!(repeated(Language::TypeScript, source).is_empty());
}

#[test]
fn a_pure_library_call_on_an_unchanged_collection_is_a_repeat_of_its_entry() {
    let source = "function f(items: string[], keys: string[]) {\n  for (const key of keys) {\n    const at = items.indexOf(\"x\");\n    sink(at, key);\n  }\n}\n";
    let found = function(Language::TypeScript, source, "f").repeats;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(matches!(found[0].of, Invoked::Lib { .. }), "{found:?}");
}

#[test]
fn a_python_call_in_a_loop_is_read_the_same_way() {
    let source = "def f(items, keys):\n    for key in keys:\n        total = measure(items)\n        print(total, key)\n\ndef measure(items):\n    return len(items)\n";
    assert_eq!(
        repeated(Language::Python, source),
        [(3, "measure".to_owned())]
    );
}

fn pairs(language: Language, source: &str) -> Vec<(String, String)> {
    function(language, source, "f")
        .pair_scans
        .into_iter()
        .map(|pair| (pair.outer, pair.inner))
        .collect()
}

#[test]
fn two_loops_over_different_collections_comparing_their_elements_are_a_pair_scan() {
    let typescript = "function f(orders: Order[], users: User[]) {\n  for (const order of orders) {\n    for (const user of users) {\n      if (order.userId === user.id) { sink(order, user); }\n    }\n  }\n}\n";
    assert_eq!(
        pairs(Language::TypeScript, typescript),
        [("orders".to_owned(), "users".to_owned())]
    );
    let python = "def f(orders, users):\n    for order in orders:\n        for user in users:\n            if order.user_id == user.id:\n                print(order, user)\n";
    assert_eq!(
        pairs(Language::Python, python),
        [("orders".to_owned(), "users".to_owned())]
    );
    let go = "package a\nfunc f(orders []Order, users []User) {\n\tfor _, order := range orders {\n\t\tfor _, user := range users {\n\t\t\tif order.UserID == user.ID { sink(order, user) }\n\t\t}\n\t}\n}\n";
    assert_eq!(
        pairs(Language::Go, go),
        [("orders".to_owned(), "users".to_owned())]
    );
}

#[test]
fn loops_over_one_collection_or_nested_collections_are_not_a_pair_scan() {
    let same = "function f(orders: Order[]) {\n  for (const a of orders) {\n    for (const b of orders) {\n      if (a.id === b.parent) { sink(a, b); }\n    }\n  }\n}\n";
    assert!(pairs(Language::TypeScript, same).is_empty());

    let nested = "function f(groups: Group[]) {\n  for (const group of groups) {\n    for (const member of group.members) {\n      if (group.id === member.group) { sink(group, member); }\n    }\n  }\n}\n";
    assert!(pairs(Language::TypeScript, nested).is_empty());

    let against_a_value = "function f(orders: Order[], users: User[], wanted: number) {\n  for (const order of orders) {\n    for (const user of users) {\n      if (order.userId === wanted) { sink(order, user); }\n    }\n  }\n}\n";
    assert!(pairs(Language::TypeScript, against_a_value).is_empty());
}
