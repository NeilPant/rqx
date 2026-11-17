//! `rqx setup agent`: the instructions that make a coding agent run rqx before it
//! finishes, and for Claude Code a Stop hook that enforces it.
//!
//! Both files belong to the project, so nothing here overwrites them. The
//! instructions live between two marker comments: the first run appends the block,
//! later runs replace only what is between the markers, and the rest of the file is
//! never read as anything but text to keep. The settings file is parsed and one entry
//! is added to it.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::{Value, json};

use crate::fix::write_atomically;

const BEGIN: &str = "<!-- rqx:begin -->";
const END: &str = "<!-- rqx:end -->";

/// What identifies the hook as rqx's, so a second run does not add it again.
const CHECK_COMMAND: &str = "rqx check --staged";

/// A Stop hook blocks the agent from finishing on exit code 2 and shows it the
/// standard error output. `rqx check` exits 1 for a failed gate and writes its report
/// to standard output, so the command moves the report and maps that one exit code.
/// Any other result, including rqx failing to run, does not hold the agent up.
const STOP_COMMAND: &str = "rqx check --staged >&2; [ $? -ne 1 ] || exit 2";

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
    Cursor,
    Generic,
}

impl Agent {
    /// Codex and Cursor read `AGENTS.md`, as does any agent that follows the
    /// convention. Claude Code reads `CLAUDE.md`.
    fn instructions_file(self) -> &'static str {
        match self {
            Self::Claude => "CLAUDE.md",
            Self::Codex | Self::Cursor | Self::Generic => "AGENTS.md",
        }
    }
}

/// How an output file is combined with what the project already has there.
type Merge = fn(&str) -> Result<String>;

/// One file `rqx setup agent` produces.
pub struct Output {
    /// Relative to the directory rqx is run from.
    pub path: &'static str,
    /// What the file gains, as it is printed without `--write`.
    pub shown: String,
    merge: Merge,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Created,
    Updated,
    Unchanged,
}

pub fn outputs(agent: Agent) -> Vec<Output> {
    let mut outputs = vec![Output {
        path: agent.instructions_file(),
        shown: section(),
        merge: with_section,
    }];
    if agent == Agent::Claude {
        outputs.push(Output {
            path: ".claude/settings.json",
            shown: stop_hook_settings(),
            merge: with_stop_hook,
        });
    }
    outputs
}

impl Output {
    /// Merges this output into the file under `directory`, creating it if needed.
    /// A file that cannot be merged without losing something is an error and is left
    /// as it was.
    pub fn apply(&self, directory: &Path) -> Result<Change> {
        let path = directory.join(self.path);
        let existing = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
        };
        let merged = (self.merge)(existing.as_deref().unwrap_or_default())
            .with_context(|| format!("merging into {}", path.display()))?;
        if existing.as_deref() == Some(merged.as_str()) {
            return Ok(Change::Unchanged);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        write_atomically(&path, &merged)?;
        Ok(if existing.is_some() {
            Change::Updated
        } else {
            Change::Created
        })
    }
}

fn section() -> String {
    format!(
        "{BEGIN}\n\
## Static analysis with rqx\n\
\n\
rqx reports dead code, duplication, complexity and architectural drift, and names the \
refactoring that resolves each finding. Run it before finishing a task.\n\
\n\
1. Stage the changes with `git add`, then run `rqx check --staged`. When the changes are \
already committed on a branch, run `rqx check --base <branch>` instead. Exit code 1 means \
the gate failed. Exit code 2 means rqx could not run.\n\
2. Read each finding. It gives the rule, the location, the measured fact and the named \
refactoring. `rqx explain <rule>` says what a rule measures and why.\n\
3. Apply the named refactoring to the code the finding points at.\n\
4. Confirm the result with `rqx verify --base <branch> --expect <rule>@<file>`. It grades \
what the change did to that finding and reports any finding the change introduced.\n\
\n\
When the rqx MCP server is configured (`rqx mcp`), use its tools instead of parsing the \
command output: `check` and `verify` take the same base revision, `findings` pages the \
findings, `explain` describes a rule, and `fix_preview` shows the mechanical fixes rqx \
can prove safe as diffs, without writing them.\n\
{END}"
    )
}

/// Appends the block, or replaces the one already there. A file with a begin marker and
/// no end marker, or the two in the wrong order, is refused: replacing from a marker to
/// a guessed end could delete the user's text.
fn with_section(existing: &str) -> Result<String> {
    let block = section();
    let begin = existing.find(BEGIN);
    let end = existing.find(END);
    match (begin, end) {
        (None, None) => Ok(appended(existing, &block)),
        (Some(begin), Some(end)) if begin < end && !has_second_block(existing) => Ok(format!(
            "{}{block}{}",
            &existing[..begin],
            &existing[end + END.len()..]
        )),
        _ => bail!(
            "it has `{BEGIN}` and `{END}` markers that are missing, repeated or out of \
             order. Fix them by hand, or remove the block, then run this again"
        ),
    }
}

fn has_second_block(existing: &str) -> bool {
    existing.matches(BEGIN).count() > 1 || existing.matches(END).count() > 1
}

fn appended(existing: &str, block: &str) -> String {
    let kept = existing.trim_end();
    if kept.is_empty() {
        format!("{block}\n")
    } else {
        format!("{kept}\n\n{block}\n")
    }
}

fn stop_hook_entry() -> Value {
    json!({ "hooks": [{ "type": "command", "command": STOP_COMMAND }] })
}

fn stop_hook_settings() -> String {
    let settings = json!({ "hooks": { "Stop": [stop_hook_entry()] } });
    pretty(&settings)
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

fn runs_check(entry: &Value) -> bool {
    entry["hooks"].as_array().is_some_and(|hooks| {
        hooks.iter().any(|hook| {
            hook["command"]
                .as_str()
                .is_some_and(|command| command.contains(CHECK_COMMAND))
        })
    })
}

/// Adds the Stop hook to the settings, keeping every other key and every other hook.
/// A file that is not a JSON object, or whose `hooks` or `Stop` has another shape, is
/// refused rather than rewritten.
fn with_stop_hook(existing: &str) -> Result<String> {
    let mut settings: Value = if existing.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(existing).context("it is not valid JSON; merge the hook by hand")?
    };
    let stop = stop_hooks(&mut settings)?;
    if stop.iter().any(runs_check) {
        return Ok(existing.to_owned());
    }
    stop.push(stop_hook_entry());
    Ok(pretty(&settings))
}

fn stop_hooks(settings: &mut Value) -> Result<&mut Vec<Value>> {
    let shape = || anyhow::anyhow!("`hooks.Stop` is not a list of hook entries; merge by hand");
    let Some(object) = settings.as_object_mut() else {
        bail!("the top level is not a JSON object; merge the hook by hand");
    };
    object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(shape)?
        .entry("Stop")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(text: &str, needle: &str) -> usize {
        text.matches(needle).count()
    }

    #[test]
    fn a_new_file_is_the_block_alone() {
        let written = with_section("").unwrap();
        assert!(written.starts_with(BEGIN));
        assert!(written.ends_with(&format!("{END}\n")));
    }

    #[test]
    fn the_block_is_appended_after_what_the_file_already_says() {
        let written = with_section("# Project\n\nUse tabs.\n").unwrap();
        assert!(written.starts_with("# Project\n\nUse tabs.\n\n<!-- rqx:begin -->"));
        assert_eq!(count(&written, BEGIN), 1);
    }

    #[test]
    fn running_it_again_changes_nothing() {
        let once = with_section("# Project\n").unwrap();
        assert_eq!(with_section(&once).unwrap(), once);
    }

    #[test]
    fn an_old_block_is_replaced_and_the_text_around_it_is_kept() {
        let old = format!("before\n\n{BEGIN}\nstale advice\n{END}\n\nafter\n");
        let written = with_section(&old).unwrap();
        assert!(written.starts_with("before\n\n"));
        assert!(written.ends_with("\n\nafter\n"));
        assert!(!written.contains("stale advice"));
        assert_eq!(count(&written, BEGIN), 1);
        assert_eq!(count(&written, END), 1);
    }

    #[test]
    fn markers_that_do_not_pair_up_are_refused_and_nothing_is_guessed() {
        for broken in [
            format!("text\n{BEGIN}\nno end\n"),
            format!("{END}\nbackwards\n{BEGIN}\n"),
            format!("{BEGIN}\na\n{END}\n{BEGIN}\nb\n{END}\n"),
            format!("only an end\n{END}\n"),
        ] {
            assert!(with_section(&broken).is_err(), "{broken}");
        }
    }

    #[test]
    fn the_block_names_the_commands_an_agent_needs() {
        let block = section();
        for command in [
            "rqx check --staged",
            "rqx check --base",
            "rqx verify --base <branch> --expect <rule>@<file>",
            "rqx mcp",
            "fix_preview",
        ] {
            assert!(block.contains(command), "{command}");
        }
    }

    #[test]
    fn the_hook_is_added_to_an_empty_settings_file() {
        let written = with_stop_hook("").unwrap();
        let parsed: Value = serde_json::from_str(&written).unwrap();
        assert!(runs_check(&parsed["hooks"]["Stop"][0]));
        assert!(written.ends_with('\n'));
    }

    #[test]
    fn the_hook_is_merged_beside_every_other_key_and_hook() {
        let existing = r#"{
            "permissions": { "allow": ["Bash(ls)"] },
            "hooks": {
                "PreToolUse": [{ "matcher": "Bash", "hooks": [] }],
                "Stop": [{ "hooks": [{ "type": "command", "command": "notify" }] }]
            }
        }"#;
        let parsed: Value = serde_json::from_str(&with_stop_hook(existing).unwrap()).unwrap();

        assert_eq!(parsed["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(parsed["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        let stop = parsed["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "notify");
        assert!(runs_check(&stop[1]));
    }

    #[test]
    fn a_hook_that_is_already_there_leaves_the_file_byte_for_byte() {
        let once = with_stop_hook("{\"model\":\"opus\"}").unwrap();
        assert_eq!(with_stop_hook(&once).unwrap(), once);
        let handwritten = r#"{ "hooks": { "Stop": [ { "hooks": [
            { "type": "command", "command": "rqx check --staged --fail-on error" } ] } ] } }"#;
        assert_eq!(with_stop_hook(handwritten).unwrap(), handwritten);
    }

    #[test]
    fn settings_that_cannot_be_merged_are_refused() {
        for broken in [
            "not json",
            "[1, 2]",
            r#"{ "hooks": [] }"#,
            r#"{ "hooks": { "Stop": {} } }"#,
        ] {
            assert!(with_stop_hook(broken).is_err(), "{broken}");
        }
    }

    #[test]
    fn only_claude_gets_a_settings_file() {
        let paths = |agent| {
            outputs(agent)
                .iter()
                .map(|output| output.path)
                .collect::<Vec<_>>()
        };
        assert_eq!(paths(Agent::Claude), ["CLAUDE.md", ".claude/settings.json"]);
        for agent in [Agent::Codex, Agent::Cursor, Agent::Generic] {
            assert_eq!(paths(agent), ["AGENTS.md"]);
        }
    }

    #[test]
    fn applying_reports_what_happened_to_the_file() {
        let directory = std::env::temp_dir().join(format!("rqx-agent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let outputs = outputs(Agent::Claude);

        let first: Vec<_> = outputs
            .iter()
            .map(|o| o.apply(&directory).unwrap())
            .collect();
        let second: Vec<_> = outputs
            .iter()
            .map(|o| o.apply(&directory).unwrap())
            .collect();

        assert_eq!(first, [Change::Created, Change::Created]);
        assert_eq!(second, [Change::Unchanged, Change::Unchanged]);
        std::fs::write(directory.join("CLAUDE.md"), "# Mine\n").unwrap();
        assert_eq!(outputs[0].apply(&directory).unwrap(), Change::Updated);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_file_that_cannot_be_merged_is_left_as_it_was() {
        let directory = std::env::temp_dir().join(format!("rqx-agent-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join(".claude")).unwrap();
        let settings = directory.join(".claude/settings.json");
        std::fs::write(&settings, "{ not json").unwrap();

        let result = outputs(Agent::Claude)[1].apply(&directory);

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ not json");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
