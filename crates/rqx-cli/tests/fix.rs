use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Every rule `fix` answers today, with a source per language that fires it and the
/// file that source has to become.
const FIXABLE: [(&str, &str, &str, &str); 5] = [
    (
        "typescript",
        "sample.ts",
        "// @author someone\nexport function keep(): number {\n  return 1\n  console.log('dead')\n}\n// const a = 1;\n// const b = 2;\n// const c = 3;\n",
        "export function keep(): number {\n  return 1\n}\n",
    ),
    (
        "python",
        "sample.py",
        "# @author someone\ndef keep():\n    return 1\n    print('dead')\n# a = 1\n# b = 2\n# c = 3\n",
        "def keep():\n    return 1\n",
    ),
    (
        "go",
        "sample.go",
        "package sample\n\n// @author someone\nfunc Keep() int {\n\treturn 1\n\tprintln(\"dead\")\n}\n\n// a := first()\n// b := second()\n// c := third()\n",
        "package sample\n\nfunc Keep() int {\n\treturn 1\n}\n\n",
    ),
    (
        "java",
        "Sample.java",
        "// @author someone\nclass Sample {\n    int keep() {\n        return 1;\n        System.out.println(\"dead\");\n    }\n    // int a = 1;\n    // int b = 2;\n    // int c = 3;\n}\n",
        "class Sample {\n    int keep() {\n        return 1;\n    }\n}\n",
    ),
    (
        "cpp",
        "sample.cpp",
        "// @author someone\nint keep() {\n    return 1;\n    printf(\"dead\");\n}\n// int a = 1;\n// int b = 2;\n// int c = 3;\n",
        "int keep() {\n    return 1;\n}\n",
    ),
];

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-fix-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    for (path, contents) in files {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("a directory");
        }
        std::fs::write(&file, contents).expect("a fixture file");
    }
    root
}

fn rqx(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .output()
        .expect("failed to run rqx");
    assert!(output.status.success(), "rqx exited with {}", output.status);
    String::from_utf8(output.stdout).expect("output was not utf-8")
}

fn fix(root: &Path, extra: &[&str]) -> String {
    let path = root.to_str().expect("a utf-8 path");
    let mut args = vec!["fix", path, "--json"];
    args.extend_from_slice(extra);
    rqx(&args)
}

fn counts_by_rule(report: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for line in report
        .lines()
        .filter(|line| !line.contains("\"kind\":\"summary\""))
    {
        if let Some(rule) = line
            .split("\"rule\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
        {
            *counts.entry(rule.to_owned()).or_default() += 1;
        }
    }
    counts
}

fn summary_field(report: &str, field: &str) -> String {
    let needle = format!("\"{field}\":");
    report
        .lines()
        .find(|line| line.contains("\"kind\":\"summary\""))
        .and_then(|line| line.split(&needle).nth(1))
        .map(|rest| {
            rest.trim_start_matches('"')
                .split(['"', ',', '}'])
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .unwrap_or_else(|| panic!("no {field} in the summary"))
}

#[test]
fn a_preview_writes_nothing() {
    let (language, name, source, _) = FIXABLE[0];
    let root = project("preview", &[(name, source)]);
    let report = fix(&root, &[]);

    assert_eq!(summary_field(&report, "written"), "false");
    assert!(summary_field(&report, "applied").parse::<u32>().unwrap() > 0);
    assert_eq!(
        std::fs::read_to_string(root.join(name)).unwrap(),
        source,
        "{language} was written during a preview"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_language_is_fixed_to_the_file_it_should_become() {
    for (language, name, source, expected) in FIXABLE {
        let root = project(language, &[(name, source)]);
        let report = fix(&root, &["--write"]);

        assert_eq!(summary_field(&report, "written"), "true", "{language}");
        assert_eq!(
            std::fs::read_to_string(root.join(name)).unwrap(),
            expected,
            "{language}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn the_findings_the_fixes_answered_are_gone_and_no_others_appeared() {
    let (_, name, source, _) = FIXABLE[0];
    let root = project("scan-after", &[(name, source)]);
    let path = root.to_str().unwrap();

    let before = counts_by_rule(&rqx(&[
        "scan",
        path,
        "--format",
        "json",
        "--max-findings",
        "0",
    ]));
    fix(&root, &["--write"]);
    let after = counts_by_rule(&rqx(&[
        "scan",
        path,
        "--format",
        "json",
        "--max-findings",
        "0",
    ]));

    for rule in ["unreachable-code", "journal-comment", "commented-out-code"] {
        assert!(
            before.contains_key(rule),
            "{rule} never fired to begin with"
        );
        assert!(!after.contains_key(rule), "{rule} survived the fix");
    }
    for (rule, count) in &after {
        assert!(
            before.get(rule).copied().unwrap_or(0) >= *count,
            "{rule} grew from {:?} to {count}",
            before.get(rule)
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fixing_a_fixed_project_changes_nothing() {
    let (_, name, source, expected) = FIXABLE[0];
    let root = project("idempotent", &[(name, source)]);
    fix(&root, &["--write"]);

    let again = fix(&root, &["--write"]);
    assert_eq!(summary_field(&again, "applied"), "0");
    assert_eq!(std::fs::read_to_string(root.join(name)).unwrap(), expected);
    let _ = std::fs::remove_dir_all(&root);
}

/// A file whose findings the fixer cannot answer is left exactly as it was, and the
/// run still succeeds — a fixer that refuses is not a fixer that fails.
#[test]
fn a_file_with_nothing_mechanical_is_untouched() {
    let source = "export function tangled(a: number, b: number, c: number, d: number, e: number) {\n  if (a) { if (b) { if (c) { if (d) { return e } } } }\n  return 0\n}\n";
    let root = project("untouched", &[("tangled.ts", source)]);
    let report = fix(&root, &[]);

    assert_eq!(summary_field(&report, "applied"), "0");
    assert!(summary_field(&report, "files").parse::<u32>().unwrap() == 0);
    assert_eq!(
        std::fs::read_to_string(root.join("tangled.ts")).unwrap(),
        source
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A project that keeps its commented-out code as documentation says so once in
/// `RQX.toml` rather than remembering a flag on every run.
#[test]
fn a_rule_the_config_disables_is_left_to_a_person() {
    let (_, name, source, _) = FIXABLE[0];
    let root = project(
        "configured",
        &[
            (name, source),
            ("RQX.toml", "[fix]\ndisabled = [\"commented-out-code\"]\n"),
        ],
    );
    let report = fix(&root, &["--write"]);

    assert!(!report.contains("commented-out-code"), "{report}");
    assert!(report.contains("unreachable-code"), "{report}");
    assert!(
        std::fs::read_to_string(root.join(name))
            .unwrap()
            .contains("// const a = 1;")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// One fix making the next possible is the whole reason rounds exist: deleting the
/// dead branch that held the only reference to a local leaves the local unused.
#[test]
fn a_fix_that_enables_another_is_taken_in_a_later_round() {
    let source =
        "export function keep(): number {\n  const once = 2\n  return 1\n  console.log(once)\n}\n";
    let root = project("rounds", &[("rounds.ts", source)]);
    let report = fix(&root, &["--write"]);

    assert!(
        summary_field(&report, "rounds").parse::<u32>().unwrap() >= 2,
        "{report}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("rounds.ts")).unwrap(),
        "export function keep(): number {\n  return 1\n}\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}
