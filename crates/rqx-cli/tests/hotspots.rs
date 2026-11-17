//! `rqx hotspots`: files ranked by churn and by what is wrong with them.

mod common;
use common::Repository;

const DEAD: &str = "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n";
const CLEAN: &str = "export function add(a: number, b: number): number {\n  return a + b\n}\n";

fn history() -> Repository {
    let repository = Repository::new("hotspots");
    repository.commit(
        "start",
        &[
            (
                "rqx.yaml",
                Some("rules:\n  disabled: [unused-file, shotgun-surgery]\n"),
            ),
            ("src/busy.ts", Some(DEAD)),
            ("src/calm.ts", Some(DEAD)),
            ("src/tidy.ts", Some(CLEAN)),
        ],
    );
    for round in 1..=4 {
        repository.commit(
            "edit",
            &[
                ("src/busy.ts", Some(&format!("{DEAD}// {round}\n"))),
                ("src/tidy.ts", Some(&format!("{CLEAN}// {round}\n"))),
            ],
        );
    }
    repository
}

fn rows(lines: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    lines
        .iter()
        .filter(|line| line["file"].is_string())
        .collect()
}

#[test]
fn files_are_ranked_by_commits_times_what_is_wrong_with_them() {
    let repository = history();
    let lines = repository.rqx_lines(&["hotspots", ".", "--reporter", "json"]);
    let rows = rows(&lines);

    let files: Vec<&str> = rows.iter().filter_map(|row| row["file"].as_str()).collect();
    assert_eq!(
        files,
        ["src/busy.ts", "src/calm.ts"],
        "a churned file with no findings is not a hotspot"
    );
    assert_eq!(rows[0]["commits"], 5);
    assert_eq!(rows[1]["commits"], 1);
    assert!(rows[0]["score"].as_f64().unwrap() > rows[1]["score"].as_f64().unwrap());
    assert!(rows[0]["main_rule"].is_string());
    assert!(
        rows[0]["explanation"]
            .as_str()
            .unwrap()
            .starts_with("5 commits in 90 days, ")
    );
}

#[test]
fn top_limits_the_list() {
    let repository = history();
    let lines = repository.rqx_lines(&["hotspots", ".", "--top", "1", "--reporter", "json"]);
    assert_eq!(rows(&lines).len(), 1);
}

#[test]
fn since_sets_the_window_the_explanation_names() {
    let repository = history();
    let lines = repository.rqx_lines(&["hotspots", ".", "--since", "30", "--reporter", "json"]);
    assert!(
        rows(&lines)[0]["explanation"]
            .as_str()
            .unwrap()
            .contains("in 30 days")
    );
}

#[test]
fn the_terminal_view_lists_the_files_with_their_reasons() {
    let repository = history();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(["hotspots", "."])
        .current_dir(&repository.root)
        .env("HOME", &repository.root)
        .env("XDG_CONFIG_HOME", &repository.root)
        .env("NO_COLOR", "1")
        .output()
        .expect("rqx runs");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Hotspots"), "{text}");
    assert!(text.contains("1. src/busy.ts"), "{text}");
    assert!(text.contains("5 commits in 90 days"), "{text}");
    assert!(!text.contains("tidy.ts"), "{text}");
}
