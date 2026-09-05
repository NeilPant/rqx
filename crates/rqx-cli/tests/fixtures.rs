use std::collections::BTreeMap;
use std::process::Command;

/// Every language gets its own scan root, with its own entry-point manifest and its
/// own exact rule counts. One shared root would make a Python fixture change a
/// TypeScript expectation, which is a test that fails for the wrong reason.
fn language_dir(language: &str) -> String {
    format!("{}/../../fixtures/{language}", env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_dir() -> String {
    format!("{}/../../fixtures", env!("CARGO_MANIFEST_DIR"))
}

fn config_path() -> String {
    format!("{}/all-rules.toml", fixtures_dir())
}

fn scan_dir(dir: &str, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", dir, "--format", "json", "--max-findings", "0"])
        .args(extra)
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success(), "rqx exited with {}", output.status);
    String::from_utf8(output.stdout).expect("output was not utf-8")
}

fn scan_language(language: &str, extra: &[&str]) -> String {
    scan_dir(&language_dir(language), extra)
}

fn scan(extra: &[&str]) -> String {
    scan_language("typescript", extra)
}

fn counts_by_rule(report: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for line in report
        .lines()
        .filter(|line| !line.contains("\"kind\":\"summary\""))
    {
        let rule = line
            .split("\"rule\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("no rule in: {line}"));
        *counts.entry(rule.to_owned()).or_default() += 1;
    }
    counts
}

/// The summary line carries every rule name inside its counts, so a plain substring
/// search over the whole report matches it too and inflates every total by one.
fn findings_for(report: &str, rule: &str) -> Vec<String> {
    let needle = format!("\"rule\":\"{rule}\"");
    report
        .lines()
        .filter(|line| !line.contains("\"kind\":\"summary\"") && line.contains(&needle))
        .map(str::to_owned)
        .collect()
}

fn summary_field(report: &str, field: &str) -> String {
    let summary = report
        .lines()
        .find(|line| line.contains("\"kind\":\"summary\""))
        .unwrap();
    summary
        .split(&format!("\"{field}\":"))
        .nth(1)
        .and_then(|rest| rest.split(&[',', '}'][..]).next())
        .unwrap()
        .to_owned()
}

/// Exact counts per language, which is what catches a rule that starts firing twice
/// or stops firing at all. The per-language tables are separate because a fixture
/// change in one language must not move another language's expectation.
#[test]
fn every_rule_fires_the_number_of_times_its_fixtures_ask_for() {
    let expected: &[(&str, &[(&str, u32)])] = &[
        (
            "typescript",
            &[
                ("accumulate-by-copy", 1),
                ("ambiguous-precedence", 1),
                ("apology-comment", 1),
                ("assertionless-test", 1),
                ("assignment-in-condition", 1),
                ("await-in-loop", 1),
                ("banner-comment", 1),
                ("base-depends-on-derived", 1),
                ("blocking-call-in-async", 1),
                ("boolean-from-branch", 1),
                ("boundary-arithmetic", 1),
                ("branching-recursion", 1),
                ("buried-configuration", 1),
                ("callback-nesting", 1),
                ("chat-residue", 1),
                ("circular-dependency", 1),
                ("cognitive-complexity", 2),
                ("collapsible-if", 2),
                ("commented-out-code", 1),
                ("complex-condition", 1),
                ("data-class", 1),
                ("data-clump", 1),
                ("deep-loop-nesting", 1),
                ("deep-nesting", 2),
                ("deep-relative-import", 1),
                ("discarded-expression", 1),
                ("distant-declaration", 2),
                ("duplicate-code", 1),
                ("duplicate-import", 1),
                ("empty-block", 1),
                ("empty-catch", 1),
                ("empty-method", 13),
                ("example-value", 1),
                ("feature-envy", 2),
                ("flag-argument", 7),
                ("float-equality", 1),
                ("focused-test", 1),
                ("identical-branches", 1),
                ("imprecise-type", 3),
                ("inappropriate-intimacy", 1),
                ("inappropriate-static", 1),
                ("invariant-in-loop", 1),
                ("journal-comment", 1),
                ("large-class", 1),
                ("lazy-class", 1),
                ("literal-comparison", 1),
                ("long-function", 2),
                ("long-parameter-list", 2),
                ("loop-runs-once", 1),
                ("magic-number", 4),
                ("many-exits", 3),
                ("message-chain", 2),
                ("middle-man", 10),
                ("mixed-conventions", 1),
                ("mixed-language-file", 1),
                ("negated-comparison", 1),
                ("nested-ternary", 1),
                ("no-default-branch", 2),
                ("obscured-expression", 1),
                ("output-parameter", 2),
                ("passthrough-function", 4),
                ("placeholder-body", 1),
                ("query-in-loop", 1),
                ("query-with-side-effect", 1),
                ("redundant-comment", 1),
                ("redundant-else", 10),
                ("redundant-jump", 1),
                ("refused-bequest", 1),
                ("rethrow-only-catch", 1),
                ("scan-in-loop", 1),
                ("self-comparison", 1),
                ("skipped-test", 1),
                ("slow-test", 1),
                ("speculative-generality", 2),
                ("suppressed-diagnostic", 1),
                ("temporal-coupling", 1),
                ("temporary-field", 1),
                ("type-code-switch", 4),
                ("unjustified-suppression", 1),
                ("unreachable-code", 1),
                ("unused-export", 2),
                ("unused-file", 1),
                ("unused-import", 2),
                ("unused-local", 2),
                ("unused-parameter", 20),
                ("unused-private-member", 5),
                ("vague-function-name", 2),
                ("wide-fan-out", 1),
                ("wide-surface", 1),
                ("wide-switch", 1),
                ("wildcard-import", 1),
            ],
        ),
        (
            "python",
            &[
                ("accumulate-by-copy", 1),
                ("ambiguous-precedence", 1),
                ("apology-comment", 1),
                ("assertionless-test", 2),
                ("await-in-loop", 1),
                ("banner-comment", 1),
                ("base-depends-on-derived", 1),
                ("blocking-call-in-async", 1),
                ("boolean-from-branch", 1),
                ("boundary-arithmetic", 1),
                ("branching-recursion", 1),
                ("buried-configuration", 1),
                ("callback-nesting", 1),
                ("chat-residue", 1),
                ("circular-dependency", 1),
                ("cognitive-complexity", 2),
                ("collapsible-if", 2),
                ("commented-out-code", 1),
                ("complex-condition", 1),
                ("data-class", 2),
                ("data-clump", 1),
                ("deep-loop-nesting", 1),
                ("deep-nesting", 2),
                ("deep-relative-import", 1),
                ("discarded-expression", 1),
                ("distant-declaration", 1),
                ("duplicate-code", 4),
                ("duplicate-import", 2),
                ("empty-catch", 1),
                ("empty-method", 1),
                ("example-value", 1),
                ("feature-envy", 1),
                ("flag-argument", 3),
                ("float-equality", 1),
                ("identical-branches", 1),
                ("imprecise-type", 4),
                ("inappropriate-intimacy", 1),
                ("inappropriate-static", 1),
                ("invariant-in-loop", 1),
                ("journal-comment", 2),
                ("large-class", 1),
                ("lazy-class", 3),
                ("literal-comparison", 1),
                ("long-function", 2),
                ("long-parameter-list", 2),
                ("loop-runs-once", 1),
                ("magic-number", 4),
                ("many-exits", 3),
                ("message-chain", 2),
                ("middle-man", 6),
                ("mixed-conventions", 3),
                ("mixed-language-file", 1),
                ("negated-comparison", 1),
                ("nested-ternary", 1),
                ("no-default-branch", 2),
                ("obscured-expression", 1),
                ("output-parameter", 7),
                ("passthrough-function", 5),
                ("placeholder-body", 2),
                ("query-in-loop", 1),
                ("query-with-side-effect", 1),
                ("redundant-comment", 1),
                ("redundant-else", 10),
                ("redundant-jump", 1),
                ("refused-bequest", 1),
                ("rethrow-only-catch", 1),
                ("scan-in-loop", 1),
                ("self-comparison", 1),
                ("skipped-test", 1),
                ("slow-test", 1),
                ("speculative-generality", 2),
                ("suppressed-diagnostic", 1),
                ("temporal-coupling", 1),
                ("temporary-field", 4),
                ("type-code-switch", 4),
                ("unjustified-suppression", 1),
                ("unreachable-code", 1),
                ("unused-export", 11),
                ("unused-file", 1),
                ("unused-import", 3),
                ("unused-local", 2),
                ("unused-parameter", 17),
                ("unused-private-member", 5),
                ("vague-function-name", 2),
                ("wide-fan-out", 1),
                ("wide-surface", 1),
                ("wide-switch", 1),
                ("wildcard-import", 1),
            ],
        ),
        (
            "go",
            &[
                ("accumulate-by-copy", 1),
                ("ambiguous-precedence", 1),
                ("apology-comment", 1),
                ("assertionless-test", 3),
                ("banner-comment", 1),
                ("boolean-from-branch", 1),
                ("boundary-arithmetic", 1),
                ("branching-recursion", 1),
                ("buried-configuration", 1),
                ("callback-nesting", 1),
                ("chat-residue", 1),
                ("circular-dependency", 1),
                ("cognitive-complexity", 1),
                ("collapsible-if", 2),
                ("commented-out-code", 1),
                ("complex-condition", 1),
                ("data-class", 8),
                ("data-clump", 1),
                ("deep-loop-nesting", 1),
                ("deep-nesting", 2),
                ("distant-declaration", 2),
                ("duplicate-code", 1),
                ("duplicate-import", 1),
                ("empty-block", 1),
                ("empty-method", 8),
                ("example-value", 6),
                ("feature-envy", 2),
                ("flag-argument", 7),
                ("float-equality", 1),
                ("identical-branches", 1),
                ("imprecise-type", 4),
                ("inappropriate-intimacy", 1),
                ("invariant-in-loop", 1),
                ("journal-comment", 2),
                ("large-class", 1),
                ("lazy-class", 7),
                ("literal-comparison", 1),
                ("long-function", 3),
                ("long-parameter-list", 2),
                ("loop-runs-once", 1),
                ("magic-number", 3),
                ("many-exits", 2),
                ("message-chain", 1),
                ("middle-man", 1),
                ("mixed-conventions", 1),
                ("mixed-language-file", 1),
                ("negated-comparison", 1),
                ("no-default-branch", 2),
                ("obscured-expression", 1),
                ("output-parameter", 2),
                ("passthrough-function", 6),
                ("query-in-loop", 1),
                ("query-with-side-effect", 1),
                ("redundant-comment", 1),
                ("redundant-else", 14),
                ("redundant-jump", 1),
                ("scan-in-loop", 1),
                ("self-comparison", 1),
                ("skipped-test", 1),
                ("slow-test", 1),
                ("speculative-generality", 1),
                ("suppressed-diagnostic", 1),
                ("temporal-coupling", 1),
                ("temporary-field", 7),
                ("type-code-switch", 3),
                ("unjustified-suppression", 1),
                ("unreachable-code", 1),
                ("unused-export", 7),
                ("unused-file", 1),
                ("unused-local", 1),
                ("unused-parameter", 26),
                ("vague-function-name", 2),
                ("wide-fan-out", 1),
                ("wide-surface", 1),
                ("wide-switch", 1),
                ("wildcard-import", 1),
            ],
        ),
        (
            "java",
            &[
                ("accumulate-by-copy", 1),
                ("ambiguous-precedence", 1),
                ("apology-comment", 1),
                ("assertionless-test", 2),
                ("assignment-in-condition", 1),
                ("banner-comment", 1),
                ("base-depends-on-derived", 1),
                ("boolean-from-branch", 1),
                ("boundary-arithmetic", 1),
                ("branching-recursion", 1),
                ("buried-configuration", 1),
                ("callback-nesting", 1),
                ("chat-residue", 1),
                ("circular-dependency", 1),
                ("cognitive-complexity", 1),
                ("collapsible-if", 2),
                ("commented-out-code", 1),
                ("complex-condition", 1),
                ("data-class", 2),
                ("data-clump", 1),
                ("deep-loop-nesting", 1),
                ("deep-nesting", 2),
                ("distant-declaration", 2),
                ("duplicate-code", 1),
                ("duplicate-import", 1),
                ("empty-block", 1),
                ("empty-catch", 1),
                ("empty-method", 9),
                ("example-value", 1),
                ("feature-envy", 5),
                ("flag-argument", 7),
                ("float-equality", 1),
                ("identical-branches", 2),
                ("imprecise-type", 5),
                ("inappropriate-intimacy", 1),
                ("inappropriate-static", 1),
                ("invariant-in-loop", 1),
                ("journal-comment", 1),
                ("large-class", 1),
                ("lazy-class", 8),
                ("literal-comparison", 1),
                ("long-function", 3),
                ("long-parameter-list", 2),
                ("loop-runs-once", 1),
                ("magic-number", 2),
                ("many-exits", 2),
                ("message-chain", 1),
                ("middle-man", 9),
                ("mixed-conventions", 1),
                ("mixed-language-file", 1),
                ("negated-comparison", 1),
                ("nested-ternary", 1),
                ("no-default-branch", 2),
                ("obscured-expression", 1),
                ("output-parameter", 2),
                ("passthrough-function", 4),
                ("placeholder-body", 1),
                ("query-in-loop", 1),
                ("query-with-side-effect", 1),
                ("redundant-comment", 1),
                ("redundant-else", 11),
                ("redundant-jump", 1),
                ("refused-bequest", 1),
                ("rethrow-only-catch", 1),
                ("scan-in-loop", 1),
                ("self-comparison", 1),
                ("skipped-test", 1),
                ("slow-test", 1),
                ("speculative-generality", 2),
                ("suppressed-diagnostic", 1),
                ("temporal-coupling", 1),
                ("temporary-field", 1),
                ("type-code-switch", 4),
                ("unjustified-suppression", 1),
                ("unreachable-code", 1),
                ("unused-export", 22),
                ("unused-file", 2),
                ("unused-import", 1),
                ("unused-local", 2),
                ("unused-parameter", 28),
                ("unused-private-member", 5),
                ("vague-function-name", 2),
                ("wide-fan-out", 1),
                ("wide-surface", 1),
                ("wide-switch", 1),
                ("wildcard-import", 1),
            ],
        ),
        (
            "cpp",
            &[
                ("accumulate-by-copy", 1),
                ("ambiguous-precedence", 1),
                ("apology-comment", 1),
                ("assertionless-test", 1),
                ("assignment-in-condition", 1),
                ("await-in-loop", 1),
                ("banner-comment", 1),
                ("base-depends-on-derived", 1),
                ("boolean-from-branch", 1),
                ("boundary-arithmetic", 1),
                ("branching-recursion", 1),
                ("buried-configuration", 1),
                ("callback-nesting", 1),
                ("chat-residue", 1),
                ("circular-dependency", 1),
                ("cognitive-complexity", 1),
                ("collapsible-if", 2),
                ("commented-out-code", 1),
                ("complex-condition", 1),
                ("data-class", 2),
                ("data-clump", 1),
                ("deep-loop-nesting", 1),
                ("deep-nesting", 2),
                ("deep-relative-import", 1),
                ("discarded-expression", 1),
                ("distant-declaration", 2),
                ("duplicate-code", 1),
                ("duplicate-import", 1),
                ("empty-block", 1),
                ("empty-catch", 1),
                ("empty-method", 9),
                ("example-value", 1),
                ("feature-envy", 1),
                ("flag-argument", 7),
                ("float-equality", 1),
                ("identical-branches", 1),
                ("imprecise-type", 2),
                ("inappropriate-intimacy", 1),
                ("inappropriate-static", 1),
                ("invariant-in-loop", 1),
                ("journal-comment", 2),
                ("large-class", 1),
                ("lazy-class", 2),
                ("literal-comparison", 1),
                ("long-function", 3),
                ("long-parameter-list", 2),
                ("loop-runs-once", 1),
                ("magic-number", 3),
                ("many-exits", 2),
                ("message-chain", 1),
                ("middle-man", 7),
                ("mixed-conventions", 4),
                ("mixed-language-file", 1),
                ("negated-comparison", 1),
                ("nested-ternary", 1),
                ("no-default-branch", 1),
                ("obscured-expression", 1),
                ("output-parameter", 2),
                ("passthrough-function", 4),
                ("placeholder-body", 1),
                ("query-in-loop", 1),
                ("query-with-side-effect", 1),
                ("redundant-comment", 1),
                ("redundant-else", 14),
                ("redundant-jump", 1),
                ("refused-bequest", 1),
                ("rethrow-only-catch", 1),
                ("scan-in-loop", 1),
                ("self-comparison", 1),
                ("skipped-test", 1),
                ("slow-test", 1),
                ("speculative-generality", 7),
                ("suppressed-diagnostic", 1),
                ("temporal-coupling", 1),
                ("temporary-field", 6),
                ("type-code-switch", 4),
                ("unjustified-suppression", 1),
                ("unreachable-code", 1),
                ("unused-export", 48),
                ("unused-file", 1),
                ("unused-local", 5),
                ("unused-parameter", 17),
                ("unused-private-member", 9),
                ("vague-function-name", 2),
                ("wide-fan-out", 1),
                ("wide-surface", 1),
                ("wide-switch", 1),
                ("wildcard-import", 1),
            ],
        ),
    ];

    let mut wrong = Vec::new();
    for (language, table) in expected {
        let counts = counts_by_rule(&scan_language(language, &[]));
        for (rule, count) in *table {
            let actual = counts.get(*rule).copied().unwrap_or(0);
            if actual != *count {
                wrong.push(format!("{language} {rule}: expected {count}, got {actual}"));
            }
        }
        for rule in counts.keys() {
            if !table.iter().any(|(known, _)| known == rule) {
                wrong.push(format!("{language} {rule}: fired but is not in the table"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn clean_code_produces_no_findings() {
    let report = scan(&[]);
    assert!(
        !report.lines().any(|line| line.contains("/clean.ts")),
        "clean.ts should be silent"
    );
}

/// The whole value of the module graph is knowing the difference between an export
/// nobody wants and one the project is entered through.
#[test]
fn an_export_reached_from_the_entry_point_is_not_dead() {
    let report = scan(&[]);
    for line in findings_for(&report, "unused-export") {
        for alive in ["reachable", "orbit", "add", "describe"] {
            assert!(
                !line.contains(&format!("\"symbol\":\"{alive}\"")),
                "{alive} is re-exported from the entry point: {line}"
            );
        }
    }
    let dead = findings_for(&report, "unused-export");
    assert_eq!(
        dead.len(),
        2,
        "expected neverImported and AlsoNeverImported"
    );
    assert!(dead.iter().all(|line| line.contains("dead-export.ts")));
}

#[test]
fn a_file_nothing_imports_is_one_finding_not_one_per_export() {
    let report = scan(&[]);
    let files = findings_for(&report, "unused-file");
    assert_eq!(files.len(), 1);
    assert!(files[0].contains("orphan.ts"), "{}", files[0]);
    assert!(files[0].contains("\"exports\":2"), "{}", files[0]);
}

#[test]
fn an_import_cycle_names_every_module_in_it() {
    let report = scan(&[]);
    let cycles = findings_for(&report, "circular-dependency");
    assert_eq!(cycles.len(), 1, "one finding per cycle, not one per edge");
    assert!(cycles[0].contains("\"modules\":3"));
    assert!(cycles[0].contains("cycle-a.ts"));
    assert!(cycles[0].contains("cycle-b.ts") && cycles[0].contains("cycle-c.ts"));

    // The ring is the way round, so it closes and it names one more module than the
    // cycle has. A set of names would satisfy the assertions above and still leave a
    // reader guessing which import to delete.
    let ring: Vec<String> =
        serde_json::from_str::<serde_json::Value>(&cycles[0]).expect("json")["evidence"]["ring"]
            .as_array()
            .expect("a ring")
            .iter()
            .map(|member| member.as_str().expect("a path").to_owned())
            .collect();
    assert_eq!(ring.len(), 4, "{ring:?}");
    assert_eq!(ring.first(), ring.last(), "{ring:?}");
    assert_eq!(
        ring.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3,
        "each module appears once until it closes: {ring:?}"
    );

    // And it underlines the import that opens the ring rather than the whole file,
    // which is the difference between naming the edge to delete and naming a file.
    let at = &serde_json::from_str::<serde_json::Value>(&cycles[0]).expect("json")["location"];
    let (start, end) = (
        at["start"].as_u64().expect("start"),
        at["end"].as_u64().expect("end"),
    );
    assert!(end > start, "an empty span is the whole-file anchor: {at}");
    let source = std::fs::read_to_string(format!("{}/cycle-a.ts", language_dir("typescript")))
        .expect("the fixture");
    assert!(
        source[start as usize..end as usize].starts_with("import"),
        "underlined `{}`",
        &source[start as usize..end as usize]
    );
}

/// A trailing parameter can just be deleted; one with parameters after it cannot,
/// and an underscore-prefixed one is the author saying they already know.
#[test]
fn unused_parameters_are_graded_by_whether_deleting_one_is_safe() {
    let report = scan(&[]);
    let params = findings_for(&report, "unused-parameter");
    assert!(
        !params.iter().any(|line| line.contains("_deliberate")),
        "an underscore prefix is an explicit annotation, not a finding"
    );
    let trailing: Vec<&String> = params
        .iter()
        .filter(|line| line.contains("\"trailing\":true"))
        .collect();
    let middle: Vec<&String> = params
        .iter()
        .filter(|line| line.contains("\"trailing\":false"))
        .collect();
    assert!(!trailing.is_empty() && !middle.is_empty());
    assert!(
        trailing
            .iter()
            .all(|line| line.contains("\"confidence\":\"verified\"")
                || line.contains("\"confidence\":\"heuristic\"")),
        "trailing params carry a confidence"
    );
    assert!(
        middle
            .iter()
            .all(|line| line.contains("\"confidence\":\"heuristic\"")),
        "a parameter that cannot be safely deleted must not claim to be verified"
    );
}

#[test]
fn the_low_confidence_rule_reports_only_when_enabled() {
    let config = config_path();
    let counts = counts_by_rule(&scan(&["--config", &config]));
    assert_eq!(counts.get("meaningless-name").copied().unwrap_or(0), 2);
}

/// A CI gate on a finding set that shifts with the machine's core count is noise,
/// so thread count must not be observable in the output. The fixtures deliberately
/// contain two unused parameters on one line, which tie on rule, file and line and
/// so catch an incomplete sort key.
#[test]
fn output_does_not_depend_on_thread_count() {
    let findings = |report: String| {
        report
            .lines()
            .filter(|line| !line.contains("\"kind\":\"summary\""))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        findings(scan(&["--threads", "1"])),
        findings(scan(&["--threads", "16"]))
    );
}

#[test]
fn the_display_cap_never_moves_the_health_score() {
    let uncapped = scan(&[]);
    let capped = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args([
            "scan",
            &language_dir("typescript"),
            "--format",
            "json",
            "--max-findings",
            "1",
        ])
        .output()
        .expect("failed to run rqx");
    let capped = String::from_utf8(capped.stdout).unwrap();

    assert_eq!(
        summary_field(&uncapped, "health_score"),
        summary_field(&capped, "health_score")
    );
    assert_eq!(
        summary_field(&uncapped, "counts"),
        summary_field(&capped, "counts")
    );
    assert_eq!(capped.lines().filter(|l| !l.contains("summary")).count(), 1);
}

#[test]
fn a_finding_always_carries_its_taxonomy_and_a_named_fix() {
    for line in scan(&[])
        .lines()
        .filter(|line| !line.contains("\"kind\":\"summary\""))
    {
        for field in [
            "\"why\":\"",
            "\"category\":\"",
            "\"metric\":\"",
            "\"fix_hint\":\"",
        ] {
            let value = line
                .split(field)
                .nth(1)
                .and_then(|rest| rest.split('"').next());
            assert!(
                value.is_some_and(|text| !text.is_empty()),
                "a finding shipped without {field}: {line}"
            );
        }
        assert!(
            line.contains("\"fixes\":[\""),
            "a finding shipped with no fix technique: {line}"
        );
    }
}

fn advise_language(language: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["patterns", &language_dir(language), "--json"])
        .args(["--config", &config_path()])
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success(), "rqx exited with {}", output.status);
    String::from_utf8(output.stdout).expect("output was not utf-8")
}

fn counts_by_advisory(report: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for line in report.lines().filter(|line| !line.is_empty()) {
        let advisory = line
            .split("\"advisory\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("no advisory in: {line}"));
        *counts.entry(advisory.to_owned()).or_default() += 1;
    }
    counts
}

/// The same bidirectional table the rules get: an advisory that stops firing fails,
/// and so does one that starts firing without being written down. A pattern claim is
/// worth less than a rule if nobody pinned what it says about known code.
#[test]
fn every_advisory_fires_the_number_of_times_its_fixtures_ask_for() {
    let expected: &[(&str, &[(&str, u32)])] = &[
        (
            "typescript",
            &[
                ("abstract-factory-found", 1),
                ("adapter-found", 1),
                ("bridge-fits", 1),
                ("builder-found", 1),
                ("builder-without-build", 1),
                ("chain-of-responsibility-fits", 1),
                ("command-fits", 1),
                ("composite-found", 1),
                ("decorator-found", 1),
                ("decorator-not-substitutable", 2),
                ("facade-fits", 1),
                ("factory-method-found", 1),
                ("flyweight-found", 1),
                ("interpreter-found", 1),
                ("iterator-found", 1),
                ("mediator-fits", 1),
                ("memento-found", 1),
                ("observer-found", 1),
                ("observer-without-removal", 1),
                ("prototype-found", 1),
                ("proxy-found", 1),
                ("singleton-escapable", 1),
                ("singleton-found", 1),
                ("state-fits", 1),
                ("strategy-fits", 2),
                ("template-method-found", 1),
                ("visitor-found", 1),
            ],
        ),
        (
            "python",
            &[
                ("abstract-factory-found", 1),
                ("adapter-found", 1),
                ("bridge-fits", 1),
                ("builder-found", 1),
                ("builder-without-build", 1),
                ("chain-of-responsibility-fits", 1),
                ("command-fits", 1),
                ("composite-found", 2),
                ("decorator-found", 1),
                ("facade-fits", 1),
                ("factory-method-found", 1),
                ("interpreter-found", 1),
                ("iterator-found", 1),
                ("mediator-fits", 1),
                ("memento-found", 1),
                ("observer-found", 1),
                ("observer-without-removal", 1),
                ("prototype-found", 1),
                ("proxy-found", 1),
                ("state-fits", 1),
                ("strategy-fits", 2),
                ("template-method-found", 1),
                ("visitor-found", 1),
            ],
        ),
        (
            "go",
            &[
                ("builder-found", 1),
                ("builder-without-build", 1),
                ("chain-of-responsibility-fits", 1),
                ("command-fits", 1),
                ("composite-found", 1),
                ("facade-fits", 1),
                ("mediator-fits", 1),
                ("memento-found", 1),
                ("observer-found", 1),
                ("observer-without-removal", 1),
                ("prototype-found", 1),
                ("state-fits", 1),
                ("strategy-fits", 1),
                ("visitor-found", 1),
            ],
        ),
        (
            "java",
            &[
                ("abstract-factory-found", 1),
                ("adapter-found", 1),
                ("bridge-fits", 1),
                ("builder-found", 1),
                ("builder-without-build", 1),
                ("chain-of-responsibility-fits", 1),
                ("command-fits", 1),
                ("composite-found", 1),
                ("decorator-found", 1),
                ("decorator-not-substitutable", 2),
                ("facade-fits", 1),
                ("factory-method-found", 1),
                ("flyweight-found", 1),
                ("interpreter-found", 1),
                ("iterator-found", 1),
                ("mediator-fits", 1),
                ("memento-found", 1),
                ("observer-found", 1),
                ("observer-without-removal", 1),
                ("prototype-found", 1),
                ("proxy-found", 1),
                ("singleton-escapable", 1),
                ("singleton-found", 1),
                ("state-fits", 1),
                ("strategy-fits", 2),
                ("template-method-found", 1),
                ("visitor-found", 1),
            ],
        ),
        (
            "cpp",
            &[
                ("abstract-factory-found", 1),
                ("adapter-found", 1),
                ("bridge-fits", 1),
                ("builder-found", 1),
                ("builder-without-build", 1),
                ("chain-of-responsibility-fits", 1),
                ("command-fits", 1),
                ("composite-found", 1),
                ("decorator-found", 1),
                ("decorator-not-substitutable", 2),
                ("facade-fits", 1),
                ("factory-method-found", 1),
                ("flyweight-found", 1),
                ("interpreter-found", 1),
                ("iterator-found", 1),
                ("mediator-fits", 1),
                ("memento-found", 1),
                ("observer-found", 1),
                ("observer-without-removal", 1),
                ("prototype-found", 1),
                ("proxy-found", 1),
                ("singleton-escapable", 1),
                ("singleton-found", 1),
                ("state-fits", 1),
                ("strategy-fits", 2),
                ("template-method-found", 1),
                ("visitor-found", 1),
            ],
        ),
    ];

    let mut wrong = Vec::new();
    for (language, table) in expected {
        let counts = counts_by_advisory(&advise_language(language));
        for (advisory, count) in *table {
            let actual = counts.get(*advisory).copied().unwrap_or(0);
            if actual != *count {
                wrong.push(format!(
                    "{language} {advisory}: expected {count}, got {actual}"
                ));
            }
        }
        for advisory in counts.keys() {
            if !table.iter().any(|(known, _)| known == advisory) {
                wrong.push(format!(
                    "{language} {advisory}: fired but is not in the table"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Same promise the rules make. An advisory that no fixture can trigger in a language
/// is indistinguishable from one that is broken there, which is exactly what
/// `not_applicable` exists to tell apart.
#[test]
fn every_applicable_advisory_fires_on_every_language() {
    use rqx_core::Language;
    use std::collections::BTreeSet;

    let mut missing = Vec::new();
    for language in Language::ALL {
        if !std::path::Path::new(&language_dir(language.slug())).is_dir() {
            continue;
        }
        let fired: BTreeSet<String> = counts_by_advisory(&advise_language(language.slug()))
            .into_keys()
            .collect();
        let expected: BTreeSet<String> = rqx_core::AdviceId::for_language(language)
            .map(|advisory| advisory.slug().to_owned())
            .collect();
        for advisory in expected.difference(&fired) {
            missing.push(format!("{}: {advisory}", language.slug()));
        }
    }
    assert!(
        missing.is_empty(),
        "no fixture triggers these: {missing:#?}"
    );
}

#[test]
fn an_advisory_declared_inapplicable_never_fires() {
    use rqx_core::Language;

    let mut wrong = Vec::new();
    for language in Language::ALL {
        if !std::path::Path::new(&language_dir(language.slug())).is_dir() {
            continue;
        }
        let fired = counts_by_advisory(&advise_language(language.slug()));
        for advisory in rqx_core::AdviceId::ALL {
            if let Some(reason) = advisory.exemption(language)
                && fired.contains_key(advisory.slug())
            {
                wrong.push(format!(
                    "{} fired {} despite: {reason}",
                    language.slug(),
                    advisory.slug()
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The load-bearing guarantee of the whole advice catalogue: a pattern claim is not a
/// defect, so turning every advisory on must leave the health score, the counts and
/// the gate verdict exactly where they were. If this ever fails, advice has leaked
/// into the score and the score has stopped meaning what it says.
#[test]
fn advice_cannot_move_the_health_score_or_the_gate() {
    let dir = std::env::temp_dir().join(format!("rqx-advice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let silent = dir.join("silent.toml");
    // Same rules, only the patterns differ, or the comparison would be measuring the
    // rule catalogue instead of the advice one.
    let all = std::fs::read_to_string(config_path()).expect("read the fixture config");
    let rules = all.split("[patterns]").next().unwrap_or_default();
    std::fs::write(
        &silent,
        format!(
            "{rules}\n[patterns]\ndisabled = [\"creational\", \"structural\", \"behavioural\"]\n"
        ),
    )
    .expect("write config");

    assert!(
        !counts_by_advisory(&advise_language("typescript")).is_empty(),
        "the comparison is vacuous unless some advisory fires"
    );
    let with = scan_language("typescript", &["--config", &config_path()]);
    let without = scan_language(
        "typescript",
        &["--config", silent.to_str().expect("utf-8 path")],
    );
    assert_eq!(summary_of(&with), summary_of(&without));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without `elapsed_ms`, which is wall-clock and differs between two runs of the same
/// scan.
/// Advice is collected on worker threads and joined after the walk, which is exactly
/// where an order can leak into the output.
#[test]
fn advice_does_not_depend_on_thread_count() {
    let single = advise_language_with("typescript", &["--threads", "1"]);
    let many = advise_language_with("typescript", &["--threads", "8"]);
    assert_eq!(single, many);
    assert!(!single.is_empty(), "the comparison would be vacuous");
}

fn advise_language_with(language: &str, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["patterns", &language_dir(language), "--json"])
        .args(["--config", &config_path()])
        .args(extra)
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success(), "rqx exited with {}", output.status);
    String::from_utf8(output.stdout).expect("output was not utf-8")
}

/// The other half of the score guarantee: the one key that lets advice reach an exit
/// code moves the verdict and nothing else, and only ever on the stance that names
/// something broken.
#[test]
fn only_a_half_applied_pattern_can_fail_the_gate() {
    let dir = std::env::temp_dir().join(format!("rqx-gate-advice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let strict = dir.join("strict.toml");
    let all = std::fs::read_to_string(config_path()).expect("read the fixture config");
    std::fs::write(
        &strict,
        format!("{all}\n[gate]\nfail_on_incomplete = true\n"),
    )
    .expect("write config");
    let strict = strict.to_str().expect("utf-8 path").to_owned();

    let relaxed = scan_language("typescript", &["--config", &config_path()]);
    // No gate flag, so this run only proves the key changed nothing about the counts;
    // the exit code is checked separately below.
    let gated = scan_language("typescript", &["--config", &strict]);
    assert_eq!(
        summary_of(&relaxed)
            .split("\"grades\"")
            .next()
            .expect("a summary"),
        summary_of(&gated)
            .split("\"grades\"")
            .next()
            .expect("a summary"),
        "the gate key must not move a count or the score"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", &language_dir("typescript")])
        .args(["--config", &strict, "--fail-on-incomplete", "--quiet"])
        .output()
        .expect("failed to run rqx");
    assert_eq!(
        output.status.code(),
        Some(1),
        "a half-applied pattern fails"
    );

    // Nothing is half applied in the clean fixture, so the same key must let it pass.
    let clean = std::env::temp_dir().join(format!("rqx-gate-clean-{}", std::process::id()));
    std::fs::create_dir_all(&clean).expect("temp dir");
    std::fs::copy(
        format!("{}/clean.ts", language_dir("typescript")),
        clean.join("clean.ts"),
    )
    .expect("copy the clean fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", clean.to_str().expect("utf-8 path")])
        .args(["--config", &strict, "--fail-on-incomplete", "--quiet"])
        .output()
        .expect("failed to run rqx");
    assert_eq!(output.status.code(), Some(0), "nothing half applied passes");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&clean);
}

fn summary_of(report: &str) -> String {
    let line = report
        .lines()
        .find(|line| line.contains("\"kind\":\"summary\""))
        .expect("no summary line");
    let (head, rest) = line.split_once("\"elapsed_ms\":").expect("no elapsed_ms");
    let tail = rest.split_once(',').expect("truncated summary").1;
    format!("{head}{tail}")
}

/// The catalogue is a promise: every rule in it claims a smell, a metric and a fix.
/// This is what stops that promise being aspirational — a rule nothing can trigger is
/// a documentation entry pretending to be a check. It is asked once per language,
/// because "Rqx covers Go" means the same 60 rules, not a subset nobody wrote down.
#[test]
fn every_applicable_rule_fires_on_every_language() {
    use rqx_core::Language;
    use std::collections::BTreeSet;

    // These read something one language directory cannot carry: two read git history,
    // two are silent until a project declares `[architecture]`, and one is about the
    // same logic appearing in two languages, which a single-language corpus cannot
    // contain by definition. They are covered by
    // `history_rules_fire_on_a_repository_with_a_shared_decision` and by
    // `tests/architecture.rs`.
    let history_backed: BTreeSet<String> = [
        "shotgun-surgery",
        "divergent-change",
        "layer-violation",
        "forbidden-import",
        "cross-language-duplicate",
    ]
    .map(str::to_owned)
    .into();

    let mut missing = Vec::new();
    for language in Language::ALL {
        if !std::path::Path::new(&language_dir(language.slug())).is_dir() {
            continue;
        }
        let report = scan_language(language.slug(), &["--config", &config_path()]);
        let fired: BTreeSet<String> = counts_by_rule(&report).into_keys().collect();
        let expected: BTreeSet<String> = rqx_core::RuleId::for_language(language)
            .map(|rule| rule.slug().to_owned())
            .collect();
        for rule in expected.difference(&fired) {
            if !history_backed.contains(rule) {
                missing.push(format!("{}: {rule}", language.slug()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "no fixture triggers these: {missing:#?}"
    );
}

/// The other half of the promise: a rule declared inapplicable must actually stay
/// silent, or the exemption is a false statement about what Rqx does.
#[test]
fn a_rule_declared_inapplicable_never_fires() {
    use rqx_core::Language;

    let mut wrong = Vec::new();
    for language in Language::ALL {
        if !std::path::Path::new(&language_dir(language.slug())).is_dir() {
            continue;
        }
        let fired = counts_by_rule(&scan_language(
            language.slug(),
            &["--config", &config_path()],
        ));
        for rule in rqx_core::RuleId::ALL {
            if let Some(reason) = rule.exemption(language)
                && fired.contains_key(rule.slug())
            {
                wrong.push(format!(
                    "{} fired {} despite: {reason}",
                    language.slug(),
                    rule.slug()
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_category_is_represented_in_the_fixtures() {
    use rqx_core::rule::Category;

    let config = config_path();
    let report = scan(&["--config", &config]);
    let fired = counts_by_rule(&report);

    for category in Category::ALL {
        if category == Category::Change {
            continue; // history-backed; see the dedicated test
        }
        assert!(
            category.rules().any(|rule| fired.contains_key(rule.slug())),
            "{} has no fixture coverage",
            category.title()
        );
    }
}

/// Builds a repository whose history contains one decision split across two files,
/// which is exactly what Shotgun Surgery names. Nothing about this is visible in a
/// single snapshot of the source, so it can only be tested against real commits.
#[test]
fn history_rules_fire_on_a_repository_with_a_shared_decision() {
    use std::fs;
    use std::process::Command;

    let root = std::env::temp_dir().join(format!("rqx-history-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();

    let git = |args: &[&str]| {
        let status = Command::new("git").arg("-C").arg(&root).args(args).output();
        assert!(
            status.is_ok_and(|out| out.status.success()),
            "git {args:?} failed"
        );
    };
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "test"]);

    fs::write(
        root.join("package.json"),
        r#"{"name":"h","main":"src/index.ts"}"#,
    )
    .unwrap();
    fs::write(
        root.join("src/index.ts"),
        "export { route } from \"./router\";\n",
    )
    .unwrap();

    // Every commit edits both files: the router learns a case and the table learns
    // its label. That is one decision living in two places.
    for step in 0..8 {
        fs::write(
            root.join("src/router.ts"),
            format!("export function route(kind: string) {{ return {step}; }}\n"),
        )
        .unwrap();
        fs::write(
            root.join("src/labels.ts"),
            format!(
                "export const labels = [{}];\n",
                (0..=step)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        )
        .unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "extend routing"]);
    }

    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan"])
        .arg(&root)
        .args(["--format", "json", "--max-findings", "0"])
        .output()
        .expect("failed to run rqx");
    let report = String::from_utf8(output.stdout).unwrap();

    let shotgun = findings_for(&report, "shotgun-surgery");
    assert!(
        !shotgun.is_empty(),
        "two files always edited together should be reported\n{report}"
    );
    assert!(
        shotgun
            .iter()
            .any(|line| line.contains("labels.ts") || line.contains("router.ts")),
        "the finding should name the pair: {shotgun:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .output()
        .expect("failed to run rqx")
}

/// The three codes are the whole CI contract: a pipeline that cannot tell a failed
/// gate from a crashed tool ends up ignoring both.
#[test]
fn the_exit_code_separates_a_failed_gate_from_a_broken_run() {
    let dir = language_dir("typescript");
    assert_eq!(
        run(&["scan", &dir]).status.code(),
        Some(0),
        "a bare scan reports, it does not gate"
    );
    assert_eq!(
        run(&["scan", &dir, "--fail-on", "error"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run(&[
            "scan",
            &dir,
            "--fail-on",
            "error",
            "--allow-findings",
            "999"
        ])
        .status
        .code(),
        Some(0)
    );
    assert_eq!(run(&["scan", "/rqx-no-such-path"]).status.code(), Some(2));
}

/// `rqx scan . | head` closes the reader mid-report. That used to surface as exit 2,
/// which reads as "rqx could not run" and fails a pipeline that worked.
/// The rule reference is generated, so a rule added without regenerating it leaves
/// the docs describing a tool that no longer exists. Run `rqx docs` to fix a failure
/// here.
#[test]
fn the_checked_in_reference_is_current() {
    let root = format!("{}/../..", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args([
            "docs",
            "--out",
            &format!("{root}/target/docs-check"),
            "--fixtures",
            &fixtures_dir(),
        ])
        .output()
        .expect("failed to run rqx docs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let generated_root = format!("{root}/target/docs-check/");
    let mut stale = Vec::new();
    for entry in walk(std::path::Path::new(&generated_root)) {
        let relative = entry
            .strip_prefix(generated_root.as_str())
            .expect("a generated path")
            .to_owned();
        let generated = std::fs::read_to_string(&entry).expect("a generated page");
        let checked_in =
            std::fs::read_to_string(format!("{root}/docs/{relative}")).unwrap_or_default();
        if generated != checked_in {
            stale.push(relative);
        }
    }
    assert!(
        stale.is_empty(),
        "run `rqx docs` \u{2014} these are out of date: {stale:?}"
    );
}

fn walk(root: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path.to_string_lossy().into_owned());
        }
    }
    found.sort();
    found
}

#[test]
fn a_reader_that_walks_away_is_not_a_failed_run() {
    use std::process::{Command, Stdio};

    let mut scan = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", &fixtures_dir()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rqx starts");
    let reader = scan.stdout.take().expect("rqx writes to stdout");
    Command::new("head")
        .arg("-3")
        .stdin(Stdio::from(reader))
        .stdout(Stdio::null())
        .status()
        .expect("head runs");
    let output = scan.wait_with_output().expect("rqx finishes");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_report_that_cannot_be_written_is_a_broken_run() {
    let output = run(&[
        "scan",
        &language_dir("typescript"),
        "--reporter",
        "json=/rqx-no-such-directory/report.json",
    ]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn one_scan_writes_every_report_it_was_asked_for() {
    let dir = std::env::temp_dir().join(format!("rqx-sinks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let junit = dir.join("rqx.xml");
    let sarif = dir.join("rqx.sarif");

    let output = run(&[
        "scan",
        &language_dir("typescript"),
        "--reporter",
        &format!("junit={}", junit.display()),
        "--reporter",
        &format!("sarif={}", sarif.display()),
    ]);
    assert!(output.status.success());

    let junit = std::fs::read_to_string(&junit).unwrap();
    let sarif = std::fs::read_to_string(&sarif).unwrap();
    assert!(junit.starts_with("<?xml"), "{junit:.80}");
    assert!(sarif.contains("\"$schema\""), "{sarif:.80}");
    assert!(
        !junit.contains('\u{1b}') && !sarif.contains('\u{1b}'),
        "a file report must never carry escape sequences"
    );
    // The console report still reaches stdout, so nothing is silently swallowed.
    assert!(String::from_utf8(output.stdout).unwrap().contains("Health"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_document_cannot_share_stdout_but_two_logs_can() {
    let output = run(&[
        "scan",
        &language_dir("typescript"),
        "--reporter",
        "json",
        "--reporter",
        "sarif",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("json") && error.contains("sarif"), "{error}");

    let output = run(&[
        "scan",
        &language_dir("typescript"),
        "--reporter",
        "terminal",
        "--reporter",
        "github",
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("Health") && text.contains("::error file="),
        "{text:.400}"
    );
}

/// A display filter that moved the score would make a CI result depend on how it
/// was printed.
#[test]
fn a_display_filter_never_moves_the_health_score() {
    let full = summary_field(&scan(&[]), "health_score");
    for narrowing in [
        ["--severity", "error"],
        ["--category", "size"],
        ["--rule", "long-function"],
    ] {
        let narrowed = summary_field(&scan(&narrowing), "health_score");
        assert_eq!(full, narrowed, "{narrowing:?} changed the score");
    }
}

#[test]
fn a_filter_narrows_the_findings_it_reports() {
    let all = counts_by_rule(&scan(&[]));
    let one = counts_by_rule(&scan(&["--rule", "long-function"]));
    assert_eq!(one.keys().collect::<Vec<_>>(), vec!["long-function"]);
    assert_eq!(one["long-function"], all["long-function"]);
}

#[test]
fn an_unknown_rule_is_a_tool_error_that_says_what_to_do_next() {
    let output = run(&[
        "scan",
        &language_dir("typescript"),
        "--rule",
        "no-such-rule",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("no-such-rule"), "{error}");
    assert!(error.contains("rqx rules"), "{error}");

    let typo = run(&[
        "scan",
        &language_dir("typescript"),
        "--rule",
        "unused-exprot",
    ]);
    let error = String::from_utf8(typo.stderr).unwrap();
    assert!(error.contains("Did you mean `unused-export`?"), "{error}");
}

/// Every machine report has to parse, and the paths in it have to resolve from the
/// directory CI actually runs in rather than from the directory that was scanned.
#[test]
fn machine_reports_name_paths_from_the_working_directory() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    for (reporter, needle) in [
        ("github", "file=fixtures/"),
        ("gitlab", "\"path\": \"fixtures/"),
        ("sarif", "\"uri\": \"fixtures/"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .current_dir(root)
            .args(["scan", "fixtures", "--reporter", reporter])
            .output()
            .expect("failed to run rqx");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(
            text.contains(needle),
            "{reporter} lost the scanned directory:\n{text:.400}"
        );
    }
}

/// The gate that decides how much of the engine runs used to test three categories,
/// while the collectors behind it also supply every Safety, Tests and Naming rule.
/// Turning those three families off silently stopped rules that had nothing to do
/// with them.
#[test]
fn disabling_three_families_does_not_silence_the_others() {
    let dir = std::env::temp_dir().join(format!("rqx-gate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("RQX.toml");
    std::fs::write(
        &config,
        "[rules]\ndisabled = [\"waste\", \"coupling\", \"abstraction\"]\n",
    )
    .unwrap();

    let report = scan(&["--config", config.to_str().unwrap()]);
    let fired = counts_by_rule(&report);
    for rule in [
        "focused-test",
        "skipped-test",
        "suppressed-diagnostic",
        "output-parameter",
    ] {
        assert!(
            fired.contains_key(rule),
            "{rule} went silent; fired: {fired:?}"
        );
    }
    for rule in ["unused-local", "message-chain", "temporary-field"] {
        assert!(
            !fired.contains_key(rule),
            "{rule} belongs to a disabled family"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A finding whose span is the whole file used to print no code at all, and a span
/// crossing lines printed only its first.
#[test]
fn every_finding_shows_the_code_it_is_about() {
    let whole_file = run(&["scan", &language_dir("typescript"), "--rule", "unused-file"]);
    let text = String::from_utf8(whole_file.stdout).unwrap();
    assert!(
        text.contains("1 \u{2502}"),
        "a whole-file finding shows no code:\n{text}"
    );

    let spanning = run(&[
        "scan",
        &language_dir("typescript"),
        "--rule",
        "long-function",
        "--context",
        "0",
    ]);
    let text = String::from_utf8(spanning.stdout).unwrap();
    assert!(
        text.contains("^^^"),
        "the offending columns are not marked:\n{text}"
    );
}

/// A grade is per file, so one file rotting is visible where a project-wide average
/// hides it, and the gate can fail on that alone.
#[test]
fn the_grade_ranks_files_and_can_gate_on_one_of_them() {
    let report = run(&["scan", &language_dir("typescript")]);
    let text = String::from_utf8(report.stdout).unwrap();
    assert!(text.contains("Grades"), "no grade distribution:\n{text}");
    assert!(text.contains("critical"), "no file graded:\n{text}");
    assert!(
        !text
            .split("Start here")
            .nth(1)
            .unwrap_or_default()
            .contains("clean.ts"),
        "a file with no findings should not be recommended:\n{text}"
    );

    // With the severity gate off, the grade is the only thing that can fail the run.
    let dir = language_dir("typescript");
    let graded = ["scan", &dir, "--fail-on", "never", "--min-grade", "poor"];
    assert_eq!(run(&graded).status.code(), Some(1));
    let ungraded = ["scan", &dir, "--fail-on", "never", "--min-grade", "never"];
    assert_eq!(run(&ungraded).status.code(), Some(0));

    let failure = String::from_utf8(run(&graded).stdout).unwrap();
    assert!(failure.contains("against a floor of poor"), "{failure}");
}
