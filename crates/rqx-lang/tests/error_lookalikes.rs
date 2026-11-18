//! Error-level rules on code that resembles their finding, next to the code each one
//! must still report. A false error costs a reader's trust in every other one.

use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::rule::RuleId;

fn count(language: Language, display: &str, source: &str, rule: RuleId) -> usize {
    rqx_lang::analyze_as(language, display, source, &RqxConfig::default())
        .expect("the snippet parses")
        .findings
        .iter()
        .filter(|finding| finding.rule == rule)
        .count()
}

#[test]
fn an_empty_catch_is_reported_until_it_says_why() {
    let cases = [
        (
            Language::TypeScript,
            "a.ts",
            "try { run(); } catch (e) {}\n",
            1,
        ),
        (
            Language::TypeScript,
            "a.ts",
            "try { run(); } catch (e) { ; }\n",
            1,
        ),
        (
            Language::TypeScript,
            "a.ts",
            "try { run(); } catch (e) {\n  // optional, the default applies\n}\n",
            0,
        ),
        (
            Language::JavaScript,
            "a.js",
            "try { run(); } catch { /* best effort */ }\n",
            0,
        ),
        (
            Language::Python,
            "a.py",
            "try:\n    run()\nexcept ValueError:\n    pass\n",
            1,
        ),
        (
            Language::Python,
            "a.py",
            "try:\n    run()\nexcept ValueError:\n    # optional, the default applies\n    pass\n",
            0,
        ),
        (
            Language::Python,
            "a.py",
            "try:\n    run()\nexcept ValueError:  # optional\n    pass\n",
            0,
        ),
        (
            Language::Java,
            "A.java",
            "class A { void f() { try { run(); } catch (Exception e) {} } }\n",
            1,
        ),
        (
            Language::Java,
            "A.java",
            "class A { void f() { try { run(); } catch (Exception e) { // optional\n } } }\n",
            0,
        ),
        (
            Language::Cpp,
            "a.cpp",
            "void f() { try { run(); } catch (...) {} }\n",
            1,
        ),
        (
            Language::Cpp,
            "a.cpp",
            "void f() { try { run(); } catch (...) { /* optional */ } }\n",
            0,
        ),
    ];
    for (language, display, source, expected) in cases {
        assert_eq!(
            count(language, display, source, RuleId::EmptyCatch),
            expected,
            "{source}"
        );
    }
}

#[test]
fn a_focused_or_skipped_test_is_a_registration_on_the_runner_name() {
    let cases = [
        (r#"it.only("t", () => {});"#, RuleId::FocusedTest, 1),
        (r#"test.only("t", () => {});"#, RuleId::FocusedTest, 1),
        (r#"describe.only("s", () => {});"#, RuleId::FocusedTest, 1),
        (r#"fit("t", () => {});"#, RuleId::FocusedTest, 1),
        ("fit(`t ${1}`, () => {});", RuleId::FocusedTest, 1),
        (r#"fdescribe("s", () => {});"#, RuleId::FocusedTest, 1),
        (
            r#"import { fit } from "@jest/globals"; fit("t", () => {});"#,
            RuleId::FocusedTest,
            1,
        ),
        (r#"it.skip("t", () => {});"#, RuleId::SkippedTest, 1),
        (r#"xit("t", () => {});"#, RuleId::SkippedTest, 1),
        (r#"it.todo("x");"#, RuleId::SkippedTest, 1),
        ("fit(bounds, padding);", RuleId::FocusedTest, 0),
        ("xit();", RuleId::SkippedTest, 0),
        (
            "function fit(a, b) { return a + b; }\nfit(1, 2);",
            RuleId::FocusedTest,
            0,
        ),
        (
            r#"const fit = makeFitter(); fit("label", () => 0);"#,
            RuleId::FocusedTest,
            0,
        ),
        (
            r#"const xit = (label, done) => done(label); xit("bye", () => 0);"#,
            RuleId::SkippedTest,
            0,
        ),
    ];
    for (source, rule, expected) in cases {
        assert_eq!(
            count(Language::TypeScript, "a.test.ts", source, rule),
            expected,
            "{source}"
        );
    }
}

#[test]
fn a_local_function_named_like_a_marked_test_leaves_the_file_a_module() {
    let source = "function fit(at: number) { return at; }\n\
                  export function stamp() { return fit(Date.now()); }\n";
    assert_eq!(
        count(
            Language::TypeScript,
            "a.ts",
            source,
            RuleId::AmbientDependency
        ),
        1
    );
}

#[test]
fn a_check_against_itself_is_reported_only_when_neither_side_runs_code() {
    let cases = [
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { const total = add(1, 1); expect(total).toBe(total); });"#,
            1,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { expect(order.total).toBe(order.total); });"#,
            1,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { const sums = [1]; expect(sums).toEqual([...sums]); });"#,
            1,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { const sums = [1]; expect(sums).toEqual(structuredClone(sums)); });"#,
            1,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { expect(add(1, 1)).toBe(add(1, 1)); });"#,
            0,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", async () => { expect(await load()).toEqual(await load()); });"#,
            0,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { expect(new Money(1)).toEqual(new Money(1)); });"#,
            0,
        ),
        (
            Language::TypeScript,
            "a.test.ts",
            r#"it("t", () => { expect(rows()).toEqual([...rows()]); });"#,
            0,
        ),
        (
            Language::Python,
            "test_a.py",
            "def test_t():\n    x = 1\n    assert x == x\n",
            1,
        ),
        (
            Language::Python,
            "test_a.py",
            "def test_t():\n    assert add(1, 1) == add(1, 1)\n",
            0,
        ),
        (
            Language::Java,
            "ATest.java",
            "import org.junit.jupiter.api.Test;\nclass ATest { @Test public void testT() { assertEquals(1, 1); } }\n",
            1,
        ),
        (
            Language::Java,
            "ATest.java",
            "import org.junit.jupiter.api.Test;\nclass ATest { @Test public void testT() { assertEquals(add(1, 1), add(1, 1)); } }\n",
            0,
        ),
        (
            Language::Cpp,
            "a_test.cpp",
            "TEST(A, B) { EXPECT_EQ(1, 1); }\n",
            1,
        ),
        (
            Language::Cpp,
            "a_test.cpp",
            "TEST(A, B) { EXPECT_EQ(add(1, 1), add(1, 1)); }\n",
            0,
        ),
        (
            Language::Go,
            "a_test.go",
            "package a\n\nfunc TestT(t *testing.T) {\n\ttotal := 1\n\trequire.Equal(t, total, total)\n}\n",
            1,
        ),
        (
            Language::Go,
            "a_test.go",
            "package a\n\nfunc TestT(t *testing.T) {\n\trequire.Equal(t, Add(1, 1), Add(1, 1))\n}\n",
            0,
        ),
    ];
    for (language, display, source, expected) in cases {
        assert_eq!(
            count(language, display, source, RuleId::SelfAssertingTest),
            expected,
            "{source}"
        );
    }
}
