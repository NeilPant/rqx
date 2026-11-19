use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::rule::RuleId;

fn rules(source: &str) -> Vec<RuleId> {
    rqx_lang::analyze_as(
        Language::Cpp,
        "catch2_test.cpp",
        source,
        &RqxConfig::default(),
    )
    .expect("framework macros are readable C++")
    .findings
    .into_iter()
    .map(|finding| finding.rule)
    .collect()
}

#[test]
fn quoted_framework_cases_are_checked_like_declared_tests() {
    for header in [
        "TEST_CASE(\"does nothing\")",
        "TEST_CASE_METHOD(Fixture, \"does nothing\", \"[tag]\")",
        "TEMPLATE_TEST_CASE(\"does nothing\", \"[tag]\", int, double)",
        "SCENARIO(\"does nothing\")",
        "SUBCASE(\"does nothing\")",
        "SECTION(\"does nothing\")",
    ] {
        let empty = rules(&format!("{header} {{}}"));
        assert!(empty.contains(&RuleId::EmptyTest), "{header}: {empty:?}");
        let unchecked = rules(&format!(
            "{header} {{ int value = add(1, 2); (void)value; }}"
        ));
        assert!(
            unchecked.contains(&RuleId::AssertionlessTest),
            "{header}: {unchecked:?}"
        );
        for check in [
            "REQUIRE(true)",
            "CHECK(true)",
            "REQUIRE_THROWS(work())",
            "STATIC_REQUIRE(true)",
        ] {
            let checked = rules(&format!("{header} {{ {check}; }}"));
            assert!(
                !checked.contains(&RuleId::AssertionlessTest),
                "{header}: {checked:?}"
            );
            assert!(
                !checked.contains(&RuleId::EmptyTest),
                "{header}: {checked:?}"
            );
        }
    }
}

#[test]
fn suite_and_bdd_containers_assert_through_their_nested_cases() {
    let source = r#"
TEST_SUITE("doctest suite") {
    TEST_CASE("checks subcases") {
        SUBCASE("checks a value") { CHECK(true); }
    }
}
SCENARIO("behaviour") {
    GIVEN("a state") { WHEN("an action") { THEN("the result") { REQUIRE(true); } } }
}
TEST_CASE("catch sections") { SECTION("a check") { REQUIRE(true); } }
"#;
    let found = rules(source);
    assert!(!found.contains(&RuleId::AssertionlessTest), "{found:?}");
    assert!(!found.contains(&RuleId::EmptyTest), "{found:?}");
}

#[test]
fn separate_template_cases_keep_their_bodies_and_source_locations() {
    let source = "TEMPLATE_TEST_CASE(\"empty\", \"\", int) {}\nTEMPLATE_TEST_CASE(\"checks\", \"\", int) { CHECK(true); }\n";
    let analysis = rqx_lang::analyze_as(
        Language::Cpp,
        "cases_test.cpp",
        source,
        &RqxConfig::default(),
    )
    .unwrap();
    let found: Vec<_> = analysis
        .findings
        .iter()
        .filter(|f| f.rule == RuleId::EmptyTest)
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].location.line, 1);
}

#[test]
fn strings_comments_and_ordinary_calls_do_not_register_cases() {
    for source in [
        "// TEST_CASE(\"example\") {}\nint answer() { return 42; }",
        "const char* text = R\"(TEST_CASE(\"example\") {})\";",
        "void f() { TEST_CASE(\"not a declaration\"); {} }",
        "OTHER_CASE(\"not a framework\") {}",
    ] {
        let found = rules(source);
        assert!(!found.contains(&RuleId::EmptyTest), "{source}: {found:?}");
        assert!(
            !found.contains(&RuleId::AssertionlessTest),
            "{source}: {found:?}"
        );
    }
}

#[test]
fn concatenated_labels_skip_calls_and_debug_output_keep_their_test_meaning() {
    let named = rules("TEST_CASE(\"works\" \" for positive values\") { CHECK(true); }");
    assert!(!named.contains(&RuleId::UnnamedTest), "{named:?}");
    let skipped = rules("TEST_CASE(\"temporarily skipped\") { SKIP(\"not ready\"); }");
    assert!(skipped.contains(&RuleId::SkippedTest), "{skipped:?}");
    let debug = rules("TEST_CASE(\"prints a value\") { printf(\"value\"); CHECK(true); }");
    assert!(debug.contains(&RuleId::DebugOutputInTest), "{debug:?}");
}
