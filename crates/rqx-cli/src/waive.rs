//! Writing a waiver: `rqx waive`, and the editor's `rqx.suppress`, which asks for the
//! same comment. The grammar lives in `rqx_core::suppress`; this module decides what
//! may be asked for and what the file looks like afterwards.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use rqx_core::rule::RuleId;
use rqx_core::suppress::{self, Waiver};
use rqx_core::{Language, RqxConfig};

use crate::report;

/// Lines of the file shown either side of the insertion in a dry run.
const CONTEXT: usize = 2;

/// What `rqx waive` was asked, as the command line spells it.
pub struct Order {
    /// `FILE:LINE`, or `FILE` alone for a whole-file waiver.
    pub at: String,
    pub rule: String,
    pub reason: String,
    pub until: Option<String>,
    pub whole_file: bool,
    pub dry_run: bool,
    pub config: Option<PathBuf>,
}

pub struct Request<'a> {
    /// One-based line of the finding. Ignored for a whole-file waiver.
    pub line: u32,
    pub rule: &'a str,
    pub reason: &'a str,
    pub until: Option<&'a str>,
    pub whole_file: bool,
}

/// Writes the waiver, or with `dry_run` prints the diff it would make. Says where it
/// went either way, so the reader can open the file at the line.
pub fn run(order: &Order, out: &mut dyn Write) -> Result<()> {
    let (path, line) = parse_target(&order.at, order.whole_file)?;
    let display = path.display().to_string();
    let language = Language::of_path(&path)
        .ok_or_else(|| anyhow!("`{display}` is not a language rqx reads"))?;
    let source = std::fs::read_to_string(&path).with_context(|| format!("reading {display}"))?;
    let settings = crate::config::resolve(&path, order.config.as_deref(), None)?;
    require_rule(&order.rule, &settings.config)?;
    require_reason(&order.reason)?;

    let waiver = plan(
        &source,
        language,
        &Request {
            line,
            rule: &order.rule,
            reason: &order.reason,
            until: order.until.as_deref(),
            whole_file: order.whole_file,
        },
    )?;
    if order.dry_run {
        write!(out, "{}", diff(&display, &source, &waiver)?)?;
        return Ok(());
    }
    apply(&path, &source, &waiver)?;
    writeln!(
        out,
        "  waived `{}` at {display}:{}",
        order.rule, waiver.line
    )?;
    Ok(())
}

/// `FILE:LINE`. The last colon splits them, so a drive letter or a colon in a
/// directory name stays part of the path.
pub fn parse_target(at: &str, whole_file: bool) -> Result<(PathBuf, u32)> {
    if let Some((file, line)) = at.rsplit_once(':')
        && let Ok(line) = line.parse::<u32>()
    {
        if line == 0 {
            bail!("lines start at 1, so `{at}` names no line");
        }
        return Ok((PathBuf::from(file), line));
    }
    if whole_file {
        return Ok((PathBuf::from(at), 1));
    }
    bail!("`{at}` is not FILE:LINE; a waiver goes above a line (or use --whole-file for the file)")
}

/// A waiver without a reason is refused by the scan, so writing one would write a
/// comment that silences nothing.
pub fn require_reason(reason: &str) -> Result<&str> {
    let reason = reason.trim();
    if reason.is_empty() {
        bail!("a waiver needs a reason, or rqx will not honour it");
    }
    Ok(reason)
}

/// The slug has to be one the scan can report, or the comment would sit in the source
/// naming a rule that never fires.
pub fn require_rule(rule: &str, config: &RqxConfig) -> Result<()> {
    if RuleId::parse(rule).is_some() || config.custom.iter().any(|custom| custom.id == rule) {
        return Ok(());
    }
    let mut known: Vec<&str> = RuleId::ALL.iter().map(|rule| rule.slug()).collect();
    known.extend(config.custom.iter().map(|custom| custom.id.as_str()));
    Err(report::unknown(
        "rule",
        rule,
        &known,
        "Run `rqx rules` for the whole catalogue.",
    ))
}

/// What to insert and where, from the file's current text. Pure: nothing is read or
/// written, so the editor and the command line answer the same way.
pub fn plan(source: &str, language: Language, request: &Request<'_>) -> Result<Waiver> {
    let reason = require_reason(request.reason)?;
    suppress::waiver(
        source,
        language,
        request.line,
        request.rule,
        reason,
        request.until,
        request.whole_file,
    )
    .ok_or_else(|| anyhow!("the waiver could not be written; check the date is YYYY-MM-DD"))
}

/// The file with the waiver in it, in the line endings the file already uses.
pub fn inserted(source: &str, waiver: &Waiver) -> Result<String> {
    let at = line_start(source, waiver.line)?;
    let text = if source.contains("\r\n") {
        waiver.text.replace('\n', "\r\n")
    } else {
        waiver.text.clone()
    };
    let mut after = String::with_capacity(source.len() + text.len() + 1);
    after.push_str(&source[..at]);
    if at == source.len() && !source.is_empty() && !source.ends_with('\n') {
        after.push('\n');
    }
    after.push_str(&text);
    after.push_str(&source[at..]);
    Ok(after)
}

pub fn apply(path: &Path, source: &str, waiver: &Waiver) -> Result<()> {
    crate::fix::write_atomically(path, &inserted(source, waiver)?)
}

/// The change as a unified diff, for `--dry-run`.
pub fn diff(display: &str, source: &str, waiver: &Waiver) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let index = waiver.line.saturating_sub(1) as usize;
    if index > lines.len() {
        bail!(
            "{display} has {} lines, so line {} is past its end",
            lines.len(),
            waiver.line
        );
    }
    let before = &lines[index.saturating_sub(CONTEXT)..index];
    let after = &lines[index..(index + CONTEXT).min(lines.len())];
    let first = index - before.len() + 1;
    let old_count = before.len() + after.len();
    let old_start = if old_count == 0 { 0 } else { first };

    let mut out = format!(
        "--- a/{display}\n+++ b/{display}\n@@ -{old_start},{old_count} +{first},{} @@\n",
        old_count + 1
    );
    for line in before {
        out.push_str(&format!(" {line}\n"));
    }
    out.push_str(&format!("+{}\n", waiver.text.trim_end_matches('\n')));
    for line in after {
        out.push_str(&format!(" {line}\n"));
    }
    Ok(out)
}

/// Byte offset of the start of a one-based line; the end of the text for the line
/// just past the last, which is where a waiver for the final line of a file with no
/// trailing newline would go.
fn line_start(source: &str, line: u32) -> Result<usize> {
    let wanted = line.saturating_sub(1) as usize;
    let mut at = 0;
    for _ in 0..wanted {
        match source[at..].find('\n') {
            Some(newline) => at += newline + 1,
            None if at < source.len() => at = source.len(),
            None => bail!("line {line} is past the end of the file"),
        }
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(line: u32, reason: &str) -> Request<'_> {
        Request {
            line,
            rule: "unused-export",
            reason,
            until: None,
            whole_file: false,
        }
    }

    #[test]
    fn the_waiver_goes_above_the_line_and_takes_its_indentation() {
        let source = "function a() {\n  const b = 1\n}\n";
        let waiver = plan(source, Language::TypeScript, &request(2, "kept")).expect("a waiver");
        assert_eq!(
            inserted(source, &waiver).expect("inserted"),
            "function a() {\n  // rqx-ignore unused-export: kept\n  const b = 1\n}\n"
        );
    }

    #[test]
    fn a_whole_file_waiver_goes_on_the_first_line() {
        let waiver = plan(
            "a = 1\n",
            Language::Python,
            &Request {
                whole_file: true,
                ..request(9, "generated")
            },
        )
        .expect("a waiver");
        assert_eq!(
            inserted("a = 1\n", &waiver).expect("inserted"),
            "# rqx-ignore-file unused-export: generated\na = 1\n"
        );
    }

    #[test]
    fn an_empty_reason_or_a_bad_date_is_refused() {
        assert!(plan("a\n", Language::Go, &request(1, "  ")).is_err());
        let late = Request {
            until: Some("soon"),
            ..request(1, "kept")
        };
        assert!(plan("a\n", Language::Go, &late).is_err());
    }

    #[test]
    fn a_file_that_uses_crlf_keeps_it() {
        let source = "a\r\nb\r\n";
        let waiver = plan(source, Language::Go, &request(2, "kept")).expect("a waiver");
        assert_eq!(
            inserted(source, &waiver).expect("inserted"),
            "a\r\n// rqx-ignore unused-export: kept\r\nb\r\n"
        );
    }

    #[test]
    fn a_line_past_the_end_is_refused() {
        let waiver = plan("a\n", Language::Go, &request(5, "kept")).expect("a waiver");
        assert!(inserted("a\n", &waiver).is_err());
        assert!(diff("a.go", "a\n", &waiver).is_err());
    }

    #[test]
    fn the_diff_shows_the_line_between_its_neighbours() {
        let source = "one\ntwo\nthree\nfour\nfive\n";
        let waiver = plan(source, Language::Go, &request(3, "kept")).expect("a waiver");
        assert_eq!(
            diff("x.go", source, &waiver).expect("a diff"),
            "--- a/x.go\n+++ b/x.go\n@@ -1,4 +1,5 @@\n one\n two\n+// rqx-ignore unused-export: kept\n three\n four\n"
        );
    }

    #[test]
    fn the_target_is_split_at_the_last_colon() {
        assert_eq!(
            parse_target("src/a.ts:42", false).expect("a target"),
            (PathBuf::from("src/a.ts"), 42)
        );
        assert_eq!(
            parse_target("C:\\work\\a.ts:7", false).expect("a target"),
            (PathBuf::from("C:\\work\\a.ts"), 7)
        );
    }

    #[test]
    fn a_file_alone_is_a_target_only_for_a_whole_file_waiver() {
        assert!(parse_target("src/a.ts", false).is_err());
        assert!(parse_target("src/a.ts:0", false).is_err());
        assert_eq!(
            parse_target("src/a.ts", true).expect("a target"),
            (PathBuf::from("src/a.ts"), 1)
        );
    }

    #[test]
    fn an_unknown_rule_names_the_one_it_was_close_to() {
        let error = require_rule("unused-exprt", &RqxConfig::default()).expect_err("unknown");
        assert!(format!("{error}").contains("`unused-export`"), "{error}");
    }
}
