//! Findings other tools wrote, read from their SARIF 2.1.0 output.
//!
//! A result becomes a finding in the `external` family, so a baseline, the gate and
//! every reporter treat it like any other finding. Only the parts of SARIF that place
//! and describe a result are read; every other field is ignored.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rqx_core::finding::{Confidence, Evidence, Finding, Severity, SourceRange};
use rqx_core::line_index::LineStarts;
use rqx_core::rule::{Category, RuleId};
use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Document {
    runs: Vec<Run>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Run {
    tool: Tool,
    results: Vec<Entry>,
    original_uri_base_ids: HashMap<String, Artifact>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Tool {
    driver: Driver,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Driver {
    name: Option<String>,
    rules: Vec<Descriptor>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Descriptor {
    id: Option<String>,
    help: Option<Text>,
    help_uri: Option<String>,
    default_configuration: Option<Configuration>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Configuration {
    level: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Entry {
    rule_id: Option<String>,
    rule_index: Option<usize>,
    rule: Option<Reference>,
    level: Option<String>,
    message: Option<Text>,
    locations: Vec<Location>,
    suppressions: Vec<serde_json::Value>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Reference {
    id: Option<String>,
    index: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Text {
    text: Option<String>,
    markdown: Option<String>,
}

impl Text {
    fn one_line(&self) -> Option<String> {
        let text = self.text.as_deref().or(self.markdown.as_deref())?;
        let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!joined.is_empty()).then_some(joined)
    }
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Location {
    physical_location: Option<Physical>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Physical {
    artifact_location: Option<Artifact>,
    region: Option<Region>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Artifact {
    uri: Option<String>,
    uri_base_id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Region {
    start_line: Option<u32>,
    start_column: Option<u32>,
    end_line: Option<u32>,
    end_column: Option<u32>,
}

/// Every result from every file named, as findings. A file that cannot be read or is
/// not SARIF is reported on stderr and skipped, because a scan that failed on another
/// tool's missing output would be a scan nobody runs.
pub fn read_all(root: &Path, names: &[String]) -> Vec<Finding> {
    let mut sources = Sources::new(root);
    let mut findings = Vec::new();
    for name in names {
        match read(&root.join(name), &mut sources) {
            Ok(found) => findings.extend(found),
            Err(reason) => eprintln!("rqx: skipping SARIF input {name} \u{2014} {reason:#}"),
        }
    }
    findings
}

fn read(path: &Path, sources: &mut Sources) -> Result<Vec<Finding>> {
    let body =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let document: Document = serde_json::from_str(&body).context("not a SARIF document")?;
    Ok(document
        .runs
        .iter()
        .flat_map(|run| run_findings(run, sources))
        .collect())
}

fn run_findings(run: &Run, sources: &mut Sources) -> Vec<Finding> {
    let tool = tool_name(run.tool.driver.name.as_deref());
    run.results
        .iter()
        .filter(|entry| entry.suppressions.is_empty())
        .filter_map(|entry| finding_of(run, &tool, entry, sources))
        .collect()
}

/// `Semgrep OSS` and `semgrep` are the same tool to whoever reads a rule id.
fn tool_name(driver: Option<&str>) -> String {
    let word: String = driver
        .and_then(|name| name.split_whitespace().next())
        .unwrap_or_default()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect();
    if word.is_empty() {
        "external".to_owned()
    } else {
        word
    }
}

fn finding_of(run: &Run, tool: &str, entry: &Entry, sources: &mut Sources) -> Option<Finding> {
    let descriptor = descriptor_of(run, entry);
    let id = entry
        .rule_id
        .as_deref()
        .or_else(|| entry.rule.as_ref().and_then(|rule| rule.id.as_deref()))
        .or_else(|| descriptor.and_then(|rule| rule.id.as_deref()))
        .unwrap_or("unknown");
    let id: Arc<str> = format!("{tool}/{id}").into();

    let physical = entry.locations.first()?.physical_location.as_ref()?;
    let location = sources.range_of(run, physical)?;
    let message = entry
        .message
        .as_ref()
        .and_then(Text::one_line)
        .unwrap_or_else(|| id.to_string());

    Some(Finding {
        rule: RuleId::Custom,
        location,
        severity: severity_of(
            entry
                .level
                .as_deref()
                .or_else(|| descriptor?.default_configuration.as_ref()?.level.as_deref()),
        ),
        confidence: Confidence::Verified,
        evidence: Evidence::Custom {
            id,
            message,
            family: Category::External,
        },
        fix_hint: descriptor.map(guidance).unwrap_or_default(),
        related: Vec::new(),
        known: false,
        focus: None,
    })
}

fn descriptor_of<'run>(run: &'run Run, entry: &Entry) -> Option<&'run Descriptor> {
    let index = entry
        .rule_index
        .or_else(|| entry.rule.as_ref().and_then(|rule| rule.index))?;
    run.tool.driver.rules.get(index)
}

fn guidance(descriptor: &Descriptor) -> String {
    descriptor
        .help
        .as_ref()
        .and_then(Text::one_line)
        .or_else(|| descriptor.help_uri.as_ref().map(|uri| format!("see {uri}")))
        .unwrap_or_default()
}

/// SARIF defaults an absent level to `warning`; `none` and `note` both mean the tool
/// is informing rather than objecting.
fn severity_of(level: Option<&str>) -> Severity {
    match level {
        Some("error") => Severity::Error,
        Some("note" | "none") => Severity::Info,
        _ => Severity::Warning,
    }
}

/// The project's files as text, read once each, so a run with a thousand results in
/// forty files reads forty files.
struct Sources {
    root: PathBuf,
    canonical_root: PathBuf,
    texts: HashMap<String, Option<(String, LineStarts)>>,
}

impl Sources {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            canonical_root: rqx_core::paths::canonicalize(root).unwrap_or_else(|_| root.into()),
            texts: HashMap::new(),
        }
    }

    fn range_of(&mut self, run: &Run, physical: &Physical) -> Option<SourceRange> {
        let artifact = physical.artifact_location.as_ref()?;
        let file = self.project_path(run, artifact)?;
        let region = physical.region.as_ref();
        let (text, starts) = self.text_of(&file)?;

        let start_line = region
            .and_then(|region| region.start_line)
            .unwrap_or(1)
            .max(1);
        let end_line = region
            .and_then(|region| region.end_line)
            .unwrap_or(start_line)
            .max(start_line);
        let start = offset(
            text,
            starts,
            start_line,
            region.and_then(|region| region.start_column),
        );
        let end = offset(
            text,
            starts,
            end_line,
            region.and_then(|region| region.end_column),
        )
        .max(start);
        let (line, column) = starts.locate(text, start);
        Some(SourceRange {
            file: Arc::from(file.as_str()),
            start,
            end,
            line,
            column,
            end_line,
        })
    }

    fn text_of(&mut self, file: &str) -> Option<(&str, &LineStarts)> {
        let root = &self.root;
        self.texts
            .entry(file.to_owned())
            .or_insert_with(|| {
                let text = std::fs::read_to_string(root.join(file)).ok()?;
                let starts = LineStarts::new(&text);
                Some((text, starts))
            })
            .as_ref()
            .map(|(text, starts)| (text.as_str(), starts))
    }

    /// The path as findings name files: relative to the scan root, with `/`. `None`
    /// for a result in a file outside it.
    fn project_path(&self, run: &Run, artifact: &Artifact) -> Option<String> {
        let uri = artifact.uri.as_deref()?;
        let base = artifact
            .uri_base_id
            .as_ref()
            .and_then(|id| run.original_uri_base_ids.get(id))
            .and_then(|base| base.uri.as_deref());
        let path = reference_path(uri, base);
        if is_absolute(&path) {
            return self.beneath_root(Path::new(&path));
        }
        clean(Path::new(&path))
    }

    fn beneath_root(&self, absolute: &Path) -> Option<String> {
        let canonical = rqx_core::paths::canonicalize(absolute).ok();
        [Some(absolute), canonical.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|candidate| {
                [&self.root, &self.canonical_root]
                    .into_iter()
                    .find_map(|root| candidate.strip_prefix(root).ok())
            })
            .and_then(clean)
    }
}

/// The byte offset of a one-based line and column. A column counts characters, and
/// one past the end of the line, or no column at all for an end, means the line's end.
fn offset(text: &str, starts: &LineStarts, line: u32, column: Option<u32>) -> u32 {
    let begin = starts.line_start(line) as usize;
    let rest = &text[begin.min(text.len())..];
    let line_length = rest.find('\n').unwrap_or(rest.len());
    let within = match column {
        Some(column) => rest[..line_length]
            .char_indices()
            .nth(column.saturating_sub(1) as usize)
            .map_or(line_length, |(at, _)| at),
        None => line_length,
    };
    (begin + within) as u32
}

fn is_absolute(path: &str) -> bool {
    Path::new(path).is_absolute() || starts_with_drive(path)
}

/// `C:`, `C:/` or `C:\work`: a drive letter, which is absolute whatever platform reads it.
fn starts_with_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && bytes.get(2).is_none_or(|next| matches!(next, b'/' | b'\\'))
}

/// The path a reference names: absolute as written, or joined to the base the run
/// declares for it.
fn reference_path(uri: &str, base: Option<&str>) -> String {
    let path = uri_path(uri);
    match base {
        Some(base) if !is_absolute(&path) => {
            format!("{}/{path}", uri_path(base).trim_end_matches('/'))
        }
        _ => path,
    }
}

/// `file:///a/b%20c` as `/a/b c`; a relative reference is decoded and left as it is.
/// A drive path loses the slash the URI put before it, and a host other than this
/// machine makes a network path: `file://server/share/a` is `//server/share/a`.
fn uri_path(uri: &str) -> String {
    let Some(rest) = uri.strip_prefix("file://") else {
        return percent_decode(uri);
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    if starts_with_drive(authority) && authority.len() == 2 {
        // `file://C:/work` is not valid, but some tools write it, and it means `C:/work`.
        return percent_decode(rest);
    }
    let decoded = percent_decode(path);
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return format!("//{authority}{decoded}");
    }
    match decoded.strip_prefix('/') {
        Some(drive) if starts_with_drive(drive) => drive.to_owned(),
        _ => decoded,
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                at += 3;
            }
            None => {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// A relative path with `.` and `..` resolved, or `None` when it climbs out of the root.
fn clean(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "rqx-external-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        for (name, text) in files {
            let file = root.join(name);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
            std::fs::write(file, text).expect("a file");
        }
        root
    }

    fn findings_of(root: &Path, sarif: &str) -> Vec<Finding> {
        std::fs::write(root.join("in.sarif"), sarif).expect("a SARIF file");
        read_all(root, &["in.sarif".to_owned()])
    }

    const RUFF: &str = include_str!("../tests/sarif/ruff.sarif");
    const ESLINT: &str = include_str!("../tests/sarif/eslint.sarif");
    const SEMGREP: &str = include_str!("../tests/sarif/semgrep.sarif");

    /// A path as the path of a file URI: forward slashes and a leading slash, so a
    /// Windows drive reads as `/C:/work` and `file://` + this is `file:///C:/work`.
    fn uri_path_of(path: &Path) -> String {
        let text = path.to_string_lossy().replace('\\', "/");
        if text.starts_with('/') {
            text
        } else {
            format!("/{text}")
        }
    }

    /// The fixture with `{root}` filled in. It goes through a JSON value so that the
    /// root is escaped as JSON, whatever characters the platform puts in a path.
    fn at(root: &Path, fixture: &str) -> String {
        fn fill(value: &mut serde_json::Value, root: &str) {
            match value {
                serde_json::Value::String(text) => *text = text.replace("{root}", root),
                serde_json::Value::Array(items) => items.iter_mut().for_each(|v| fill(v, root)),
                serde_json::Value::Object(map) => map.values_mut().for_each(|v| fill(v, root)),
                _ => {}
            }
        }
        let mut document: serde_json::Value =
            serde_json::from_str(fixture).expect("a SARIF fixture");
        fill(&mut document, &uri_path_of(root));
        document.to_string()
    }

    #[test]
    fn ruff_results_use_the_tool_name_and_an_absolute_file_uri() {
        let root = project(&[("app/main.py", "import os\n\nvalue = 1\n")]);
        let found = findings_of(&root, &at(&root, RUFF));
        assert_eq!(found.len(), 1, "the result outside the root is skipped");
        let finding = &found[0];
        assert_eq!(finding.slug(), "ruff/F401");
        assert_eq!(&*finding.location.file, "app/main.py");
        assert_eq!((finding.location.line, finding.location.column), (1, 8));
        assert_eq!(finding.severity, Severity::Error);
        assert!(finding.is_external());
        assert_eq!(
            finding.evidence.to_string(),
            "`os` imported but unused",
            "the message is carried as written"
        );
    }

    #[test]
    fn eslint_levels_map_and_a_rule_index_names_the_rule() {
        let root = project(&[("src/a.js", "var unused = 1;\nconsole.log(2)\n")]);
        let found = findings_of(&root, &at(&root, ESLINT));
        let by_slug: Vec<(&str, Severity)> = found
            .iter()
            .map(|finding| (finding.slug(), finding.severity))
            .collect();
        assert_eq!(
            by_slug,
            vec![
                ("eslint/no-unused-vars", Severity::Error),
                ("eslint/no-console", Severity::Warning),
            ]
        );
    }

    #[test]
    fn semgrep_relative_uris_resolve_and_suppressed_results_are_dropped() {
        let root = project(&[(
            "service/handler.py",
            "import subprocess\nsubprocess.call(x, shell=True)\n",
        )]);
        let found = findings_of(&root, SEMGREP);
        assert_eq!(
            found.len(),
            1,
            "{:?}",
            found.iter().map(Finding::slug).collect::<Vec<_>>()
        );
        let finding = &found[0];
        assert_eq!(
            finding.slug(),
            "semgrep/python.lang.security.audit.subprocess-shell-true.subprocess-shell-true"
        );
        assert_eq!(&*finding.location.file, "service/handler.py");
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.location.line, 2);
        assert!(
            finding.fix_hint.contains("shell=True"),
            "{}",
            finding.fix_hint
        );
    }

    #[test]
    fn levels_map_to_severities() {
        assert_eq!(severity_of(Some("error")), Severity::Error);
        assert_eq!(severity_of(Some("warning")), Severity::Warning);
        assert_eq!(severity_of(Some("note")), Severity::Info);
        assert_eq!(severity_of(Some("none")), Severity::Info);
        assert_eq!(severity_of(None), Severity::Warning);
    }

    #[test]
    fn a_base_uri_declared_by_the_run_is_joined_to_a_relative_path() {
        let root = project(&[("pkg/a.py", "x = 1\n")]);
        let sarif = serde_json::json!({"runs": [{
            "tool": {"driver": {"name": "t"}},
            "originalUriBaseIds": {"ROOT": {"uri": format!("file://{}/", uri_path_of(&root))}},
            "results": [{
                "ruleId": "r",
                "message": {"text": "m"},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": "pkg/a.py", "uriBaseId": "ROOT"},
                    "region": {"startLine": 1}
                }}]
            }]
        }]})
        .to_string();
        let found = findings_of(&root, &sarif);
        assert_eq!(found.len(), 1);
        assert_eq!(&*found[0].location.file, "pkg/a.py");
    }

    #[test]
    fn a_path_that_climbs_out_of_the_root_is_skipped() {
        let root = project(&[("a.py", "x = 1\n")]);
        let sarif = r#"{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"t"}},"results":[
            {"ruleId":"r","message":{"text":"m"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"../outside.py"},"region":{"startLine":1}}}]},
            {"ruleId":"r","message":{"text":"m"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"./sub/../a.py"},"region":{"startLine":1}}}]}
        ]}]}"#;
        let found = findings_of(&root, sarif);
        assert_eq!(found.len(), 1);
        assert_eq!(&*found[0].location.file, "a.py");
    }

    #[test]
    fn unknown_fields_and_missing_regions_are_tolerated() {
        let root = project(&[("a.py", "x = 1\n")]);
        let sarif = r#"{"$schema":"x","version":"2.1.0","extra":{"a":[1,2]},"runs":[{"tool":{"driver":{"name":"Other Tool","informationUri":"u","rules":[{"id":"R1","defaultConfiguration":{"level":"error"},"properties":{"tags":["a"]}}]}},"artifacts":[],"results":[
            {"ruleId":"R1","ruleIndex":0,"message":{"text":"first\nsecond"},"partialFingerprints":{"a":"b"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"a.py"}}}]}
        ]}]}"#;
        let found = findings_of(&root, sarif);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slug(), "other/R1");
        assert_eq!(
            found[0].severity,
            Severity::Error,
            "the rule's default level applies"
        );
        assert_eq!(found[0].evidence.to_string(), "first second");
        assert_eq!(found[0].location.line, 1);
    }

    #[test]
    fn a_file_that_is_not_sarif_is_skipped_rather_than_fatal() {
        let root = project(&[("a.py", "x = 1\n")]);
        assert!(findings_of(&root, "not json at all").is_empty());
        assert!(read_all(&root, &["missing.sarif".to_owned()]).is_empty());
    }

    #[test]
    fn two_rules_on_identical_text_keep_distinct_identities() {
        let root = project(&[("a.py", "x = 1\n")]);
        let sarif = r#"{"runs":[{"tool":{"driver":{"name":"t"}},"results":[
            {"ruleId":"one","message":{"text":"m"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"a.py"},"region":{"startLine":1,"startColumn":1,"endColumn":2}}}]},
            {"ruleId":"two","message":{"text":"m"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"a.py"},"region":{"startLine":1,"startColumn":1,"endColumn":2}}}]}
        ]}]}"#;
        let found = findings_of(&root, sarif);
        let source = "x = 1\n";
        let identities: std::collections::BTreeSet<_> = found
            .iter()
            .map(|finding| rqx_core::Identity::resolved(finding, source))
            .collect();
        assert_eq!(identities.len(), 2);
    }

    #[test]
    fn percent_escapes_and_drive_letters_decode() {
        assert_eq!(uri_path("file:///work/my%20dir/a.py"), "/work/my dir/a.py");
        assert_eq!(uri_path("file:///C:/work/a.py"), "C:/work/a.py");
        assert_eq!(uri_path("src/a%2Bb.py"), "src/a+b.py");
    }

    #[test]
    fn a_windows_file_uri_names_the_drive_path() {
        for (uri, path) in [
            ("file:///C:/Users/me/proj/a.py", "C:/Users/me/proj/a.py"),
            ("file:///c:/Users/me/a.py", "c:/Users/me/a.py"),
            ("file:///C%3A/Users/me/a.py", "C:/Users/me/a.py"),
            ("file:///C:/My%20Work/a.py", "C:/My Work/a.py"),
            ("file://localhost/C:/work/a.py", "C:/work/a.py"),
            ("file://C:/work/a.py", "C:/work/a.py"),
            ("C:/work/a.py", "C:/work/a.py"),
            (r"C:\work\a.py", r"C:\work\a.py"),
        ] {
            assert_eq!(uri_path(uri), path, "{uri}");
            assert!(is_absolute(path), "{path} is absolute");
        }
    }

    #[test]
    fn a_host_in_a_file_uri_makes_a_network_path() {
        assert_eq!(uri_path("file://server/share/a.py"), "//server/share/a.py");
        assert_eq!(uri_path("file://LOCALHOST/work/a.py"), "/work/a.py");
        assert_eq!(uri_path("file:///work/a.py"), "/work/a.py");
    }

    #[test]
    fn only_a_letter_and_a_colon_make_a_drive() {
        assert!(starts_with_drive("C:"));
        assert!(starts_with_drive("c:/work"));
        assert!(starts_with_drive(r"D:\work"));
        assert!(!starts_with_drive("C:work"), "a drive-relative path");
        assert!(!starts_with_drive("1:/work"));
        assert!(!starts_with_drive("src/a.py"));
        assert!(!is_absolute("a:b.py"));
    }

    #[test]
    fn a_relative_reference_joins_the_base_with_a_drive() {
        assert_eq!(
            reference_path("pkg/a.py", Some("file:///C:/Users/me/proj/")),
            "C:/Users/me/proj/pkg/a.py"
        );
        assert_eq!(
            reference_path("pkg/a.py", Some("file:///work/")),
            "/work/pkg/a.py"
        );
        assert_eq!(
            reference_path("file:///C:/other/a.py", Some("file:///C:/proj/")),
            "C:/other/a.py",
            "an absolute reference ignores the base"
        );
        assert_eq!(reference_path("pkg/a.py", None), "pkg/a.py");
    }

    #[test]
    fn a_windows_root_is_escaped_into_the_fixture_as_json() {
        let root = Path::new(r"C:\Users\runner\proj");
        let document: Document =
            serde_json::from_str(&at(root, RUFF)).expect("backslashes do not break the JSON");
        let uri = document.runs[0].results[0].locations[0]
            .physical_location
            .as_ref()
            .and_then(|physical| physical.artifact_location.as_ref())
            .and_then(|artifact| artifact.uri.as_deref())
            .expect("a location uri");
        assert_eq!(uri, "file:///C:/Users/runner/proj/app/main.py");
        assert_eq!(uri_path(uri), "C:/Users/runner/proj/app/main.py");
    }
}
