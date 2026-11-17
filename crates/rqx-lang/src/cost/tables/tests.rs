use std::collections::HashSet;

use rqx_core::Language;
use rqx_core::config::Performance;

use super::*;

fn performance(yaml: &str) -> Performance {
    serde_saphyr::from_str(yaml).expect("the configuration reads")
}

fn cost_of(found: Option<(u16, &Entry)>) -> Option<String> {
    found.map(|(_, entry)| entry.cost.to_string())
}

#[test]
fn the_built_in_tables_read_and_name_every_entry_once() {
    let tables = CostTables::built_in();
    let mut seen = HashSet::new();
    for (_, text) in BUILT_IN {
        let entries: Vec<CostEntry> = serde_saphyr::from_str(text).expect("a table reads");
        for entry in entries {
            assert!(
                seen.insert(entry.id.clone()),
                "`{}` is in two tables",
                entry.id
            );
        }
    }
    assert!(seen.len() > 300, "{} entries", seen.len());
    assert!(tables.entry(0).is_some());
}

#[test]
fn a_typed_receiver_is_looked_up_by_its_head() {
    let tables = CostTables::built_in();
    let call = CallFacts::NONE;
    // The same entry answers for every spelling of the type.
    assert_eq!(
        cost_of(tables.method(Language::TypeScript, "Array", "includes", &call)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.method(Language::Python, "list", "index", &call)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.method(Language::Java, "LinkedList", "get", &call)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.method(Language::Java, "ArrayList", "get", &call)).as_deref(),
        Some("O(1)")
    );
    assert_eq!(
        cost_of(tables.method(Language::Cpp, "vector", "insert", &call)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.method(Language::Cpp, "map", "find", &call)).as_deref(),
        Some("O(log n)")
    );
    assert!(
        tables
            .method(Language::Python, "list", "no_such", &call)
            .is_none()
    );
}

#[test]
fn keyed_types_hit_entries_that_cost_nothing() {
    let tables = CostTables::built_in();
    let call = CallFacts::NONE;
    for (language, head, method) in [
        (Language::JavaScript, "Set", "has"),
        (Language::TypeScript, "Map", "get"),
        (Language::Python, "dict", "get"),
        (Language::Python, "set", "__contains__"),
        (Language::Java, "HashMap", "containsKey"),
        (Language::Cpp, "unordered_map", "find"),
    ] {
        let (_, entry) = tables
            .method(language, head, method, &call)
            .unwrap_or_else(|| panic!("{language:?} {head}.{method}"));
        assert!(entry.cost.is_one(), "{head}.{method}");
    }
}

#[test]
fn an_unknown_receiver_gets_only_a_name_that_means_one_thing() {
    let tables = CostTables::built_in();
    let call = CallFacts::NONE;
    assert_eq!(
        cost_of(tables.any_method(Language::JavaScript, "indexOf", &call)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.any_method(Language::JavaScript, "sort", &call)).as_deref(),
        Some("O(n log n)")
    );
    // `get`, `has` and `add` mean different things on different types.
    for method in ["get", "has", "add", "remove"] {
        assert!(
            tables
                .any_method(Language::JavaScript, method, &call)
                .is_none(),
            "{method}"
        );
        assert!(
            tables.any_method(Language::Python, method, &call).is_none(),
            "{method}"
        );
    }
}

#[test]
fn conditions_on_the_arguments_choose_between_entries() {
    let tables = CostTables::built_in();
    let nothing = CallFacts { literals: &[] };
    let zero = CallFacts {
        literals: &[Some("0")],
    };
    let some_index = CallFacts { literals: &[None] };
    let cost = |call: &CallFacts<'_>| {
        cost_of(tables.method(Language::Python, "list", "pop", call)).unwrap()
    };
    assert_eq!(cost(&nothing), "O(1)");
    assert_eq!(cost(&zero), "O(n)");
    assert_eq!(cost(&some_index), "O(n)");

    let insert_at = CallFacts {
        literals: &[None, None],
    };
    assert_eq!(
        cost_of(tables.method(Language::Java, "ArrayList", "add", &insert_at)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.method(Language::Java, "ArrayList", "add", &some_index)).as_deref(),
        Some("O(1)")
    );
}

#[test]
fn a_free_function_is_found_however_its_namespace_is_written() {
    let tables = CostTables::built_in();
    let call = CallFacts::NONE;
    for name in ["std::find", "std.find", "find"] {
        assert_eq!(
            cost_of(tables.function(Language::Cpp, name, &call)).as_deref(),
            Some("O(n)"),
            "{name}"
        );
    }
    assert_eq!(
        cost_of(tables.function(Language::Go, "sort.Ints", &call)).as_deref(),
        Some("O(n log n)")
    );
    let (_, entry) = tables.function(Language::Go, "copy", &call).unwrap();
    assert_eq!(entry.size_of, SizeOf::Arg(1));
    assert!(tables.function(Language::Go, "mine.Sort", &call).is_none());
}

#[test]
fn an_entry_is_read_for_the_languages_its_table_names() {
    let tables = CostTables::built_in();
    let call = CallFacts::NONE;
    assert!(
        tables
            .method(Language::TypeScript, "Uint8Array", "sort", &call)
            .is_some()
    );
    assert!(
        tables
            .method(Language::JavaScript, "Uint8Array", "sort", &call)
            .is_none()
    );
    assert!(
        tables
            .method(Language::Python, "Array", "includes", &call)
            .is_none()
    );
}

#[test]
fn a_project_entry_is_tried_before_the_built_in_one() {
    let user = performance(
        "costs:\n  - {id: mine.has, receiver: [Set], method: has, cost: n}\n  \
         - {id: mine.fetchAll, fn: repo.fetchAll, cost: n log n, space: n}\n  \
         - {id: mine.includes, receiver: [\"T[]\"], method: includes, cost: 1}\n",
    );
    let tables = CostTables::for_performance(&user);
    let call = CallFacts::NONE;
    assert_eq!(
        cost_of(tables.method(Language::JavaScript, "Set", "has", &call)).as_deref(),
        Some("O(n)")
    );
    assert_eq!(
        cost_of(tables.function(Language::Java, "repo.fetchAll", &call)).as_deref(),
        Some("O(n log n)")
    );
    // `T[]` is `Array` in TypeScript, so the project's entry shadows the built-in one.
    assert_eq!(
        cost_of(tables.method(Language::TypeScript, "Array", "includes", &call)).as_deref(),
        Some("O(1)")
    );
}

#[test]
fn an_unreadable_condition_switches_its_entry_off() {
    let user = performance(
        "costs:\n  - {id: mine.odd, receiver: [list], method: odd, when: {first: weird}, cost: n}\n",
    );
    let tables = CostTables::for_performance(&user);
    assert!(
        tables
            .method(Language::Python, "list", "odd", &CallFacts::NONE)
            .is_none()
    );
}

#[test]
fn the_version_follows_the_entries() {
    let none = CostTables::for_performance(&Performance::default());
    assert!(Arc::ptr_eq(&none, &CostTables::built_in()));

    let one = performance("costs:\n  - {id: a.b, receiver: [list], method: b, cost: n}\n");
    let other = performance("costs:\n  - {id: a.b, receiver: [list], method: b, cost: 1}\n");
    let first = CostTables::for_performance(&one).version();
    assert_ne!(first, none.version());
    assert_ne!(first, CostTables::for_performance(&other).version());
    assert_eq!(first, CostTables::for_performance(&one).version());
}
