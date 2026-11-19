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

/// `FIXABLE` covers heuristic rules too, so the projects that exercise all of it
/// lower the confidence floor to where rqx 0.5.0 had it.
const EVERY_CONFIDENCE: (&str, &str) = ("rqx.yaml", "fix:\n  min_confidence: low\n");

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
        let root = project(language, &[(name, source), EVERY_CONFIDENCE]);
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
    let root = project("scan-after", &[(name, source), EVERY_CONFIDENCE]);
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
    let root = project("idempotent", &[(name, source), EVERY_CONFIDENCE]);
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

/// The brief exists so a finding reaches an agent without losing the span, the
/// metric or the reasoning on the way.
#[test]
fn the_plan_carries_what_a_reader_needs_and_omits_what_fix_can_answer() {
    let (_, name, source, _) = FIXABLE[0];
    let tangled = "export function tangled(a: number, b: number, c: number, d: number, e: number) {\n  if (a) { if (b) { if (c) { if (d) { return e } } } }\n  return 0\n}\n";
    let root = project("plan", &[(name, source), ("tangled.ts", tangled)]);
    let report = fix(&root, &["--plan"]);

    assert!(!report.contains("unreachable-code"), "{report}");
    let mut seen = 0;
    for line in report.lines() {
        let item: serde_json::Value = serde_json::from_str(line).expect("a json object");
        assert_eq!(item["kind"], "decision");
        for field in [
            "rule",
            "file",
            "source",
            "metric",
            "why",
            "hint",
            "technique",
        ] {
            assert!(
                !item[field].is_null() && item[field] != "",
                "{field} is missing from {item}"
            );
        }
        seen += 1;
    }
    assert!(seen > 0, "the plan named nothing");
    let _ = std::fs::remove_dir_all(&root);
}

/// A project that keeps its commented-out code as documentation says so once in
/// `rqx.yaml` rather than remembering a flag on every run.
#[test]
fn a_rule_the_config_disables_is_left_to_a_person() {
    let (_, name, source, _) = FIXABLE[0];
    let root = project(
        "configured",
        &[
            (name, source),
            ("rqx.yaml", "fix:\n  disabled: [commented-out-code]\n"),
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

/// Go's runner finds a case by name and calls it with a `*testing.T`. Deleting the
/// parameter leaves a function the runner will not call, so the test stops running
/// and nothing says so — which is what `rqx fix --write` did to this project's own
/// Go fixtures before the signature was recognised as the framework's.
#[test]
fn a_go_test_keeps_the_parameter_its_runner_requires() {
    let root = std::env::temp_dir().join(format!("rqx-gotest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("go.mod"), "module example.com/t\n\ngo 1.22\n").expect("go.mod");
    std::fs::write(
        root.join("thing.go"),
        "package thing\n\nfunc Add(a int, b int) int { return a + b }\n",
    )
    .expect("thing.go");
    let test_file = root.join("thing_test.go");
    let source = "package thing\n\nimport \"testing\"\n\n\
         func TestAdd(t *testing.T) {\n\tif Add(1, 2) != 3 {\n\t\tpanic(\"no\")\n\t}\n}\n\n\
         func BenchmarkAdd(b *testing.B) {\n\tAdd(1, 2)\n}\n\n\
         func helper(unused int) {\n}\n";
    std::fs::write(&test_file, source).expect("thing_test.go");

    let reported = String::from_utf8_lossy(
        &std::process::Command::new(env!("CARGO_BIN_EXE_rqx"))
            .args(["scan"])
            .arg(&root)
            .args(["--reporter", "json", "--max-findings", "0"])
            .env("RQX_NO_CACHE", "1")
            .output()
            .expect("failed to run rqx")
            .stdout,
    )
    .into_owned();
    let named = |parameter: &str| {
        reported.lines().any(|line| {
            line.contains("\"unused-parameter\"") && line.contains(&format!("\"{parameter}\""))
        })
    };
    assert!(!named("t"), "reported the `*testing.T` a Go test must take");
    assert!(
        !named("b"),
        "reported the `*testing.B` a benchmark must take"
    );
    // The rule still works in the same file: only the framework's own signature is
    // exempt, not every parameter that happens to sit beside one.
    assert!(named("unused"), "the rule stopped working:\n{reported}");

    std::process::Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["fix"])
        .arg(&root)
        .args(["--write", "--no-tui"])
        .env("RQX_NO_CACHE", "1")
        .output()
        .expect("failed to run rqx");
    let after = std::fs::read_to_string(&test_file).expect("reading back");
    assert!(after.contains("func TestAdd(t *testing.T)"), "{after}");
    assert!(after.contains("func BenchmarkAdd(b *testing.B)"), "{after}");

    let _ = std::fs::remove_dir_all(&root);
}

/// The default floor applies verified fixes and leaves the heuristic ones, and the
/// summary counts what it left.
#[test]
fn the_default_floor_applies_verified_fixes_and_counts_the_rest() {
    let (_, name, source, _) = FIXABLE[0];
    let root = project("floor", &[(name, source)]);
    let report = fix(&root, &["--write"]);

    let after = std::fs::read_to_string(root.join(name)).unwrap();
    assert!(!after.contains("console.log('dead')"), "{after}");
    assert!(after.contains("// const a = 1;"), "{after}");
    assert!(
        summary_field(&report, "below_floor")
            .parse::<u32>()
            .unwrap()
            >= 1,
        "{report}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The cobra licence header: 0.5.0 deleted its URL line from every file as a banner.
#[test]
fn a_licence_header_survives_fixing_at_every_confidence() {
    let header = "\
// Copyright 2013-2023 The Cobra Authors
//
// Licensed under the Apache License, Version 2.0 (the \"License\");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an \"AS IS\" BASIS.
";
    let source = format!(
        "{header}\npackage cobra\n\n// ---------------- helpers ----------------\n\nfunc Keep() int {{\n\t// https://github.com/spf13/cobra/issues/1257\n\treturn 1\n\tprintln(\"dead\")\n}}\n"
    );
    let root = project(
        "licence",
        &[
            ("go.mod", "module example.com/cobra\n\ngo 1.22\n"),
            ("cobra.go", &source),
            EVERY_CONFIDENCE,
        ],
    );
    fix(&root, &["--write"]);

    let after = std::fs::read_to_string(root.join("cobra.go")).unwrap();
    assert!(after.starts_with(header), "{after}");
    assert!(
        after.contains("// https://github.com/spf13/cobra/issues/1257"),
        "{after}"
    );
    assert!(!after.contains("println(\"dead\")"), "{after}");
    assert!(!after.contains("---- helpers ----"), "{after}");
    let _ = std::fs::remove_dir_all(&root);
}

/// cobra's `preExecHookFn`: declared at package level in two build-tagged files and
/// read by a third. 0.5.0 deleted both declarations as unused locals.
#[test]
fn a_package_level_var_read_by_a_sibling_file_is_kept() {
    let windows =
        "//go:build windows\n\npackage cobra\n\nvar preExecHookFn = func(c *Command) {}\n";
    let other = "//go:build !windows\n\npackage cobra\n\nvar preExecHookFn func(*Command)\n";
    let reader = "package cobra\n\ntype Command struct{}\n\nfunc (c *Command) Execute() {\n\tif preExecHookFn != nil {\n\t\tpreExecHookFn(c)\n\t}\n}\n";
    let root = project(
        "hook",
        &[
            ("go.mod", "module example.com/cobra\n\ngo 1.22\n"),
            ("command_win.go", windows),
            ("command_notwin.go", other),
            ("command.go", reader),
            EVERY_CONFIDENCE,
        ],
    );
    fix(&root, &["--write"]);

    assert_eq!(
        std::fs::read_to_string(root.join("command_win.go")).unwrap(),
        windows
    );
    assert_eq!(
        std::fs::read_to_string(root.join("command_notwin.go")).unwrap(),
        other
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The shape behind cobra's `completions.go:187: too many return values`. The
/// results stay, and a real unused parameter of a Go function is not removed either,
/// because another file of the package may call it.
#[test]
fn a_go_function_keeps_its_signature() {
    let source = "\
package cobra

type CompletionFunc func()

type Command struct{}

func (c *Command) GetFlagCompletionFunc(flagName string) (CompletionFunc, bool) {
\treturn nil, false
}

func checkIfFlagCompletion(args []string, lastArg string) ([]string, string, error) {
\treturn args, \"\", nil
}

func complete(c *Command, args []string, toComplete string) {
\tc.GetFlagCompletionFunc(\"x\")
\t_, _, _ = checkIfFlagCompletion(args, toComplete)
}
";
    let root = project(
        "signature",
        &[
            ("go.mod", "module example.com/cobra\n\ngo 1.22\n"),
            ("completions.go", source),
            EVERY_CONFIDENCE,
        ],
    );
    let report = fix(&root, &["--write"]);

    assert_eq!(
        std::fs::read_to_string(root.join("completions.go")).unwrap(),
        source,
        "{report}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `--verify-build` runs `go vet` on a copy, so it needs the toolchain; without it the
/// test has nothing to check.
#[test]
fn verify_build_keeps_a_fix_that_builds() {
    if Command::new("go").arg("version").output().is_err() {
        return;
    }
    // A journal comment rather than dead code: `go vet` reports unreachable code, so
    // a project holding some fails the check before any edit.
    let source = "package keep\n\n// @author someone\nfunc Keep() int {\n\treturn 1\n}\n";
    let root = project(
        "verify-build",
        &[
            ("go.mod", "module example.com/keep\n\ngo 1.22\n"),
            ("keep.go", source),
        ],
    );
    let report = fix(&root, &["--write", "--verify-build"]);

    assert_eq!(summary_field(&report, "applied"), "1", "{report}");
    assert_eq!(
        std::fs::read_to_string(root.join("keep.go")).unwrap(),
        "package keep\n\nfunc Keep() int {\n\treturn 1\n}\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `go vet ./...` in `root`, when the toolchain is installed; without it there is
/// nothing more to check.
fn assert_vets(root: &Path) {
    let Ok(output) = Command::new("go")
        .args(["vet", "./..."])
        .current_dir(root)
        .env("GOWORK", "off")
        .output()
    else {
        return;
    };
    assert!(
        output.status.success(),
        "go vet failed after the fix:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn scan_json(root: &Path) -> String {
    rqx(&[
        "scan",
        root.to_str().expect("a utf-8 path"),
        "--format",
        "json",
        "--max-findings",
        "0",
    ])
}

/// klauspost/compress and go-runewidth: a label after a `return` that a `goto` jumps
/// to is live, and so is a `yield` that makes a Python function a generator and a
/// call spelled as a C++ macro. 0.5.2 deleted all three as unreachable code.
#[test]
fn fix_keeps_code_that_is_reached_without_falling_through() {
    let go_source = "package flow\n\nfunc Clamp(n int) int {\n\tif n > 3 {\n\t\tgoto done\n\t}\n\treturn 0\ndone:\n\tn = 3\n\treturn n\n}\n";
    let python = "def events(log):\n    log(\"start\")\n    return\n    yield\n";
    let cpp = "int release(int* handle, int status) {\n    if (status != 0) goto cleanup;\n    return 0;\ncleanup:\n    delete handle;\n    return status;\n}\n\nint squared(int value) {\n    return value * value;\n    NOT_REACHED();\n}\n";
    let root = project(
        "reached",
        &[
            ("go.mod", "module example.com/flow\n\ngo 1.22\n"),
            ("flow.go", go_source),
            ("events.py", python),
            ("native/release.cpp", cpp),
            EVERY_CONFIDENCE,
        ],
    );
    assert!(!scan_json(&root).contains("\"unreachable-code\""));
    fix(&root, &["--write"]);

    assert_eq!(
        std::fs::read_to_string(root.join("flow.go")).unwrap(),
        go_source
    );
    assert_eq!(
        std::fs::read_to_string(root.join("events.py")).unwrap(),
        python
    );
    assert_eq!(
        std::fs::read_to_string(root.join("native/release.cpp")).unwrap(),
        cpp
    );
    assert_vets(&root);
    let _ = std::fs::remove_dir_all(&root);
}
