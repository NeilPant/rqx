//! Ownership from a CODEOWNERS file, end to end: the owners on findings, the table of
//! owners and the per-owner gate, on a project that is not a repository.

mod common;
use common::Scratch;

use std::path::Path;
use std::process::{Command, Output};

const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";
const DIRTY: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";

fn project(name: &str, files: &[(&str, &str)]) -> Scratch {
    let root = Scratch::new(&format!("ownership-{name}"));
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(file, text).expect("a file");
    }
    root
}

fn owned_project(name: &str) -> Scratch {
    project(
        name,
        &[
            ("web/a.ts", DIRTY),
            ("api/b.ts", CLEAN),
            (".github/CODEOWNERS", "web/ @web\napi/ @api\n"),
        ],
    )
}

fn rqx(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env_remove("RQX_CONFIG")
        .env_remove("RQX_POLICY")
        .env_remove("RQX_LEVEL")
        .output()
        .expect("failed to run rqx")
}

fn lines(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn json_findings_and_the_summary_carry_owners() {
    let root = owned_project("json");
    let scan = lines(&rqx(&root, &["scan", ".", "--reporter", "json"]));

    let finding = scan.iter().find(|line| line["kind"] == "finding");
    assert_eq!(finding.expect("a finding")["owners"][0], "@web");
    let summary = scan.last().expect("a summary");
    assert_eq!(summary["owners"]["@api"]["files"], 1);
    assert!(summary["owners"]["@web"]["findings"].as_u64() > Some(0));
}

#[test]
fn without_a_codeowners_file_the_json_has_no_owners() {
    let root = project("none", &[("web/a.ts", DIRTY)]);
    let scan = lines(&rqx(&root, &["scan", ".", "--reporter", "json"]));
    assert!(scan.iter().all(|line| line.get("owners").is_none()));
}

#[test]
fn the_owners_command_ranks_the_owners_worst_first() {
    let root = owned_project("table");
    let table = lines(&rqx(&root, &["owners", ".", "--reporter", "json"]));
    let names: Vec<&str> = table
        .iter()
        .filter_map(|row| row["owner"].as_str())
        .collect();
    assert_eq!(names, ["@web", "@api"]);

    let text = stdout(&rqx(&root, &["owners", "."]));
    assert!(text.contains("@web") && text.contains("@api"), "{text}");
}

#[test]
fn grouping_by_owner_lists_the_findings_under_their_owner() {
    let root = owned_project("group");
    let text = stdout(&rqx(&root, &["scan", ".", "--group-by", "owner"]));
    assert!(text.contains("@web"), "{text}");
}

#[test]
fn an_owner_gate_fails_the_run_and_names_the_owner() {
    let root = project(
        "gate",
        &[
            ("web/a.ts", DIRTY),
            ("api/b.ts", CLEAN),
            (".github/CODEOWNERS", "web/ @web\napi/ @api\n"),
            (
                "rqx.yaml",
                "gate:\n  owners:\n    \"@web\": { min_health: 100 }\n    \"@api\": { min_health: 1 }\n",
            ),
        ],
    );
    let output = rqx(&root, &["scan", ".", "--fail-on", "never"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let text = format!(
        "{}{}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("`@web` health score"), "{text}");
    assert!(!text.contains("`@api` health score"), "{text}");
}
