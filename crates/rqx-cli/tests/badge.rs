//! `rqx badge`: the health score as a self-contained SVG.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rqx-badge-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temp project");
    std::fs::write(root.join("a.ts"), source).expect("a file");
    root
}

fn rqx(root: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rqx"))
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .output()
        .expect("rqx runs")
}

#[test]
fn a_clean_project_gets_a_pristine_badge_on_standard_output() {
    let root = project(
        "stdout",
        "export function add(a: number, b: number): number {\n  return a + b\n}\n",
    );
    let output = rqx(&root, &["badge", "."]);
    let svg = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(svg.starts_with("<svg "), "{svg}");
    assert!(svg.contains("rqx: health"), "{svg}");
}

#[test]
fn output_writes_the_file_and_leaves_standard_output_empty() {
    let root = project(
        "file",
        "export function keep(): number {\n  return 1\n  console.log('dead')\n}\n",
    );
    let output = rqx(&root, &["badge", ".", "--output", "badge.svg"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());

    let svg = std::fs::read_to_string(root.join("badge.svg")).expect("the badge");
    assert!(svg.contains("<title>rqx: health "), "{svg}");
    assert!(svg.trim_end().ends_with("</svg>"));
}
