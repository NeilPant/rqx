use std::io::Write;
use std::ops::Range;

use anyhow::Result;
use rqx_core::finding::{Evidence, Finding};
use rqx_core::{Cleanliness, LineIndex};

use crate::brand::{Ink, grade_ink, score_ink, severity_ink};
use crate::gate::Verdict;
use crate::preview::{self, HEAD_LINES, Line, Preview, Role};
use crate::report::escape::markup;
use crate::report::tally::{RuleCount, Tally};
use crate::report::{Report, plural};

/// Lines of source shown either side of a finding. The page is read at leisure, so it
/// carries the same two the terminal spends on `--detail full`.
const CONTEXT: u32 = 2;

type Rgb = (u8, u8, u8);

/// Every colour the page uses, named once and written as CSS variables so the dark
/// and the light theme cannot drift apart in what they define.
struct Theme {
    ground: Rgb,
    surface: Rgb,
    mark: Rgb,
    text: Rgb,
    muted: Rgb,
    accent: Rgb,
    danger: Rgb,
    warn: Rgb,
    good: Rgb,
    line: Rgb,
}

impl Theme {
    /// The brand palette itself, so the report is the same colours as the terminal.
    const DARK: Self = Self {
        ground: Ink::Ground.rgb(),
        surface: (0x1b, 0x16, 0x12),
        mark: (0x4a, 0x2d, 0x27),
        text: Ink::Text.rgb(),
        muted: Ink::Muted.rgb(),
        accent: Ink::Accent.rgb(),
        danger: Ink::Danger.rgb(),
        warn: Ink::Warn.rgb(),
        good: Ink::Good.rgb(),
        line: Ink::Line.rgb(),
    };

    /// The same hues darkened until text on a pale ground clears 4.5:1, which the
    /// brand inks, chosen for a dark terminal, do not.
    const LIGHT: Self = Self {
        ground: (0xfb, 0xf7, 0xf0),
        surface: (0xf1, 0xea, 0xdf),
        mark: (0xf6, 0xd5, 0xcf),
        text: (0x1f, 0x1a, 0x14),
        muted: (0x62, 0x54, 0x42),
        accent: (0x7a, 0x4f, 0x1e),
        danger: (0xb3, 0x26, 0x1e),
        warn: (0x85, 0x50, 0x0a),
        good: (0x3d, 0x6a, 0x1c),
        line: (0x8c, 0x7c, 0x62),
    };

    fn tokens(&self) -> [(&'static str, Rgb); 10] {
        [
            ("ground", self.ground),
            ("surface", self.surface),
            ("mark", self.mark),
            ("text", self.text),
            ("muted", self.muted),
            ("accent", self.accent),
            ("danger", self.danger),
            ("warn", self.warn),
            ("good", self.good),
            ("line", self.line),
        ]
    }

    fn variables(&self) -> String {
        self.tokens()
            .iter()
            .map(|(name, (r, g, b))| format!("--{name}:#{r:02x}{g:02x}{b:02x};"))
            .collect()
    }
}

/// Layout only; every colour comes from a variable. No script, no font, no image: the
/// file has to open from a CI artifact list with nothing else beside it.
const STYLE: &str = "\
*{box-sizing:border-box}\
body{margin:0;background:var(--ground);color:var(--text);font:16px/1.5 system-ui,-apple-system,'Segoe UI',sans-serif}\
header,main{max-width:64rem;margin:0 auto;padding:1rem}\
h1,h2,h3,h4{line-height:1.25}\
h1{margin:0 0 .25rem}\
h2{margin:2rem 0 .75rem;border-bottom:1px solid var(--line);padding-bottom:.25rem}\
a{color:var(--accent)}\
code,pre{font:.875rem/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;tab-size:4}\
table{border-collapse:collapse;width:100%}\
th,td{text-align:left;padding:.4rem .6rem;border-bottom:1px solid var(--line);vertical-align:top}\
th{color:var(--muted);font-weight:600}\
td.count,th.count{text-align:right;font-variant-numeric:tabular-nums}\
.muted{color:var(--muted)}\
.danger{color:var(--danger)}.warn{color:var(--warn)}.good{color:var(--good)}.accent{color:var(--accent)}\
.facts{display:flex;flex-wrap:wrap;gap:.5rem 2rem;margin:1rem 0}\
.facts div{min-width:6rem}\
.facts dt{color:var(--muted);font-size:.875rem}\
.facts dd{margin:0;font-size:1.25rem;font-weight:600}\
.gate{border:1px solid var(--line);border-left-width:.4rem;background:var(--surface);padding:.5rem 1rem;margin:1rem 0}\
.gate.failed{border-left-color:var(--danger)}.gate.passed{border-left-color:var(--good)}\
.gate ul{margin:.25rem 0}\
.bar{display:inline-block;height:.6rem;width:calc(var(--share)*1%);min-width:2px;background:var(--accent);vertical-align:middle}\
details.file{margin:1rem 0;border:1px solid var(--line)}\
details.file>summary{padding:.5rem 1rem;background:var(--surface);cursor:pointer}\
details.file>summary h3{display:inline;margin:0;font-size:1rem}\
.path{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;word-break:break-all}\
.finding{padding:.5rem 1rem 1rem;border-top:1px solid var(--line)}\
.finding h4{margin:.5rem 0;font-size:1rem}\
.sev{font-weight:700;text-transform:uppercase;font-size:.8rem;letter-spacing:.04em}\
.tag{border:1px solid var(--line);padding:0 .35rem;font-size:.8rem;color:var(--muted)}\
.frame{margin:.5rem 0;padding:.5rem 0;background:var(--surface);border:1px solid var(--line);overflow-x:auto}\
.frame code{display:block;min-width:max-content}\
.line{display:inline-block;width:100%;padding-right:1rem;color:var(--muted)}\
.line.hit{color:var(--text)}\
.ln{display:inline-block;width:4rem;padding-right:1rem;text-align:right;user-select:none}\
.line.hit .ln{font-weight:700}\
.gap{color:var(--muted);padding-left:4rem}\
mark{background:var(--mark);color:var(--text);text-decoration:underline wavy;text-underline-offset:.2em}\
.finding dl{margin:.5rem 0}\
.finding dt{color:var(--muted);float:left;clear:left;width:3.5rem}\
.finding dd{margin:0 0 .25rem 3.5rem}\
";

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let project = project_name(report);
    writeln!(out, "<!doctype html>\n<html lang=\"en\">")?;
    write_head(out, &project)?;
    writeln!(out, "<body>")?;
    write_header(out, report, &project)?;
    writeln!(out, "<main>")?;
    write_grades(out, report)?;
    write_tally(out, &Tally::of(&report.findings))?;
    write_files(out, report)?;
    write_waived(out, report)?;
    writeln!(out, "</main>\n</body>\n</html>")?;
    Ok(())
}

/// What the scanned directory is called, since `.` names nothing.
fn project_name(report: &Report<'_>) -> String {
    let root = &report.outcome.root;
    root.canonicalize()
        .ok()
        .and_then(|absolute| {
            absolute
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| root.display().to_string())
}

fn write_head(out: &mut dyn Write, project: &str) -> Result<()> {
    writeln!(
        out,
        "<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
    )?;
    writeln!(out, "<title>rqx report: {}</title>", markup(project))?;
    writeln!(
        out,
        "<style>:root{{color-scheme:dark;{}}}@media (prefers-color-scheme:light){{:root{{color-scheme:light;{}}}}}{STYLE}</style>\n</head>",
        Theme::DARK.variables(),
        Theme::LIGHT.variables()
    )?;
    Ok(())
}

fn write_header(out: &mut dyn Write, report: &Report<'_>, project: &str) -> Result<()> {
    let health = report.health();
    let grade = Cleanliness::of(health, report.totals.findings);
    writeln!(out, "<header>\n<h1>rqx report</h1>")?;
    writeln!(out, "<p class=\"muted\">{}", markup(project))?;
    if let Some(base) = report.base {
        writeln!(
            out,
            "&middot; changes against <code>{}</code>",
            markup(base)
        )?;
    }
    writeln!(out, "</p>\n<dl class=\"facts\">")?;
    write_fact(out, "Files", "", &report.files.len().to_string())?;
    write_fact(out, "Findings", "", &report.totals.findings.to_string())?;
    write_fact(
        out,
        "Health",
        ink_class(score_ink(health)),
        &format!("{health}/100"),
    )?;
    write_fact(out, "Grade", ink_class(grade_ink(grade)), grade.label())?;
    if let Some(verdict) = report.verdict {
        let (class, word) = if verdict.passed() {
            ("good", "passed")
        } else {
            ("danger", "failed")
        };
        write_fact(out, "Gate", class, word)?;
    }
    writeln!(out, "</dl>")?;
    if let Some(verdict) = report.verdict {
        write_verdict(out, verdict)?;
    }
    writeln!(out, "</header>")?;
    Ok(())
}

fn write_fact(out: &mut dyn Write, name: &str, class: &str, value: &str) -> Result<()> {
    writeln!(
        out,
        "<div><dt>{name}</dt><dd class=\"{class}\">{}</dd></div>",
        markup(value)
    )?;
    Ok(())
}

fn write_verdict(out: &mut dyn Write, verdict: &Verdict) -> Result<()> {
    if verdict.passed() {
        writeln!(
            out,
            "<div class=\"gate passed\" role=\"status\"><strong>Gate passed.</strong></div>"
        )?;
        return Ok(());
    }
    writeln!(
        out,
        "<div class=\"gate failed\" role=\"alert\"><strong>Gate failed.</strong>\n<ul>"
    )?;
    for failure in &verdict.failures {
        writeln!(out, "<li>{}</li>", markup(failure))?;
    }
    writeln!(out, "</ul></div>")?;
    Ok(())
}

fn write_grades(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    let grades = report.grades();
    let total = report.files.len() as u32;
    writeln!(
        out,
        "<section aria-labelledby=\"grades\">\n<h2 id=\"grades\">Grade distribution</h2>"
    )?;
    writeln!(
        out,
        "<table>\n<thead><tr><th scope=\"col\">Grade</th><th scope=\"col\" class=\"count\">Files</th><th scope=\"col\">Share</th></tr></thead>\n<tbody>"
    )?;
    for grade in Cleanliness::ALL {
        let count = grades.get(&grade).copied().unwrap_or(0);
        if count == 0 {
            continue;
        }
        writeln!(
            out,
            "<tr><th scope=\"row\" class=\"{}\">{}</th><td class=\"count\">{count}</td><td>{}</td></tr>",
            ink_class(grade_ink(grade)),
            grade.label(),
            bar(count, total)
        )?;
    }
    writeln!(out, "</tbody>\n</table>\n</section>")?;
    Ok(())
}

fn write_tally(out: &mut dyn Write, tally: &Tally) -> Result<()> {
    let total: u32 = tally.families.iter().map(|(_, count)| count).sum();
    writeln!(
        out,
        "<section aria-labelledby=\"families\">\n<h2 id=\"families\">Findings by family</h2>"
    )?;
    writeln!(
        out,
        "<table>\n<thead><tr><th scope=\"col\">Family</th><th scope=\"col\" class=\"count\">Findings</th><th scope=\"col\">Share</th></tr></thead>\n<tbody>"
    )?;
    for (family, count) in &tally.families {
        writeln!(
            out,
            "<tr><th scope=\"row\">{}</th><td class=\"count\">{count}</td><td>{}</td></tr>",
            markup(family),
            bar(*count, total)
        )?;
    }
    writeln!(out, "</tbody>\n</table>\n</section>")?;

    writeln!(
        out,
        "<section aria-labelledby=\"rules\">\n<h2 id=\"rules\">Findings by rule</h2>"
    )?;
    writeln!(
        out,
        "<table>\n<thead><tr><th scope=\"col\">Rule</th><th scope=\"col\">Severity</th><th scope=\"col\" class=\"count\">Findings</th><th scope=\"col\">Share</th></tr></thead>\n<tbody>"
    )?;
    for RuleCount {
        slug,
        count,
        severity,
    } in &tally.rules
    {
        writeln!(
            out,
            "<tr><th scope=\"row\"><code>{}</code></th><td class=\"{}\">{}</td><td class=\"count\">{count}</td><td>{}</td></tr>",
            markup(slug),
            ink_class(severity_ink(*severity)),
            severity.label(),
            bar(*count, total)
        )?;
    }
    writeln!(out, "</tbody>\n</table>\n</section>")?;
    Ok(())
}

fn write_files(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    writeln!(
        out,
        "<section aria-labelledby=\"files\">\n<h2 id=\"files\">Files, worst first</h2>"
    )?;
    let shown = report.shown_by_file();
    let prefix = report.path_prefix();
    let mut listed = 0;
    for (file, grade) in report.priority() {
        let Some(findings) = shown.get(file) else {
            continue;
        };
        listed += 1;
        write_file(out, report, &prefix, file, grade, findings)?;
    }
    if listed == 0 {
        writeln!(out, "<p>No findings.</p>")?;
    }
    writeln!(out, "</section>")?;
    Ok(())
}

fn write_file(
    out: &mut dyn Write,
    report: &Report<'_>,
    prefix: &str,
    file: &str,
    grade: Cleanliness,
    findings: &[&Finding],
) -> Result<()> {
    writeln!(
        out,
        "<details class=\"file\" open>\n<summary><h3><span class=\"path\">{}</span> <span class=\"{}\">{}</span> <span class=\"muted\">&middot; {}</span></h3></summary>",
        markup(&format!("{prefix}{file}")),
        ink_class(grade_ink(grade)),
        grade.label(),
        plural(findings.len() as u32, "finding", "findings")
    )?;
    let source = std::fs::read_to_string(report.outcome.root.join(file)).ok();
    let index = source.as_deref().map(LineIndex::new);
    let mut ordered = findings.to_vec();
    ordered.sort_by_key(|finding| finding.location.start);
    for finding in ordered {
        write_finding(out, finding, index.as_ref())?;
    }
    writeln!(out, "</details>")?;
    Ok(())
}

fn write_finding(
    out: &mut dyn Write,
    finding: &Finding,
    index: Option<&LineIndex<'_>>,
) -> Result<()> {
    let spec = finding.rule.spec();
    let known = if finding.known {
        " <span class=\"tag\">known</span>"
    } else {
        ""
    };
    writeln!(
        out,
        "<article class=\"finding\">\n<h4><span class=\"sev {}\">{}</span> <code>{}</code> <span class=\"muted\">{}:{}</span>{known}</h4>",
        ink_class(severity_ink(finding.severity)),
        finding.severity.label(),
        markup(finding.slug()),
        finding.location.line,
        finding.location.column
    )?;
    if let Some(view) = index.and_then(|index| preview::of(index, &finding.location, CONTEXT)) {
        write_frame(out, &view)?;
    }
    writeln!(
        out,
        "<p>{}</p>\n<dl>",
        markup(&finding.evidence.to_string())
    )?;
    if !matches!(finding.evidence, Evidence::Custom { .. }) {
        writeln!(out, "<dt>Why</dt><dd>{}</dd>", markup(spec.why))?;
    }
    if let Some(fix) = fix_text(finding) {
        writeln!(out, "<dt>Fix</dt><dd>{}</dd>", markup(&fix))?;
    }
    for site in &finding.related {
        writeln!(
            out,
            "<dt>Also at</dt><dd>{}</dd>",
            markup(&site.to_string())
        )?;
    }
    writeln!(out, "</dl>\n</article>")?;
    Ok(())
}

/// The refactorings the rule prescribes, then what to do in this place.
fn fix_text(finding: &Finding) -> Option<String> {
    let techniques = finding
        .rule
        .spec()
        .fixes
        .iter()
        .map(|fix| fix.name())
        .collect::<Vec<_>>()
        .join(" / ");
    match (techniques.is_empty(), finding.fix_hint.is_empty()) {
        (true, true) => None,
        (false, true) => Some(techniques),
        (true, false) => Some(finding.fix_hint.clone()),
        (false, false) => Some(format!("{techniques} \u{2014} {}", finding.fix_hint)),
    }
}

fn write_frame(out: &mut dyn Write, view: &Preview) -> Result<()> {
    write!(out, "<pre class=\"frame\"><code>")?;
    let leading_context = view
        .lines
        .iter()
        .take_while(|line| line.role == Role::Context)
        .count();
    for (position, line) in view.lines.iter().enumerate() {
        if view.elided > 0 && position == leading_context + HEAD_LINES {
            writeln!(
                out,
                "<span class=\"gap\">\u{22ee} {} more lines</span>",
                view.elided
            )?;
        }
        write_line(out, line)?;
    }
    writeln!(out, "</code></pre>")?;
    Ok(())
}

fn write_line(out: &mut dyn Write, line: &Line) -> Result<()> {
    let class = match line.role {
        Role::Offence => "line hit",
        Role::Context => "line",
    };
    writeln!(
        out,
        "<span class=\"{class}\"><span class=\"ln\">{}</span>{}</span>",
        line.number,
        highlighted(&line.text, line.mark.as_ref())
    )?;
    Ok(())
}

/// The marked columns underlined in place, so the page keeps what the terminal's caret
/// row says even where a viewer drops background colour.
fn highlighted(text: &str, mark: Option<&Range<usize>>) -> String {
    let parts = mark.and_then(|mark| {
        Some((
            text.get(..mark.start)?,
            text.get(mark.clone())?,
            text.get(mark.end..)?,
        ))
    });
    match parts {
        Some((before, marked, after)) => {
            format!(
                "{}<mark>{}</mark>{}",
                markup(before),
                markup(marked),
                markup(after)
            )
        }
        None => markup(text),
    }
}

fn write_waived(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    let waived = &report.outcome.suppressed;
    if waived.is_empty() {
        return Ok(());
    }
    let prefix = report.path_prefix();
    writeln!(
        out,
        "<section aria-labelledby=\"waived\">\n<h2 id=\"waived\">{}</h2>",
        plural(waived.len() as u32, "waived finding", "waived findings")
    )?;
    writeln!(
        out,
        "<table>\n<thead><tr><th scope=\"col\">Rule</th><th scope=\"col\">Location</th><th scope=\"col\">Reason</th><th scope=\"col\">Until</th></tr></thead>\n<tbody>"
    )?;
    for one in waived {
        writeln!(
            out,
            "<tr><td><code>{}</code></td><td class=\"path\">{}</td><td>{}</td><td>{}</td></tr>",
            markup(one.finding.slug()),
            markup(&format!(
                "{prefix}{}:{}",
                one.finding.location.file, one.finding.location.line
            )),
            markup(one.reason.as_deref().unwrap_or("")),
            markup(one.until.as_deref().unwrap_or(""))
        )?;
    }
    writeln!(out, "</tbody>\n</table>\n</section>")?;
    Ok(())
}

/// Proportional to the whole, drawn by the stylesheet from a number the markup keeps,
/// and hidden from a screen reader because the count beside it already says it.
fn bar(count: u32, total: u32) -> String {
    let share = (u64::from(count) * 100)
        .checked_div(u64::from(total))
        .unwrap_or(0)
        .max(u64::from(count > 0));
    format!("<span class=\"bar\" style=\"--share:{share}\" aria-hidden=\"true\"></span>")
}

const fn ink_class(ink: Ink) -> &'static str {
    match ink {
        Ink::Danger => "danger",
        Ink::Warn => "warn",
        Ink::Good => "good",
        Ink::Accent => "accent",
        _ => "muted",
    }
}

#[cfg(test)]
mod tests {
    use rqx_core::finding::Severity;
    use rqx_core::rule::RuleId;

    use super::*;
    use crate::brand::tests::contrast;
    use crate::report::FindingFilter;
    use crate::report::tests::{outcome_of, outcome_with};

    fn rendered(report: &Report<'_>) -> String {
        let mut out = Vec::new();
        render(report, &mut out).expect("a page");
        String::from_utf8(out).expect("utf-8")
    }

    fn page_of(outcome: &crate::scan::ScanOutcome) -> String {
        rendered(&Report::scan(outcome, &FindingFilter::default()))
    }

    /// Elements that never close, and the one place text is not markup. Everything
    /// else has to nest, or a browser quietly repairs the page into something else.
    const VOID: [&str; 3] = ["meta", "br", "hr"];

    fn assert_well_formed(page: &str) {
        let mut open: Vec<String> = Vec::new();
        let mut rest = page;
        while let Some(start) = rest.find('<') {
            rest = &rest[start + 1..];
            let end = rest.find('>').expect("a tag that ends");
            let tag = &rest[..end];
            rest = &rest[end + 1..];
            if tag.starts_with('!') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                assert_eq!(open.pop().as_deref(), Some(name), "unbalanced </{name}>");
                continue;
            }
            let name = tag.split([' ', '/']).next().unwrap_or_default();
            if VOID.contains(&name) {
                continue;
            }
            if name == "style" {
                let close = rest.find("</style>").expect("a closed style");
                rest = &rest[close + "</style>".len()..];
                continue;
            }
            open.push(name.to_owned());
        }
        assert!(open.is_empty(), "never closed: {open:?}");
    }

    #[test]
    fn source_text_cannot_become_markup() {
        let dir = std::env::temp_dir().join(format!("rqx-html-escape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("<b>.ts"),
            "const a = '<script>alert(1)</script>'\nconst b = 1\n",
        )
        .unwrap();
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "<b>.ts")]);
        outcome.root = dir.clone();
        outcome.findings[0].fix_hint = "<img src=x onerror=alert(1)>".to_owned();

        let page = page_of(&outcome);
        std::fs::remove_dir_all(&dir).ok();
        assert!(!page.contains("<script"), "{page}");
        assert!(!page.contains("<img"), "{page}");
        assert!(!page.contains("<b>.ts"), "{page}");
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(page.contains("&lt;b&gt;.ts"));
        assert_well_formed(&page);
    }

    #[test]
    fn the_page_is_well_formed_and_self_contained() {
        let outcome = outcome_of(&[
            (RuleId::LongFunction, "a.ts"),
            (RuleId::MagicNumber, "b.ts"),
        ]);
        let page = page_of(&outcome);
        assert!(page.starts_with("<!doctype html>"));
        assert_well_formed(&page);
        for reference in ["http://", "https://", "<link", "<script", "@import", "url("] {
            assert!(!page.contains(reference), "references {reference}");
        }
        for heading in [
            "Grade distribution",
            "Findings by family",
            "Findings by rule",
        ] {
            assert!(page.contains(heading), "missing {heading}");
        }
        assert!(page.contains("<th scope=\"col\">Rule</th>"));
    }

    #[test]
    fn files_are_listed_worst_grade_first() {
        let mut outcome = outcome_with(&[("light.ts", Severity::Info)], 1);
        let heavy = outcome_of(&[
            (RuleId::LongFunction, "heavy.ts"),
            (RuleId::LongFunction, "heavy.ts"),
            (RuleId::LongFunction, "heavy.ts"),
        ]);
        for finding in heavy.findings {
            outcome.counts.record(&finding);
            *outcome
                .findings_by_file
                .entry(finding.location.file.clone())
                .or_default() += 1;
            outcome.analyzed.insert("heavy.ts".to_owned());
            outcome.findings.push(finding);
        }
        let page = page_of(&outcome);
        let heavy = page.find("heavy.ts").expect("the heavy file");
        let light = page.find("light.ts").expect("the light file");
        assert!(heavy < light, "the worse file is not first");
    }

    #[test]
    fn a_gated_scan_shows_its_verdict() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let report = Report::scan(&outcome, &FindingFilter::default());

        let failed = Verdict {
            failures: vec!["1 error <over> the limit".to_owned()],
            counted: 1,
            known: 0,
        };
        let page = rendered(&report.judged(&failed));
        assert!(page.contains("Gate failed."), "{page}");
        assert!(page.contains("1 error &lt;over&gt; the limit"));

        let report = Report::scan(&outcome, &FindingFilter::default());
        let passed = Verdict {
            failures: Vec::new(),
            counted: 0,
            known: 0,
        };
        assert!(rendered(&report.judged(&passed)).contains("Gate passed."));

        assert!(!page_of(&outcome).contains("Gate"));
    }

    #[test]
    fn a_waived_finding_is_listed_apart_with_its_reason() {
        let mut outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let finding = outcome.findings[0].clone();
        outcome.suppressed.push(rqx_core::suppress::Suppressed {
            at: finding.location.clone(),
            finding,
            reason: Some("legacy <import>".to_owned()),
            until: Some("2030-01-01".to_owned()),
        });
        let page = page_of(&outcome);
        assert!(page.contains("1 waived finding"));
        assert!(page.contains("legacy &lt;import&gt;"));
        assert!(page.contains("2030-01-01"));
        assert_well_formed(&page);
    }

    #[test]
    fn a_long_span_elides_its_middle_where_the_terminal_does() {
        let dir = std::env::temp_dir().join(format!("rqx-html-gap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        std::fs::write(dir.join("long.ts"), &source).unwrap();
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "long.ts")]);
        outcome.root = dir.clone();
        outcome.findings[0].location.end_line = 30;
        outcome.findings[0].location.end = source.len() as u32;

        let page = page_of(&outcome);
        std::fs::remove_dir_all(&dir).ok();
        assert!(page.contains("more lines</span>"), "{page}");
        assert_well_formed(&page);
    }

    /// Text is read on the ground and on the raised surface a code frame sits on, and
    /// the highlight carries text of its own.
    #[test]
    fn both_themes_clear_the_contrast_floor() {
        for (name, theme) in [("dark", Theme::DARK), ("light", Theme::LIGHT)] {
            for (ink, colour) in [
                ("text", theme.text),
                ("muted", theme.muted),
                ("accent", theme.accent),
                ("danger", theme.danger),
                ("warn", theme.warn),
                ("good", theme.good),
            ] {
                for (ground, base) in [("ground", theme.ground), ("surface", theme.surface)] {
                    let ratio = contrast(colour, base);
                    assert!(ratio >= 4.5, "{name} {ink} on {ground} is {ratio:.2}:1");
                }
            }
            let marked = contrast(theme.text, theme.mark);
            assert!(
                marked >= 4.5,
                "{name} text on the highlight is {marked:.2}:1"
            );
            let border = contrast(theme.line, theme.ground);
            assert!(border >= 3.0, "{name} border is {border:.2}:1");
        }
    }
}
