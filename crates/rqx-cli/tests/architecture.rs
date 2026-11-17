use std::path::{Path, PathBuf};
use std::process::Command;

/// The layering rules are silent until a project declares one, so unlike every other
/// rule they cannot be proven by the shared fixture corpus. Each language gets the
/// same shape: an inner layer reaching for an outer one, and a subtree reaching past
/// what it was allowed.
const PLAN: &str = r#"
architecture:
  layers: [domain, adapters]
  paths:
    "domain/**": { layer: domain }
    "adapters/**": { layer: adapters }
    "scripts/**": { may_only_import: ["adapters/**"] }
"#;

struct Case {
    language: &'static str,
    files: &'static [(&'static str, &'static str)],
}

const CASES: &[Case] = &[
    Case {
        language: "typescript",
        files: &[
            (
                "adapters/db.ts",
                "export function save(n: number): number {\n  return n\n}\n",
            ),
            (
                "domain/pricing.ts",
                "import { save } from '../adapters/db'\nexport function price(): number {\n  return save(1)\n}\n",
            ),
            (
                "scripts/report.ts",
                "import { price } from '../domain/pricing'\nexport const out = price()\n",
            ),
        ],
    },
    Case {
        language: "python",
        files: &[
            ("adapters/__init__.py", ""),
            ("domain/__init__.py", ""),
            ("scripts/__init__.py", ""),
            ("adapters/db.py", "def save(n):\n    return n\n"),
            (
                "domain/pricing.py",
                "from ..adapters.db import save\n\n\ndef price():\n    return save(1)\n",
            ),
            (
                "scripts/report.py",
                "from ..domain.pricing import price\n\nout = price()\n",
            ),
        ],
    },
    Case {
        language: "go",
        files: &[
            ("go.mod", "module example\n\ngo 1.21\n"),
            (
                "adapters/db.go",
                "package adapters\n\nfunc Save(n int) int {\n\treturn n\n}\n",
            ),
            (
                "domain/pricing.go",
                "package domain\n\nimport \"example/adapters\"\n\nfunc Price() int {\n\treturn adapters.Save(1)\n}\n",
            ),
            (
                "scripts/report.go",
                "package scripts\n\nimport \"example/domain\"\n\nvar Out = domain.Price()\n",
            ),
        ],
    },
];

fn project(case: &Case) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("rqx-arch-{}-{}", case.language, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("rqx.yaml"), PLAN).expect("the plan");
    for (path, text) in case.files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(&file, text).expect("a fixture file");
    }
    root
}

fn rules_fired(root: &Path) -> Vec<(String, String)> {
    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args([
            "scan",
            root.to_str().expect("utf-8"),
            "--format",
            "json",
            "--max-findings",
            "0",
        ])
        .output()
        .expect("failed to run rqx");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            let rule = value["rule"].as_str()?.to_owned();
            let file = value["location"]["file"].as_str()?.to_owned();
            Some((rule, file))
        })
        .collect()
}

#[test]
fn an_inner_layer_reaching_outward_is_reported_in_every_language() {
    for case in CASES {
        let root = project(case);
        let fired = rules_fired(&root);

        assert!(
            fired
                .iter()
                .any(|(rule, file)| rule == "layer-violation" && file.contains("pricing")),
            "{}: no layer violation on the domain file: {fired:?}",
            case.language
        );
        assert!(
            fired
                .iter()
                .any(|(rule, file)| rule == "forbidden-import" && file.contains("report")),
            "{}: the script reached past what it may import: {fired:?}",
            case.language
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// The outward direction is the whole point: an adapter depending on the domain is
/// what the layering is for, not a violation of it.
#[test]
fn an_outer_layer_depending_inward_is_allowed() {
    let root = std::env::temp_dir().join(format!("rqx-arch-inward-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("domain")).expect("a directory");
    std::fs::create_dir_all(root.join("adapters")).expect("a directory");
    std::fs::write(root.join("rqx.yaml"), PLAN).expect("the plan");
    std::fs::write(
        root.join("domain/pricing.ts"),
        "export function price(): number {\n  return 1\n}\n",
    )
    .expect("a file");
    std::fs::write(
        root.join("adapters/db.ts"),
        "import { price } from '../domain/pricing'\nexport const stored = price()\n",
    )
    .expect("a file");

    let fired = rules_fired(&root);
    assert!(
        !fired.iter().any(|(rule, _)| rule == "layer-violation"),
        "an adapter depending on the domain is the arrangement, not a violation: {fired:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A layer named where files live but never declared in `layers` would govern nothing,
/// which is the silent misconfiguration the whole table exists to prevent.
#[test]
fn a_layer_that_was_never_declared_is_refused() {
    let root = std::env::temp_dir().join(format!("rqx-arch-typo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a directory");
    std::fs::write(
        root.join("rqx.yaml"),
        "architecture:\n  layers: [domain]\n  paths:\n    \"src/**\": { layer: adaptors }\n",
    )
    .expect("the plan");
    std::fs::write(root.join("a.ts"), "export const a = 1\n").expect("a file");

    let output = Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["scan", root.to_str().expect("utf-8")])
        .output()
        .expect("failed to run rqx");
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "a typo'd layer must not be ignored"
    );
    assert!(message.contains("adaptors"), "{message}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Clone detection is per-language by design: the hash reads node ids a grammar
/// assigns, and no two grammars agree on them. This is the second hash, over a
/// vocabulary none of them owns.
#[test]
fn the_same_rule_written_in_two_languages_is_found() {
    let root = std::env::temp_dir().join(format!("rqx-ported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a directory");
    std::fs::write(
        root.join("rqx.yaml"),
        "rules:\n  enabled: [cross-language-duplicate]\n",
    )
    .expect("the config");
    std::fs::write(
        root.join("validate.ts"),
        "export function validatePrice(amount: number, currency: string): string {\n\
           if (amount < 0) {\n    return \"negative\";\n  }\n\
           if (amount > 1000000) {\n    return \"too large\";\n  }\n\
           if (currency !== \"USD\") {\n    return \"unsupported\";\n  }\n\
           if (currency !== \"EUR\") {\n    return \"unsupported\";\n  }\n\
           return \"ok\";\n}\n",
    )
    .expect("a file");
    std::fs::write(
        root.join("validate.py"),
        "def validate_price(amount, currency):\n\
         \x20   if amount < 0:\n        return \"negative\"\n\
         \x20   if amount > 1000000:\n        return \"too large\"\n\
         \x20   if currency != \"USD\":\n        return \"unsupported\"\n\
         \x20   if currency != \"EUR\":\n        return \"unsupported\"\n\
         \x20   return \"ok\"\n",
    )
    .expect("a file");

    let fired = rules_fired(&root);
    assert!(
        fired
            .iter()
            .any(|(rule, _)| rule == "cross-language-duplicate"),
        "the same validation in two languages went unreported: {fired:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Two blocks that merely branch and return in the same order are not the same rule.
/// The shared constants are what separate a port from a coincidence.
#[test]
fn two_unrelated_blocks_that_merely_branch_alike_are_not_reported() {
    let root = std::env::temp_dir().join(format!("rqx-unported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a directory");
    std::fs::write(
        root.join("rqx.yaml"),
        "rules:\n  enabled: [cross-language-duplicate]\n",
    )
    .expect("the config");
    std::fs::write(
        root.join("a.ts"),
        "export function widen(a: number): number {\n  if (a < 1) {\n    return 11;\n  }\n  return 22;\n}\n",
    )
    .expect("a file");
    std::fs::write(
        root.join("b.py"),
        "def narrow(b):\n    if b < 2:\n        return 33\n    return 44\n",
    )
    .expect("a file");

    let fired = rules_fired(&root);
    assert!(
        !fired
            .iter()
            .any(|(rule, _)| rule == "cross-language-duplicate"),
        "two short unrelated bodies were called a port: {fired:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
