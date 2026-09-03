use std::collections::BTreeMap;
use std::process::Command;

fn fixtures_dir() -> String {
    format!("{}/../../fixtures", env!("CARGO_MANIFEST_DIR"))
}

fn scan(extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", &fixtures_dir(), "--format", "json", "--max-findings", "0"])
        .args(extra)
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success(), "rqx exited with {}", output.status);
    String::from_utf8(output.stdout).expect("output was not utf-8")
}

fn counts_by_rule(report: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for line in report.lines().filter(|line| !line.contains("\"kind\":\"summary\"")) {
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
    let summary = report.lines().find(|line| line.contains("\"kind\":\"summary\"")).unwrap();
    summary
        .split(&format!("\"{field}\":"))
        .nth(1)
        .and_then(|rest| rest.split(&[',', '}'][..]).next())
        .unwrap()
        .to_owned()
}

#[test]
fn every_rule_fires_on_its_fixture() {
    let counts = counts_by_rule(&scan(&[]));
    let expected = [
        ("assertionless-test", 1),
        ("base-depends-on-derived", 1),
        ("boundary-arithmetic", 1),
        ("circular-dependency", 1),
        ("cognitive-complexity", 1),
        ("commented-out-code", 1),
        ("complex-condition", 1),
        ("data-class", 1),
        ("data-clump", 1),
        ("deep-nesting", 2),
        ("distant-declaration", 2),
        ("duplicate-code", 1),
        ("empty-catch", 1),
        ("feature-envy", 2),
        ("flag-argument", 4),
        ("focused-test", 1),
        ("imprecise-type", 3),
        ("inappropriate-static", 1),
        ("journal-comment", 1),
        ("large-class", 1),
        ("lazy-class", 1),
        ("long-function", 2),
        ("long-parameter-list", 2),
        ("magic-number", 3),
        ("message-chain", 2),
        ("middle-man", 1),
        ("mixed-conventions", 1),
        ("mixed-language-file", 1),
        ("output-parameter", 1),
        ("query-with-side-effect", 1),
        ("redundant-comment", 1),
        ("refused-bequest", 1),
        ("skipped-test", 1),
        ("slow-test", 1),
        ("speculative-generality", 1),
        ("suppressed-diagnostic", 1),
        ("temporary-field", 1),
        ("type-code-switch", 1),
        ("unreachable-code", 1),
        ("unused-export", 2),
        ("unused-file", 1),
        ("unused-local", 2),
        ("unused-parameter", 11),
        ("unused-private-member", 2),
        ("vague-function-name", 2),
    ];

    for (rule, count) in expected {
        assert_eq!(
            counts.get(rule).copied().unwrap_or(0),
            count,
            "{rule} changed; full counts: {counts:?}"
        );
    }
    assert!(
        !counts.contains_key("meaningless-name"),
        "meaningless-name ships disabled and must stay off by default"
    );
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
    assert_eq!(dead.len(), 2, "expected neverImported and AlsoNeverImported");
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
    assert!(cycles[0].contains("\"modules\":2"));
    assert!(cycles[0].contains("cycle-a.ts") && cycles[0].contains("cycle-b.ts"));
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
    let trailing: Vec<&String> =
        params.iter().filter(|line| line.contains("\"trailing\":true")).collect();
    let middle: Vec<&String> =
        params.iter().filter(|line| line.contains("\"trailing\":false")).collect();
    assert!(!trailing.is_empty() && !middle.is_empty());
    assert!(
        trailing.iter().all(|line| line.contains("\"confidence\":\"verified\"")
            || line.contains("\"confidence\":\"heuristic\"")),
        "trailing params carry a confidence"
    );
    assert!(
        middle.iter().all(|line| line.contains("\"confidence\":\"heuristic\"")),
        "a parameter that cannot be safely deleted must not claim to be verified"
    );
}

#[test]
fn the_low_confidence_rule_reports_only_when_enabled() {
    let config = format!("{}/all-rules.toml", fixtures_dir());
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
    assert_eq!(findings(scan(&["--threads", "1"])), findings(scan(&["--threads", "16"])));
}

#[test]
fn the_display_cap_never_moves_the_health_score() {
    let uncapped = scan(&[]);
    let capped = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", &fixtures_dir(), "--format", "json", "--max-findings", "1"])
        .output()
        .expect("failed to run rqx");
    let capped = String::from_utf8(capped.stdout).unwrap();

    assert_eq!(summary_field(&uncapped, "health_score"), summary_field(&capped, "health_score"));
    assert_eq!(summary_field(&uncapped, "counts"), summary_field(&capped, "counts"));
    assert_eq!(capped.lines().filter(|l| !l.contains("summary")).count(), 1);
}

#[test]
fn a_finding_always_carries_its_taxonomy_and_a_named_fix() {
    for line in scan(&[]).lines().filter(|line| !line.contains("\"kind\":\"summary\"")) {
        for field in ["\"why\":\"", "\"clean_code_smell\":\"", "\"metric\":\"", "\"fix_hint\":\""] {
            let value = line.split(field).nth(1).and_then(|rest| rest.split('"').next());
            assert!(
                value.is_some_and(|text| !text.is_empty()),
                "a finding shipped without {field}: {line}"
            );
        }
        assert!(line.contains("\"fixes\":[\""), "a finding shipped with no fix technique: {line}");
    }
}

/// The catalogue is a promise: every rule in it claims a smell, a metric and a fix.
/// This is what stops that promise being aspirational — a rule nothing can trigger is
/// a documentation entry pretending to be a check.
#[test]
fn every_rule_in_the_catalogue_fires_on_a_fixture() {
    use std::collections::BTreeSet;

    let config = format!("{}/all-rules.toml", fixtures_dir());
    let report = scan(&["--config", &config]);
    let fired: BTreeSet<String> = counts_by_rule(&report).into_keys().collect();

    // These two read git history, which the fixture directory has none of; they are
    // covered by `history_rules_fire_on_a_repository_with_a_shared_decision`.
    let history_backed: BTreeSet<String> =
        ["shotgun-surgery", "divergent-change"].map(str::to_owned).into();

    let catalogue: BTreeSet<String> = rqx_core::rule::RuleId::ALL
        .iter()
        .map(|rule| rule.slug().to_owned())
        .collect();

    let silent: Vec<&String> =
        catalogue.difference(&fired).filter(|rule| !history_backed.contains(*rule)).collect();
    assert!(silent.is_empty(), "no fixture triggers: {silent:?}");
}

#[test]
fn every_category_is_represented_in_the_fixtures() {
    use rqx_core::rule::Category;

    let config = format!("{}/all-rules.toml", fixtures_dir());
    let report = scan(&["--config", &config]);
    let fired = counts_by_rule(&report);

    for category in Category::ALL {
        if category == Category::ChangePreventers {
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
        assert!(status.is_ok_and(|out| out.status.success()), "git {args:?} failed");
    };
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "test"]);

    fs::write(root.join("package.json"), r#"{"name":"h","main":"src/index.ts"}"#).unwrap();
    fs::write(root.join("src/index.ts"), "export { route } from \"./router\";\n").unwrap();

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
            format!("export const labels = [{}];\n", (0..=step).map(|n| n.to_string()).collect::<Vec<_>>().join(",")),
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
    assert!(!shotgun.is_empty(), "two files always edited together should be reported\n{report}");
    assert!(
        shotgun.iter().any(|line| line.contains("labels.ts") || line.contains("router.ts")),
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
    let dir = fixtures_dir();
    assert_eq!(run(&["scan", &dir]).status.code(), Some(0), "a bare scan reports, it does not gate");
    assert_eq!(run(&["scan", &dir, "--fail-on", "error"]).status.code(), Some(1));
    assert_eq!(
        run(&["scan", &dir, "--fail-on", "error", "--allow-findings", "999"]).status.code(),
        Some(0)
    );
    assert_eq!(run(&["scan", "/rqx-no-such-path"]).status.code(), Some(2));
}

#[test]
fn one_scan_writes_every_report_it_was_asked_for() {
    let dir = std::env::temp_dir().join(format!("rqx-sinks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let junit = dir.join("rqx.xml");
    let sarif = dir.join("rqx.sarif");

    let output = run(&[
        "scan",
        &fixtures_dir(),
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
    let output = run(&["scan", &fixtures_dir(), "--reporter", "json", "--reporter", "sarif"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("json") && error.contains("sarif"), "{error}");

    let output = run(&["scan", &fixtures_dir(), "--reporter", "terminal", "--reporter", "github"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Health") && text.contains("::error file="), "{text:.400}");
}

/// A display filter that moved the score would make a CI result depend on how it
/// was printed.
#[test]
fn a_display_filter_never_moves_the_health_score() {
    let full = summary_field(&scan(&[]), "health_score");
    for narrowing in [
        ["--severity", "error"],
        ["--category", "bloaters"],
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
fn an_unknown_rule_is_a_tool_error_that_names_the_known_ones() {
    let output = run(&["scan", &fixtures_dir(), "--rule", "no-such-rule"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("long-function"), "{error}");
}

/// Every machine report has to parse, and the paths in it have to resolve from the
/// directory CI actually runs in rather than from the directory that was scanned.
#[test]
fn machine_reports_name_paths_from_the_working_directory() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    for (reporter, needle) in
        [("github", "file=fixtures/"), ("gitlab", "\"path\": \"fixtures/"), ("sarif", "\"uri\": \"fixtures/")]
    {
        let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
            .current_dir(root)
            .args(["scan", "fixtures", "--reporter", reporter])
            .output()
            .expect("failed to run rqx");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(needle), "{reporter} lost the scanned directory:\n{text:.400}");
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
    let config = dir.join("rqx.toml");
    std::fs::write(&config, "[rules]\ndisabled = [\"dispensables\", \"couplers\", \"oo-abusers\"]\n")
        .unwrap();

    let report = scan(&["--config", config.to_str().unwrap()]);
    let fired = counts_by_rule(&report);
    for rule in ["focused-test", "skipped-test", "suppressed-diagnostic", "output-parameter"] {
        assert!(fired.contains_key(rule), "{rule} went silent; fired: {fired:?}");
    }
    for rule in ["unused-local", "message-chain", "temporary-field"] {
        assert!(!fired.contains_key(rule), "{rule} belongs to a disabled family");
    }

    let _ = std::fs::remove_dir_all(&dir);
}
