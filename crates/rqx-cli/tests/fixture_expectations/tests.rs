//! The harness has to fail for the right reasons, so its comparison is tested on small
//! inputs here, apart from the fixtures it normally reads.

use super::compare::{blessed, compare, fired};
use super::sidecar::{parse, render};
use super::sources::Sources;
use super::{Difference, Finding, report};

const LABEL: &str = "fixtures/go";
const SOURCE: &str = "\
func first() {
    total := 0
    return total
}

func second() {
    total := 0
    return total
}
";

fn sources() -> Sources {
    Sources::from_texts([("a.go", SOURCE.to_owned())])
}

fn finding(line: usize, rule: &str) -> Finding {
    Finding {
        file: "a.go".to_owned(),
        line,
        rule: rule.to_owned(),
    }
}

fn differences(sidecar: &str, findings: &[Finding]) -> Vec<String> {
    differences_in(sidecar, findings, &sources())
}

fn differences_in(sidecar: &str, findings: &[Finding], sources: &Sources) -> Vec<String> {
    let (expectations, problems) = parse(sidecar);
    assert_eq!(problems, [], "the test's sidecar is well formed");
    compare(&expectations, &fired(findings), sources, LABEL)
        .iter()
        .map(Difference::to_string)
        .collect()
}

fn problems(sidecar: &str) -> Vec<String> {
    parse(sidecar).1.into_iter().map(|p| p.message).collect()
}

#[test]
fn a_finding_on_its_expected_line_is_no_difference() {
    let sidecar = "a.go rule=unused-local | total := 0\n";
    assert_eq!(
        differences(sidecar, &[finding(2, "unused-local")]),
        [] as [&str; 0]
    );
}

#[test]
fn a_finding_nothing_expects_is_reported_at_its_line() {
    assert_eq!(
        differences("", &[finding(3, "output-parameter")]),
        ["fixtures/go/a.go:3  unexpected output-parameter"]
    );
}

#[test]
fn an_expected_finding_that_stops_firing_is_reported_at_its_line() {
    assert_eq!(
        differences("a.go rule=discarded-error | return total\n", &[]),
        ["fixtures/go/a.go:3  missing discarded-error"]
    );
}

#[test]
fn a_repeat_count_is_exact_in_both_directions() {
    let twice = "a.go rule=magic-number x2 | total := 0\n";
    assert_eq!(
        differences(twice, &[finding(2, "magic-number")]),
        ["fixtures/go/a.go:2  missing magic-number (fires 1 times, expected 2)"]
    );
    let once = "a.go rule=magic-number | total := 0\n";
    assert_eq!(
        differences(
            once,
            &[finding(2, "magic-number"), finding(2, "magic-number")]
        ),
        ["fixtures/go/a.go:2  unexpected magic-number (fires 2 times, expected 1)"]
    );
}

#[test]
fn an_occurrence_selects_among_identical_lines() {
    let second = "a.go rule=unused-local #2 | total := 0\n";
    assert_eq!(
        differences(second, &[finding(7, "unused-local")]),
        [] as [&str; 0]
    );
    assert_eq!(
        differences(second, &[finding(2, "unused-local")]),
        [
            "fixtures/go/a.go:2  unexpected unused-local",
            "fixtures/go/a.go:7  missing unused-local",
        ]
    );
}

#[test]
fn an_entry_follows_its_line_when_the_file_moves() {
    let moved = Sources::from_texts([("a.go", format!("// header\n\n{SOURCE}"))]);
    let sidecar = "a.go rule=unused-local | total := 0\n";
    assert_eq!(
        differences_in(sidecar, &[finding(4, "unused-local")], &moved),
        [] as [&str; 0]
    );
}

#[test]
fn none_fails_only_when_the_rule_fires() {
    let sidecar = "a.go none=output-parameter | total := 0\n";
    assert_eq!(differences(sidecar, &[]), [] as [&str; 0]);
    assert_eq!(
        differences(sidecar, &[finding(2, "output-parameter")]),
        ["fixtures/go/a.go:2  unexpected output-parameter (marked none=)"]
    );
}

#[test]
fn may_accepts_the_rule_firing_or_not() {
    let sidecar = "a.go may=scan-in-loop | total := 0\n";
    assert_eq!(differences(sidecar, &[]), [] as [&str; 0]);
    assert_eq!(
        differences(sidecar, &[finding(2, "scan-in-loop")]),
        [] as [&str; 0]
    );
}

#[test]
fn a_known_false_positive_passes_while_it_fires_and_fails_once_it_stops() {
    let sidecar = "a.go known-fp=discarded-error returns no error | total := 0\n";
    assert_eq!(
        differences(sidecar, &[finding(2, "discarded-error")]),
        [] as [&str; 0]
    );
    assert_eq!(
        differences(sidecar, &[]),
        [
            "fixtures/go/a.go:2  known false positive no longer fires: discarded-error \
             (returns no error); change known-fp= to none="
        ]
    );
}

#[test]
fn an_entry_whose_line_is_gone_is_reported() {
    assert_eq!(
        differences("a.go none=unused-local | total := 1\n", &[]),
        ["fixtures/go/a.go  no line matches the entry for unused-local: total := 1"]
    );
}

#[test]
fn two_entries_for_one_rule_on_one_line_are_reported() {
    let sidecar = "a.go rule=unused-local | total := 0\na.go none=unused-local | total := 0\n";
    assert_eq!(
        differences(sidecar, &[finding(2, "unused-local")]),
        ["fixtures/go/a.go:2  unused-local is listed twice for this line"]
    );
}

#[test]
fn the_format_rejects_what_it_cannot_mean() {
    for (line, expected) in [
        ("a.go rule=unused-local", "no ` | ` before"),
        ("a.go | total", "no `kind=rule`"),
        ("a.go rule=no-such-rule | total", "no rule is called"),
        ("a.go known-fp=unused-local | total", "needs a reason"),
        ("a.go rule=unused-local why | total", "takes no reason"),
        ("a.go none=unused-local x2 | total", "takes no repeat count"),
        ("a.go rule=unused-local x0 | total", "not a positive number"),
        ("a.go must=unused-local | total", "unknown kind"),
    ] {
        let found = problems(&format!("{line}\n"));
        assert!(
            found.len() == 1 && found[0].contains(expected),
            "{line}: {found:?}"
        );
    }
}

#[test]
fn entries_survive_being_written_and_read_back() {
    let sidecar = "\
a.go known-fp=discarded-error x2 #2 returns no error | a | b
a.go rule=unused-local |
a.go may=scan-in-loop | total := 0
";
    let (expectations, found) = parse(sidecar);
    assert_eq!(found, []);
    let (again, found) = parse(&render(LABEL, &expectations));
    assert_eq!(found, []);
    assert_eq!(again, expectations);
    assert_eq!(expectations[0].text, "a | b");
    assert_eq!(expectations[1].text, "");
}

#[test]
fn blessing_writes_the_findings_in_source_order() {
    let findings = [
        finding(7, "unused-local"),
        finding(2, "magic-number"),
        finding(2, "magic-number"),
    ];
    let expectations = blessed(&[], &fired(&findings), &sources());
    let body: Vec<String> = render(LABEL, &expectations)
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(str::to_owned)
        .collect();
    assert_eq!(
        body,
        [
            "a.go rule=magic-number x2 | total := 0",
            "a.go rule=unused-local #2 | total := 0",
        ]
    );
}

#[test]
fn blessing_keeps_decisions_and_replaces_only_rule_entries() {
    let sidecar = "\
a.go rule=unused-local | return total
a.go none=output-parameter | total := 0
a.go may=scan-in-loop | total := 0
a.go known-fp=discarded-error returns no error | func first() {
a.go none=magic-number | a line that is gone
";
    let (existing, _) = parse(sidecar);
    let findings = [finding(2, "output-parameter"), finding(7, "unused-local")];
    let expectations = blessed(&existing, &fired(&findings), &sources());
    let body = render(LABEL, &expectations);
    assert!(body.contains("a.go none=output-parameter | total := 0"));
    assert!(body.contains("a.go may=scan-in-loop | total := 0"));
    assert!(body.contains("a.go known-fp=discarded-error returns no error | func first() {"));
    assert!(body.contains("a.go none=magic-number | a line that is gone"));
    assert!(body.contains("a.go rule=unused-local #2 | total := 0"));
    assert!(!body.contains("| return total"));
    assert!(
        !body.contains("rule=output-parameter"),
        "blessing must not overrule a none= entry"
    );
    assert_eq!(
        compare(&expectations, &fired(&findings), &sources(), LABEL)
            .iter()
            .map(Difference::to_string)
            .collect::<Vec<_>>(),
        [
            "fixtures/go/a.go  no line matches the entry for magic-number: a line that is gone",
            "fixtures/go/a.go:1  known false positive no longer fires: discarded-error \
             (returns no error); change known-fp= to none=",
            "fixtures/go/a.go:2  unexpected output-parameter (marked none=)",
        ]
    );
}

#[test]
fn the_report_gives_one_line_per_difference_and_the_command() {
    let text = report(&[Difference::at(
        "fixtures/go/a.go".to_owned(),
        12,
        "unexpected output-parameter".to_owned(),
    )]);
    assert_eq!(
        text,
        "fixtures/go/a.go:12  unexpected output-parameter\n\n\
         1 difference(s). If the change is intended, rewrite the expectations and review \
         their diff:\n  RQX_BLESS=1 cargo test -p rqx --test fixtures \
         every_fixture_finding_is_expected"
    );
}
