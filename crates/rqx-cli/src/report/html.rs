use std::io::Write;

use anyhow::Result;
use ratatui::style::Color;
use rqx_core::finding::{Evidence, Finding};
use rqx_core::{Cleanliness, LineIndex};
use rqx_lang::highlight::{self, Kind, Span};

use crate::brand::{ColorDepth, Ink, Palette, grade_ink, mascot_ink, score_ink, severity_ink};
use crate::gate::Verdict;
use crate::preview::{self, HEAD_LINES, Line, Preview, Role};
use crate::report::escape::markup;
use crate::report::tally::{RuleCount, Tally};
use crate::report::terminal::summary_rows;
use crate::report::{Report, plural};
use crate::session::sprite::Sprite;

/// Lines of source shown either side of a finding. The page is read at leisure, so it
/// carries the same two the terminal spends on `--detail full`.
const CONTEXT: u32 = 2;

/// Files open when the page loads. The rest are one click away, so a scan of a
/// thousand files does not open as a thousand code frames.
const OPEN_FILES: usize = 10;

/// Cells in a block-character bar, the width `--stats` draws its own at.
const BAR_WIDTH: usize = 24;

const MASCOT: &str = include_str!("../../assets/mascot.txt");

type Rgb = (u8, u8, u8);

/// Every colour the page uses, named once and written as CSS variables so the screen
/// and the print theme cannot drift apart in what they define.
struct Theme {
    ground: Rgb,
    panel: Rgb,
    rule: Rgb,
    line: Rgb,
    text: Rgb,
    muted: Rgb,
    accent: Rgb,
    shade: Rgb,
    danger: Rgb,
    warn: Rgb,
    good: Rgb,
    kind: Rgb,
    tint: Rgb,
}

/// `percent` of `over` laid on `under`, which is how the offending lines get a ground
/// of their own without a colour that is not part of the palette.
const fn blend(under: Rgb, over: Rgb, percent: u16) -> Rgb {
    const fn channel(under: u8, over: u8, percent: u16) -> u8 {
        ((under as u16 * (100 - percent) + over as u16 * percent) / 100) as u8
    }
    (
        channel(under.0, over.0, percent),
        channel(under.1, over.1, percent),
        channel(under.2, over.2, percent),
    )
}

impl Theme {
    /// The brand palette itself, so the report is the same colours as the terminal.
    const DARK: Self = Self {
        ground: Ink::Ground.rgb(),
        panel: (0x17, 0x12, 0x0d),
        rule: (0x3a, 0x31, 0x27),
        line: Ink::Line.rgb(),
        text: Ink::Text.rgb(),
        muted: Ink::Muted.rgb(),
        accent: Ink::Accent.rgb(),
        shade: Ink::Shade.rgb(),
        danger: Ink::Danger.rgb(),
        warn: Ink::Warn.rgb(),
        good: Ink::Good.rgb(),
        kind: (0xe6, 0xcf, 0xae),
        tint: blend((0x17, 0x12, 0x0d), Ink::Accent.rgb(), TINT_PERCENT),
    };

    /// The same hues darkened until text on paper clears 4.5:1, which the brand inks,
    /// chosen for a dark terminal, do not.
    const LIGHT: Self = Self {
        ground: (0xfb, 0xf7, 0xf0),
        panel: (0xf1, 0xea, 0xdf),
        rule: (0xd8, 0xcc, 0xb8),
        line: (0x8c, 0x7c, 0x62),
        text: (0x1f, 0x1a, 0x14),
        muted: (0x62, 0x54, 0x42),
        accent: (0x7a, 0x4f, 0x1e),
        shade: (0xa8, 0x84, 0x5a),
        danger: (0xb3, 0x26, 0x1e),
        warn: (0x85, 0x50, 0x0a),
        good: (0x3d, 0x6a, 0x1c),
        kind: (0x5a, 0x46, 0x2c),
        tint: blend((0xf1, 0xea, 0xdf), (0x7a, 0x4f, 0x1e), TINT_PERCENT),
    };

    fn tokens(&self) -> [(&'static str, Rgb); 13] {
        [
            ("ground", self.ground),
            ("panel", self.panel),
            ("rule", self.rule),
            ("line", self.line),
            ("text", self.text),
            ("muted", self.muted),
            ("accent", self.accent),
            ("shade", self.shade),
            ("danger", self.danger),
            ("warn", self.warn),
            ("good", self.good),
            ("type", self.kind),
            ("tint", self.tint),
        ]
    }

    fn variables(&self) -> String {
        self.tokens()
            .iter()
            .map(|(name, colour)| format!("--{name}:{};", hex(*colour)))
            .collect()
    }
}

/// How much of the accent the offending lines are laid over the panel with: enough to
/// find them at a glance, little enough that every syntax colour still reads on them.
const TINT_PERCENT: u16 = 8;

fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Layout only; every colour comes from a variable. No script, no font, no image: the
/// file has to open from a CI artifact list with nothing else beside it.
const STYLE: &str = r#"
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--ground);color:var(--text);font:14px/1.55 var(--mono)}
.page{max-width:1040px;margin:0 auto;padding:32px}
h1,h2,h3,h4,p,ul,dl{margin:0}
h3,h4{font-size:inherit;font-weight:inherit}
h2{font-size:16px;font-weight:700;margin:40px 0 16px}
a{color:var(--accent)}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
:target{scroll-margin-top:16px}
table{border-collapse:collapse}
th{font-weight:400;text-align:left}
.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.skip{position:absolute;left:-9999px;top:8px;padding:8px 16px;background:var(--panel);border:1px solid var(--rule);border-radius:6px;z-index:1}
.skip:focus{left:16px}
.muted{color:var(--muted)}
.danger{color:var(--danger);--tone:var(--danger)}
.warn{color:var(--warn);--tone:var(--warn)}
.good{color:var(--good);--tone:var(--good)}
.accent{color:var(--accent);--tone:var(--accent)}
.muted{--tone:var(--muted)}
.strong{font-weight:700}
.hero{display:flex;flex-wrap:wrap;align-items:center;gap:24px;padding:24px;background:var(--panel);border:1px solid var(--rule);border-radius:10px}
.owl{flex:none;width:144px;height:144px}
.hero-text{flex:1 1 320px;min-width:0}
.sigil{color:var(--shade)}
.verdict{margin:8px 0;font-size:44px;line-height:1.15;font-weight:700}
.reasons{margin-top:16px;padding:0;list-style:none;max-width:80ch}
.reasons li::before{content:"- ";color:var(--muted)}
.toc ul{display:flex;flex-wrap:wrap;gap:0 16px;margin:16px 0 0;padding:0;list-style:none}
.toc li+li::before{content:"\00b7";margin-right:16px;color:var(--muted)}
.rows{display:grid;grid-template-columns:10ch minmax(0,1fr);column-gap:16px}
.rows dt{color:var(--muted)}
.rows dd{margin:0;overflow-wrap:anywhere}
section h3{margin:24px 0 8px;font-weight:700}
.scroll{overflow-x:auto}
.bars th{padding:0 16px 0 0;white-space:nowrap}
.bars td{padding:0 16px 0 0;white-space:nowrap}
.bars .count{text-align:right;font-variant-numeric:tabular-nums}
.trend{display:block;width:100%;max-width:640px;height:auto}
.trend-axis{stroke:var(--rule);stroke-width:1}
.trend-line{fill:none;stroke:var(--accent);stroke-width:2;stroke-linejoin:round}
.trend-dot{fill:var(--accent)}
.trend-label{fill:var(--muted);font-size:12px;font-family:inherit}
.fill{color:var(--accent)}
.rest{color:var(--rule)}
details.file{min-width:0;margin:0 0 8px;border:1px solid var(--rule);border-radius:10px}
summary{display:flex;flex-wrap:wrap;align-items:baseline;gap:0 16px;padding:8px 16px;cursor:pointer;list-style:none}
summary::-webkit-details-marker{display:none}
summary::before{content:"\25b8";color:var(--muted)}
details[open]>summary::before{content:"\25be"}
.badge{white-space:pre;font-weight:700;background:var(--tone);color:var(--ground)}
.path{overflow-wrap:anywhere}
.finding{min-width:0;padding:16px;border-top:1px solid var(--rule)}
.finding>*+*{margin-top:8px}
.finding p{max-width:80ch;overflow-wrap:anywhere}
.head{display:flex;flex-wrap:wrap;gap:0 16px}
.frame{max-width:100%;padding:8px 0;overflow-x:auto;background:var(--panel);border:1px solid var(--rule);border-radius:6px}
.rows-of-code{min-width:max-content}
.row{display:block;padding-right:16px;white-space:pre;tab-size:4}
.row .src{color:var(--muted)}
.row.hit{background:var(--tint)}
.row.hit .src{color:var(--text)}
.no{display:inline-block;vertical-align:top;min-width:5ch;padding-right:1ch;text-align:right;color:var(--muted);user-select:none}
.sep{padding-right:1ch;color:var(--rule);user-select:none}
.gap{color:var(--muted)}
.src .k{color:var(--accent)}
.src .s{color:var(--good)}
.src .n{color:var(--warn)}
.src .c{color:var(--muted);font-style:italic}
.src .t{color:var(--type)}
.src .f{color:var(--text);font-weight:700}
.to-top{padding:8px 16px;border-top:1px solid var(--rule);text-align:right}
.waived{color:var(--muted)}
.waived th,.waived td{padding:0 16px 0 0;vertical-align:top;overflow-wrap:anywhere}
.waived thead th{border-bottom:1px solid var(--rule)}
footer{margin-top:48px;font-size:12px;color:var(--muted)}
@media (max-width:640px){
.page{padding:16px}
.hero{padding:16px;gap:16px}
.owl{width:96px;height:96px}
.verdict{font-size:28px}
}
@media print{
.skip,.toc,.to-top{display:none}
.hero,.finding{break-inside:avoid}
.frame{overflow:visible}
.row{white-space:pre-wrap}
details::details-content{content-visibility:visible;display:block}
}
"#;

const FONTS: &str =
    r#"ui-monospace,"SF Mono","JetBrains Mono","Cascadia Code",Menlo,Consolas,monospace"#;

pub fn render(report: &Report<'_>, out: &mut dyn Write) -> Result<()> {
    let project = project_name(report);
    writeln!(out, "<!doctype html>\n<html lang=\"en\">")?;
    write_head(out, &project)?;
    writeln!(
        out,
        "<body>\n<a class=\"skip\" href=\"#files\">Skip to the files</a>\n<div class=\"page\">"
    )?;
    write_hero(out, report)?;
    write_contents(out, report)?;
    writeln!(out, "<main>")?;
    write_summary(out, report)?;
    write_owners(out, report)?;
    write_trend(out, report)?;
    write_tally(out, report)?;
    write_files(out, report)?;
    write_waived(out, report)?;
    writeln!(out, "</main>")?;
    write_footer(out, report, &project)?;
    writeln!(out, "</div>\n</body>\n</html>")?;
    Ok(())
}

/// Plain anchor links, so a long page can be crossed without a script.
fn write_contents(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    let mut links = vec![("summary", "Summary")];
    if report.outcome.owners.is_some() {
        links.push(("owners", "Owners"));
    }
    if report.trend.is_some() {
        links.push(("trend", "Trend"));
    }
    if !report.findings.is_empty() {
        links.extend([("families", "Families"), ("rules", "Rules")]);
    }
    links.push(("files", "Files"));
    if !report.outcome.suppressed.is_empty() {
        links.push(("waived", "Waived"));
    }
    writeln!(out, "<nav class=\"toc\" aria-label=\"Contents\">\n<ul>")?;
    for (id, name) in links {
        writeln!(out, "<li><a href=\"#{id}\">{name}</a></li>")?;
    }
    writeln!(out, "</ul>\n</nav>")?;
    Ok(())
}

/// What the scanned directory is called, since `.` names nothing.
fn project_name(report: &Report<'_>) -> String {
    let root = &report.outcome.root;
    rqx_core::paths::canonicalize(root)
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
        "<style>:root{{color-scheme:dark;--mono:{FONTS};{}}}@media print{{:root{{color-scheme:light;{}}}}}{STYLE}</style>\n</head>",
        Theme::DARK.variables(),
        Theme::LIGHT.variables()
    )?;
    Ok(())
}

/// The command that made this page, as it would be typed.
fn command_line(report: &Report<'_>) -> String {
    let root = report.outcome.root.display();
    match report.base {
        Some(base) => format!("rqx check {root} --base {base}"),
        None => format!("rqx scan {root}"),
    }
}

/// The session's header box as a page: the owl, wearing the colour the session gives
/// it for this health, beside the command and the answer it gave.
fn write_hero(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    let health = report.health();
    let grade = Cleanliness::of(health, report.totals.findings);
    writeln!(out, "<header id=\"top\" class=\"hero\">")?;
    writeln!(out, "{}", owl(mascot_ink(Some(health))))?;
    writeln!(out, "<div class=\"hero-text\">")?;
    writeln!(
        out,
        "<p class=\"muted\"><span class=\"sigil\">$</span> {}</p>",
        markup(&command_line(report))
    )?;
    writeln!(out, "{}", verdict_line(report.verdict, health))?;
    writeln!(
        out,
        "<p class=\"muted\">health {health}/100 \u{b7} grade {} \u{b7} {} \u{b7} {}</p>",
        grade.label(),
        plural(report.totals.findings, "finding", "findings"),
        plural(report.files.len() as u32, "file", "files"),
    )?;
    if let Some(verdict) = report.verdict.filter(|verdict| !verdict.passed()) {
        write_reasons(out, verdict)?;
    }
    writeln!(out, "</div>\n</header>")?;
    Ok(())
}

/// The largest thing on the page. The text variation selector keeps the cross a glyph
/// where a platform would otherwise draw it as a coloured emoji.
fn verdict_line(verdict: Option<&Verdict>, health: u32) -> String {
    let (class, words) = match verdict {
        Some(verdict) if verdict.passed() => ("good", "\u{2713} gate passed".to_owned()),
        Some(_) => ("danger", "\u{2716}\u{fe0e} gate failed".to_owned()),
        None => (ink_class(score_ink(health)), format!("health {health}/100")),
    };
    let role = match verdict {
        Some(verdict) if !verdict.passed() => "alert",
        _ => "status",
    };
    format!("<h1 class=\"verdict {class}\" role=\"{role}\">{words}</h1>")
}

fn write_reasons(out: &mut dyn Write, verdict: &Verdict) -> Result<()> {
    writeln!(out, "<ul class=\"reasons\">")?;
    for failure in &verdict.failures {
        writeln!(out, "<li>{}</li>", markup(failure))?;
    }
    writeln!(out, "</ul>")?;
    Ok(())
}

/// The owl's idle frame, one rectangle per run of a colour. The plumage and its shade
/// follow `signal` exactly as they do in the terminal; the eyes never do.
fn owl(signal: Ink) -> String {
    let sprite = Sprite::parse(MASCOT);
    let palette = Palette::with_depth(ColorDepth::Truecolor);
    let rows = sprite.pixels(sprite.frame("idle"));
    let mut shapes = String::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, width, ink) in runs_of(row) {
            let Color::Rgb(r, g, b) = palette.art_color(ink, signal) else {
                continue;
            };
            shapes.push_str(&format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"1\" fill=\"{}\"/>",
                hex((r, g, b))
            ));
        }
    }
    format!(
        "<svg class=\"owl\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\" role=\"img\" aria-label=\"The rqx owl\">{shapes}</svg>",
        sprite.width(),
        rows.len()
    )
}

/// Neighbouring pixels of one ink as `(start, length, ink)`.
fn runs_of(row: &[Option<Ink>]) -> Vec<(usize, usize, Ink)> {
    let mut runs: Vec<(usize, usize, Ink)> = Vec::new();
    for (x, pixel) in row.iter().enumerate() {
        let Some(ink) = *pixel else { continue };
        match runs.last_mut() {
            Some((start, length, last)) if *last == ink && *start + *length == x => *length += 1,
            _ => runs.push((x, 1, ink)),
        }
    }
    runs
}

/// What the terminal prints at the foot of a run, as aligned rows, then the counts that
/// terminal prints as bars under `--stats`.
fn write_summary(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    writeln!(
        out,
        "<section id=\"summary\" aria-labelledby=\"summary-title\">\n<h2 id=\"summary-title\">Summary</h2>\n<dl class=\"rows\">"
    )?;
    for row in summary_rows(report, report.totals_by_file().len()) {
        let class = row
            .ink
            .map_or(String::new(), |ink| format!("strong {}", ink_class(ink)));
        writeln!(
            out,
            "<dt>{}</dt><dd class=\"{class}\">{}</dd>",
            row.label,
            markup(&row.value)
        )?;
    }
    writeln!(out, "</dl>")?;

    let grades = report.grades();
    let files = report.files.len() as u32;
    writeln!(out, "<h3 id=\"grades-title\">By grade</h3>")?;
    write_bars(
        out,
        "grades-title",
        files,
        Cleanliness::ALL.iter().filter_map(|grade| {
            let count = grades.get(grade).copied().unwrap_or(0);
            (count > 0).then(|| {
                (
                    grade.label().to_owned(),
                    ink_class(grade_ink(*grade)),
                    count,
                )
            })
        }),
    )?;
    writeln!(out, "</section>")?;
    Ok(())
}

/// Each owner's health, in a section of its own right after the summary. Absent when
/// the project has no CODEOWNERS file, so a page for such a project is unchanged.
fn write_owners(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    if report.outcome.owners.is_none() {
        return Ok(());
    }
    writeln!(
        out,
        "<section id=\"owners\" aria-labelledby=\"owners-title\">\n<h2 id=\"owners-title\">Owners</h2>"
    )?;
    let rows = report.owner_rows();
    if rows.is_empty() {
        writeln!(
            out,
            "<p class=\"muted\">No CODEOWNERS rules apply to a scanned file.</p>"
        )?;
    } else {
        writeln!(
            out,
            "<div class=\"scroll\"><table class=\"bars\" aria-labelledby=\"owners-title\">\n<thead><tr><th scope=\"col\" class=\"muted\">Owner</th><th scope=\"col\" class=\"muted count\">Files</th><th scope=\"col\" class=\"muted count\">Findings</th><th scope=\"col\" class=\"muted count\">Health</th><th scope=\"col\" class=\"muted\">Grade</th></tr></thead>\n<tbody>"
        )?;
        for row in rows {
            writeln!(
                out,
                "<tr><th scope=\"row\">{}</th><td class=\"count\">{}</td><td class=\"count\">{}</td><td class=\"count {}\">{}</td><td class=\"{}\">{}</td></tr>",
                markup(&row.owner),
                row.files,
                row.findings,
                ink_class(score_ink(row.health)),
                row.health,
                ink_class(grade_ink(row.grade)),
                row.grade.label()
            )?;
        }
        writeln!(out, "</tbody>\n</table></div>")?;
    }
    writeln!(out, "</section>")?;
    Ok(())
}

/// The health score over the recorded snapshots, when `--trend` named a file. A file
/// with fewer than two snapshots has no line to draw, and the section says so rather
/// than vanishing.
fn write_trend(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    let Some(snapshots) = report.trend else {
        return Ok(());
    };
    writeln!(
        out,
        "<section id=\"trend\" aria-labelledby=\"trend-heading\">\n<h2 id=\"trend-heading\">Trend</h2>"
    )?;
    if snapshots.len() < 2 {
        writeln!(
            out,
            "<p class=\"muted\">A chart needs two snapshots; <code>rqx scan --record</code> adds one.</p>"
        )?;
    } else {
        writeln!(out, "{}", super::trend::chart(snapshots))?;
    }
    writeln!(out, "</section>")?;
    Ok(())
}

/// Where the findings are, by family and by rule, in their own sections so a reader
/// who wants only the files can skip past them.
fn write_tally(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    if report.findings.is_empty() {
        return Ok(());
    }
    let tally = Tally::of(&report.findings);
    let findings = report.findings.len() as u32;
    writeln!(
        out,
        "<section id=\"families\" aria-labelledby=\"families-title\">\n<h2 id=\"families-title\">Findings by family</h2>"
    )?;
    write_bars(
        out,
        "families-title",
        findings,
        tally
            .families
            .iter()
            .map(|(family, count)| ((*family).to_owned(), "", *count)),
    )?;
    writeln!(
        out,
        "</section>\n<section id=\"rules\" aria-labelledby=\"rules-title\">\n<h2 id=\"rules-title\">Findings by rule</h2>"
    )?;
    write_bars(
        out,
        "rules-title",
        findings,
        tally.rules.iter().map(
            |RuleCount {
                 slug,
                 count,
                 severity,
             }| (slug.clone(), ink_class(severity_ink(*severity)), *count),
        ),
    )?;
    writeln!(out, "</section>")?;
    Ok(())
}

/// A count and its share of `total` as filled and empty blocks, the way `--stats`
/// draws it. Hidden from a screen reader because the number beside it says the same.
fn write_bars(
    out: &mut dyn Write,
    labelled_by: &str,
    total: u32,
    rows: impl Iterator<Item = (String, &'static str, u32)>,
) -> Result<()> {
    let rows: Vec<_> = rows.collect();
    if rows.is_empty() {
        return Ok(());
    }
    writeln!(
        out,
        "<div class=\"scroll\"><table class=\"bars\" aria-labelledby=\"{labelled_by}\">\n<thead class=\"sr-only\"><tr><th scope=\"col\">Name</th><th scope=\"col\">Findings</th><th scope=\"col\">Share</th></tr></thead>\n<tbody>"
    )?;
    for (name, class, count) in rows {
        writeln!(
            out,
            "<tr><th scope=\"row\" class=\"{class}\">{}</th><td class=\"count\">{count}</td><td aria-hidden=\"true\">{}</td></tr>",
            markup(&name),
            meter(count, total)
        )?;
    }
    writeln!(out, "</tbody>\n</table></div>")?;
    Ok(())
}

fn meter(count: u32, total: u32) -> String {
    let filled = (count as usize * BAR_WIDTH)
        .checked_div(total as usize)
        .unwrap_or(0)
        .clamp(usize::from(count > 0), BAR_WIDTH);
    format!(
        "<span class=\"fill\">{}</span><span class=\"rest\">{}</span>",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(BAR_WIDTH - filled)
    )
}

fn write_files(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    writeln!(
        out,
        "<section id=\"files\" aria-labelledby=\"files-title\">\n<h2 id=\"files-title\">Files, worst first</h2>"
    )?;
    let shown = report.shown_by_file();
    let prefix = report.path_prefix();
    let mut listed = 0;
    for (file, grade) in report.priority() {
        let Some(findings) = shown.get(file) else {
            continue;
        };
        write_file(
            out,
            report,
            &prefix,
            file,
            grade,
            findings,
            listed < OPEN_FILES,
        )?;
        listed += 1;
    }
    if listed == 0 {
        writeln!(
            out,
            "<p class=\"muted\">{}</p>",
            empty_files_message(report)
        )?;
    }
    writeln!(out, "</section>")?;
    Ok(())
}

/// What an empty list means, so a reader is not left to guess whether the scan found
/// nothing or the report was narrowed to nothing.
fn empty_files_message(report: &Report<'_>) -> &'static str {
    if report.totals.findings > 0 {
        "No findings are shown: this report was narrowed to none of them."
    } else {
        "No findings \u{2014} every scanned file passed."
    }
}

/// An id a link can name: the path as shown, with the whitespace a fragment cannot
/// hold replaced.
fn anchor_of(path: &str) -> String {
    markup(&path.split_whitespace().collect::<Vec<_>>().join("_"))
}

/// The terminal's word for a file: a verdict on it, in the colour of its grade.
const fn badge_word(ink: Ink) -> &'static str {
    match ink {
        Ink::Danger => "FAIL",
        Ink::Warn => "WARN",
        _ => "PASS",
    }
}

fn write_file(
    out: &mut dyn Write,
    report: &Report<'_>,
    prefix: &str,
    file: &str,
    grade: Cleanliness,
    findings: &[&Finding],
    open: bool,
) -> Result<()> {
    let ink = grade_ink(grade);
    let shown = format!("{prefix}{file}");
    writeln!(
        out,
        "<details id=\"{}\" class=\"file\"{}>\n<summary><span class=\"badge {}\"> {} </span><h3 class=\"path\">{}</h3><span class=\"muted\">{} \u{b7} {}</span></summary>",
        anchor_of(&shown),
        if open { " open" } else { "" },
        ink_class(ink),
        badge_word(ink),
        markup(&shown),
        grade.label(),
        plural(findings.len() as u32, "finding", "findings")
    )?;
    let text = std::fs::read_to_string(report.outcome.root.join(file)).ok();
    let source = text.as_deref().map(|text| Source::of(file, text));
    let mut ordered = findings.to_vec();
    ordered.sort_by_key(|finding| finding.location.start);
    let mut on_line: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for finding in ordered {
        let nth = on_line.entry(finding.location.line).or_default();
        *nth += 1;
        let id = match nth {
            1 => format!("{}:{}", anchor_of(&shown), finding.location.line),
            more => format!("{}:{}-{more}", anchor_of(&shown), finding.location.line),
        };
        write_finding(out, finding, source.as_ref(), &id)?;
    }
    writeln!(
        out,
        "<p class=\"to-top\"><a href=\"#top\">Back to top</a></p>\n</details>"
    )?;
    Ok(())
}

/// One file's text, indexed by line and coloured once for every frame that shows a
/// piece of it.
struct Source<'text> {
    index: LineIndex<'text>,
    spans: Vec<Span>,
}

impl<'text> Source<'text> {
    fn of(file: &str, text: &'text str) -> Self {
        Self {
            index: LineIndex::new(text),
            spans: highlight::spans_for_file(file, text).unwrap_or_default(),
        }
    }

    /// The line's text as markup, each coloured run in a span of its own. A run that
    /// began on an earlier line, a block comment or a template, is cut to this one.
    fn line_markup(&self, line: &Line) -> String {
        let start = self.index.line_start(line.number) as usize;
        let end = start + line.text.len();
        let first = self
            .spans
            .partition_point(|span| span.byte_range.end <= start);
        let mut out = String::new();
        let mut at = 0;
        for span in self.spans[first..]
            .iter()
            .take_while(|span| span.byte_range.start < end)
        {
            let from = span.byte_range.start.max(start) - start;
            let to = span.byte_range.end.min(end) - start;
            if from < at || from >= to {
                continue;
            }
            out.push_str(&markup(line.text.get(at..from).unwrap_or_default()));
            out.push_str(&format!(
                "<span class=\"{}\">{}</span>",
                kind_class(span.kind),
                markup(line.text.get(from..to).unwrap_or_default())
            ));
            at = to;
        }
        out.push_str(&markup(line.text.get(at..).unwrap_or_default()));
        out
    }
}

const fn kind_class(kind: Kind) -> &'static str {
    match kind {
        Kind::Plain => "",
        Kind::Comment => "c",
        Kind::String => "s",
        Kind::Number => "n",
        Kind::Keyword => "k",
        Kind::Type => "t",
        Kind::Function => "f",
    }
}

fn write_finding(
    out: &mut dyn Write,
    finding: &Finding,
    source: Option<&Source<'_>>,
    id: &str,
) -> Result<()> {
    let spec = finding.rule.spec();
    let severity = ink_class(severity_ink(finding.severity));
    let known = if finding.known {
        " <span class=\"muted\">known</span>"
    } else {
        ""
    };
    writeln!(
        out,
        "<article id=\"{id}\" class=\"finding\">\n<h4 class=\"head\"><span class=\"{severity}\">\u{25cf}</span><span class=\"accent\">{}</span><a class=\"muted\" href=\"#{id}\">{}:{}</a><span class=\"{severity}\">{}</span>{known}</h4>",
        markup(finding.slug()),
        finding.location.line,
        finding.location.column,
        finding.severity.label()
    )?;
    let view = source.and_then(|source| {
        preview::of(&source.index, &finding.location, CONTEXT).map(|view| (source, view))
    });
    if let Some((source, view)) = view {
        write_frame(out, source, &view, severity)?;
    }
    writeln!(
        out,
        "<p class=\"evidence\">{}</p>",
        markup(&finding.evidence.to_string())
    )?;
    if !matches!(finding.evidence, Evidence::Custom { .. }) {
        writeln!(
            out,
            "<p><span class=\"muted\">Why:</span> {}</p>",
            markup(spec.why)
        )?;
    }
    if let Some(fix) = fix_markup(finding) {
        writeln!(out, "<p><span class=\"muted\">Fix:</span> {fix}</p>")?;
    }
    for site in &finding.related {
        writeln!(
            out,
            "<p><span class=\"muted\">also at</span> {}</p>",
            markup(&site.to_string())
        )?;
    }
    writeln!(out, "</article>")?;
    Ok(())
}

/// The refactorings the rule prescribes, then what to do in this place.
fn fix_markup(finding: &Finding) -> Option<String> {
    let techniques = finding
        .rule
        .spec()
        .fixes
        .iter()
        .map(|fix| fix.name())
        .collect::<Vec<_>>()
        .join(" / ");
    let technique = format!("<span class=\"accent\">{}</span>", markup(&techniques));
    let hint = markup(&finding.fix_hint);
    match (techniques.is_empty(), hint.is_empty()) {
        (true, true) => None,
        (false, true) => Some(technique),
        (true, false) => Some(hint),
        (false, false) => Some(format!("{technique} \u{2014} {hint}")),
    }
}

fn write_frame(
    out: &mut dyn Write,
    source: &Source<'_>,
    view: &Preview,
    severity: &str,
) -> Result<()> {
    writeln!(out, "<div class=\"frame\"><div class=\"rows-of-code\">")?;
    let leading_context = view
        .lines
        .iter()
        .take_while(|line| line.role == Role::Context)
        .count();
    let underlined = view
        .lines
        .iter()
        .filter(|line| line.role == Role::Offence)
        .count()
        == 1;
    for (position, line) in view.lines.iter().enumerate() {
        if view.elided > 0 && position == leading_context + HEAD_LINES {
            writeln!(
                out,
                "<div class=\"row gap\"><span class=\"no\">\u{22ee}</span><span class=\"sep\">\u{2502}</span>{} more lines</div>",
                view.elided
            )?;
        }
        write_row(out, source, line)?;
        if underlined && let Some(caret) = caret_row(line, severity) {
            writeln!(out, "{caret}")?;
        }
    }
    writeln!(out, "</div></div>")?;
    Ok(())
}

fn write_row(out: &mut dyn Write, source: &Source<'_>, line: &Line) -> Result<()> {
    let class = match line.role {
        Role::Offence => "row hit",
        Role::Context => "row",
    };
    writeln!(
        out,
        "<div class=\"{class}\"><span class=\"no\">{}</span><span class=\"sep\">\u{2502}</span><span class=\"src\">{}</span></div>",
        line.number,
        source.line_markup(line)
    )?;
    Ok(())
}

/// The `^^^` under a single-line span, which stays meaningful where a viewer drops the
/// background colour.
fn caret_row(line: &Line, severity: &str) -> Option<String> {
    let (lead, width) = line.ruler()?;
    Some(format!(
        "<div class=\"row\"><span class=\"no\"></span><span class=\"sep\">\u{2502}</span><span class=\"{severity}\">{}{}</span></div>",
        " ".repeat(lead),
        "^".repeat(width)
    ))
}

fn write_waived(out: &mut dyn Write, report: &Report<'_>) -> Result<()> {
    let waived = &report.outcome.suppressed;
    if waived.is_empty() {
        return Ok(());
    }
    let prefix = report.path_prefix();
    writeln!(
        out,
        "<section id=\"waived\" aria-labelledby=\"waived-title\">\n<h2 id=\"waived-title\">{}</h2>",
        plural(waived.len() as u32, "waived finding", "waived findings")
    )?;
    writeln!(
        out,
        "<table class=\"waived\">\n<thead><tr><th scope=\"col\">Rule</th><th scope=\"col\">Location</th><th scope=\"col\">Reason</th><th scope=\"col\">Until</th></tr></thead>\n<tbody>"
    )?;
    for one in waived {
        writeln!(
            out,
            "<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
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

fn write_footer(out: &mut dyn Write, report: &Report<'_>, project: &str) -> Result<()> {
    writeln!(
        out,
        "<footer>rqx {} \u{b7} {} \u{b7} {} scanned in {:.2}s</footer>",
        env!("CARGO_PKG_VERSION"),
        markup(project),
        plural(report.outcome.files_scanned, "file", "files"),
        report.outcome.elapsed_ms as f64 / 1000.0
    )?;
    Ok(())
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

    /// A scratch directory holding one file, removed with the guard.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn with(name: &str, file: &str, text: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rqx-html-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(file), text).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// Elements that never close. Everything else has to nest, or a browser quietly
    /// repairs the page into something else.
    const VOID: [&str; 3] = ["meta", "br", "hr"];

    fn assert_well_formed(page: &str) {
        let mut open: Vec<String> = Vec::new();
        let mut rest = page;
        while let Some(start) = rest.find('<') {
            rest = &rest[start + 1..];
            let end = rest.find('>').expect("a tag that ends");
            let tag = &rest[..end];
            rest = &rest[end + 1..];
            if tag.starts_with('!') || tag.ends_with('/') {
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
        let scratch = Scratch::with(
            "escape",
            "<b>.ts",
            "const a = '<script>alert(1)</script>'\nconst b = 1\n",
        );
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "<b>.ts")]);
        outcome.root = scratch.0.clone();
        outcome.findings[0].fix_hint = "<img src=x onerror=alert(1)>".to_owned();

        let page = page_of(&outcome);
        assert!(!page.contains("<script"), "{page}");
        assert!(!page.contains("<img"), "{page}");
        assert!(!page.contains("<b>.ts"), "{page}");
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(page.contains("&lt;b&gt;.ts"));
        assert_well_formed(&page);
    }

    #[test]
    fn a_command_with_markup_in_it_is_escaped() {
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "a.ts")]);
        outcome.root = std::path::PathBuf::from("<dir>");
        let page = page_of(&outcome);
        assert!(page.contains("$</span> rqx scan &lt;dir&gt;"), "{page}");
        assert!(!page.contains("<dir>"));
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
            "Summary",
            "By grade",
            "Findings by family",
            "Findings by rule",
        ] {
            assert!(page.contains(heading), "missing {heading}");
        }
        assert!(page.contains("$</span> rqx scan ."));
    }

    #[test]
    fn the_style_keeps_to_the_brand() {
        let style = STYLE.to_lowercase();
        for forbidden in ["gradient", "shadow", "animation", "transition", "@import"] {
            assert!(!style.contains(forbidden), "the style uses {forbidden}");
        }
        assert!(
            !style.contains("text-transform"),
            "labels are sentence case"
        );
        let page = page_of(&outcome_of(&[(RuleId::LongFunction, "a.ts")]));
        assert!(page.contains("@media print{:root{color-scheme:light;"));
        assert!(!page.contains("prefers-color-scheme"));
        assert!(page.contains("font:14px/1.55 var(--mono)"));
    }

    #[test]
    fn the_owl_wears_the_verdict_and_keeps_its_eyes() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let colour = |ink: Ink, signal: Ink| match palette.art_color(ink, signal) {
            Color::Rgb(r, g, b) => format!("fill=\"{}\"", hex((r, g, b))),
            other => panic!("not truecolor: {other:?}"),
        };
        let failing = owl(Ink::Danger);
        let healthy = owl(Ink::Accent);
        assert!(failing.contains(&colour(Ink::Signal, Ink::Danger)));
        assert!(failing.contains(&colour(Ink::Shade, Ink::Danger)));
        assert!(!failing.contains(&colour(Ink::Signal, Ink::Accent)));
        assert!(healthy.contains(&colour(Ink::Signal, Ink::Accent)));
        for eyes in [Ink::Text, Ink::Ground] {
            let fill = colour(eyes, Ink::Danger);
            assert!(failing.contains(&fill) && healthy.contains(&fill), "{fill}");
        }
        assert!(failing.contains("viewBox=\"0 0 12 12\""));
        assert!(failing.contains("shape-rendering=\"crispEdges\""));
    }

    #[test]
    fn the_hero_names_the_verdict_the_health_and_the_reasons() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let report = Report::scan(&outcome, &FindingFilter::default());
        let failed = Verdict {
            failures: vec!["1 error <over> the limit".to_owned()],
            counted: 1,
            known: 0,
        };
        let page = rendered(&report.judged(&failed));
        assert!(page.contains("\u{2716}\u{fe0e} gate failed"), "{page}");
        assert!(page.contains("<li>1 error &lt;over&gt; the limit</li>"));
        assert!(page.contains("class=\"verdict danger\""));
        assert!(page.contains(" \u{b7} grade "));
        assert!(page.contains("1 finding \u{b7} 1 file</p>"), "{page}");

        let report = Report::scan(&outcome, &FindingFilter::default());
        let passed = Verdict {
            failures: Vec::new(),
            counted: 0,
            known: 0,
        };
        let page = rendered(&report.judged(&passed));
        assert!(page.contains("\u{2713} gate passed"));
        assert!(page.contains("class=\"verdict good\""));
        assert!(!page.contains("<ul class=\"reasons\">"));
    }

    #[test]
    fn an_ungated_scan_leads_with_its_health() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let page = page_of(&outcome);
        assert!(!page.contains("gate"), "{page}");
        let health = Report::scan(&outcome, &FindingFilter::default()).health();
        assert!(page.contains("verdict"));
        assert!(
            page.contains(&format!(">health {health}/100</h1>")),
            "{page}"
        );
    }

    #[test]
    fn the_summary_is_aligned_rows_and_block_bars() {
        let outcome = outcome_of(&[
            (RuleId::LongFunction, "a.ts"),
            (RuleId::LongFunction, "b.ts"),
            (RuleId::MagicNumber, "c.ts"),
        ]);
        let page = page_of(&outcome);
        for label in ["Files", "Findings", "Grades", "Rules", "Health", "Duration"] {
            assert!(page.contains(&format!("<dt>{label}</dt>")), "no {label}");
        }
        assert!(page.contains(&"\u{2588}".repeat(16)), "the busiest bar");
        assert!(page.contains("\u{2591}"), "the empty remainder");
        assert!(page.contains("long-function"));
    }

    #[test]
    fn a_bar_is_never_empty_for_a_count_and_never_overflows() {
        let filled = |markup: &str| markup.matches('\u{2588}').count();
        let empty = |markup: &str| markup.matches('\u{2591}').count();
        assert_eq!(filled(&meter(1, 1000)), 1);
        assert_eq!(filled(&meter(0, 1000)), 0);
        assert_eq!(filled(&meter(5, 5)), BAR_WIDTH);
        assert_eq!(filled(&meter(3, 0)), 1);
        assert_eq!(filled(&meter(3, 7)) + empty(&meter(3, 7)), BAR_WIDTH);
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
    fn a_file_heading_is_the_terminals_badge_path_and_count() {
        let outcome = outcome_of(&[(RuleId::LongFunction, "a.ts")]);
        let page = page_of(&outcome);
        let report = Report::scan(&outcome, &FindingFilter::default());
        let ink = grade_ink(report.grade_of("a.ts"));
        let heading = format!(
            "<summary><span class=\"badge {}\"> {} </span><h3 class=\"path\">a.ts</h3>",
            ink_class(ink),
            badge_word(ink)
        );
        assert!(page.contains(&heading), "{page}");
        assert!(page.contains("1 finding</span></summary>"));
    }

    #[test]
    fn only_the_first_ten_files_start_open() {
        let names: Vec<String> = (0..12).map(|n| format!("f{n:02}.ts")).collect();
        let findings: Vec<(RuleId, &str)> = names
            .iter()
            .map(|name| (RuleId::LongFunction, name.as_str()))
            .collect();
        let page = page_of(&outcome_of(&findings));
        assert_eq!(page.matches("<details id=").count(), 12);
        assert_eq!(page.matches(" class=\"file\" open>").count(), 10);
    }

    #[test]
    fn a_finding_reads_as_the_terminal_does() {
        let scratch = Scratch::with("finding", "a.ts", "const answer = 42\nconst other = 1\n");
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "a.ts")]);
        outcome.root = scratch.0.clone();
        outcome.findings[0].location.column = 7;
        outcome.findings[0].location.end = 13;
        outcome.findings[0].related = vec![outcome.findings[0].location.clone()];
        let page = page_of(&outcome);

        assert!(page.contains("long-function</span>"), "{page}");
        assert!(page.contains(".ts:1\">1:7</a>"), "{page}");
        assert!(page.contains("<span class=\"muted\">Why:</span>"));
        assert!(page.contains("<span class=\"muted\">Fix:</span> <span class=\"accent\">"));
        assert!(page.contains("<span class=\"muted\">also at</span>"));
        assert!(page.contains("<div class=\"row hit\">"));
        assert!(page.contains("\u{2502}"), "the gutter separator");
        assert!(
            page.contains("<span class=\"src\"><span class=\"k\">const</span> answer = <span class=\"n\">42</span></span>"),
            "{page}"
        );
        assert!(page.contains("^^^^^^"), "the underline under the span");
        assert_well_formed(&page);
    }

    #[test]
    fn a_span_over_several_lines_is_tinted_but_not_underlined() {
        let scratch = Scratch::with("multi", "a.ts", "function f() {\n  return 1\n}\n");
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "a.ts")]);
        outcome.root = scratch.0.clone();
        outcome.findings[0].location.end_line = 3;
        outcome.findings[0].location.end = 28;
        let page = page_of(&outcome);
        assert_eq!(page.matches("<div class=\"row hit\">").count(), 3);
        assert!(!page.contains("^^"), "{page}");
    }

    #[test]
    fn a_comment_that_began_on_an_earlier_line_is_coloured_on_this_one() {
        let scratch = Scratch::with("block", "a.ts", "/* one\ntwo */ const x = 1\n");
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "a.ts")]);
        outcome.root = scratch.0.clone();
        outcome.findings[0].location.line = 2;
        outcome.findings[0].location.end_line = 2;
        outcome.findings[0].location.start = 7;
        outcome.findings[0].location.end = 26;
        let page = page_of(&outcome);
        assert!(page.contains("<span class=\"c\">two */</span>"), "{page}");
        assert!(page.contains("<span class=\"c\">/* one</span>"), "{page}");
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
    fn owners_get_a_section_after_the_summary_when_a_codeowners_file_exists() {
        let mut outcome = outcome_with(&[("web/a.ts", Severity::Error)], 2);
        outcome.owners = Some(rqx_core::ownership::CodeOwners::parse(
            "web/ @web <b>\n",
            rqx_core::ownership::Dialect::GitHub,
        ));
        let page = page_of(&outcome);

        let summary = page.find("<section id=\"summary\"").expect("a summary");
        let owners = page
            .find("<section id=\"owners\"")
            .expect("an owners section");
        let families = page.find("<section id=\"families\"").expect("families");
        assert!(summary < owners && owners < families);
        assert!(page.contains("href=\"#owners\""));
        assert!(page.contains("<th scope=\"row\">@web</th>"), "{page}");
        assert!(page.contains("&lt;b&gt;"), "owners are escaped");
        assert_well_formed(&page);
    }

    #[test]
    fn a_trend_file_adds_a_chart_after_the_summary_and_nothing_else_changes() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let snapshots: Vec<crate::trend::Snapshot> = (0..3)
            .map(|day| crate::trend::Snapshot::of(&outcome, day * 86_400))
            .collect();
        let filter = crate::report::FindingFilter::default();
        let report = Report::scan(&outcome, &filter).trended(Some(&snapshots));
        let mut out = Vec::new();
        render(&report, &mut out).expect("rendering to memory");
        let page = String::from_utf8(out).expect("utf-8");

        let summary = page.find("<section id=\"summary\"").expect("a summary");
        let trend = page.find("<section id=\"trend\"").expect("a trend section");
        let families = page.find("<section id=\"families\"").expect("families");
        assert!(summary < trend && trend < families);
        assert!(page.contains("href=\"#trend\"") && page.contains("<svg viewBox=\"0 0 640 180\""));
        assert!(!page.contains("<script"));
        assert_well_formed(&page);

        let without = page_of(&outcome);
        assert!(!without.contains("id=\"trend\""), "no --trend, no chart");
    }

    #[test]
    fn a_trend_file_with_one_snapshot_says_a_chart_needs_two() {
        let outcome = outcome_with(&[], 1);
        let snapshots = [crate::trend::Snapshot::of(&outcome, 0)];
        let filter = crate::report::FindingFilter::default();
        let report = Report::scan(&outcome, &filter).trended(Some(&snapshots));
        let mut out = Vec::new();
        render(&report, &mut out).expect("rendering to memory");
        let page = String::from_utf8(out).expect("utf-8");
        assert!(page.contains("A chart needs two snapshots"), "{page}");
        assert!(!page.contains("class=\"trend\""));
    }

    #[test]
    fn a_project_without_codeowners_has_no_owners_section() {
        let page = page_of(&outcome_with(&[("a.ts", Severity::Error)], 1));
        assert!(!page.contains("owners"), "{page}");
    }

    #[test]
    fn the_page_has_one_h1_and_a_way_around_it() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let page = page_of(&outcome);
        assert_eq!(page.matches("<h1 ").count(), 1);
        assert!(page.contains("<a class=\"skip\" href=\"#files\">"));
        assert!(page.contains("<nav class=\"toc\" aria-label=\"Contents\">"));
        for target in ["summary", "families", "rules", "files"] {
            assert!(
                page.contains(&format!("href=\"#{target}\"")),
                "no link to {target}"
            );
            assert!(
                page.contains(&format!("<section id=\"{target}\"")),
                "no {target}"
            );
        }
        assert!(!page.contains("href=\"#waived\""), "nothing is waived");
        assert!(page.contains("<header id=\"top\""));
        assert!(page.contains("<footer>"));
        assert!(!page.contains("<script"));
        assert_well_formed(&page);
    }

    #[test]
    fn a_file_and_a_finding_have_stable_ids_a_link_can_name() {
        let mut outcome = outcome_of(&[
            (RuleId::LongFunction, "src/x y.ts"),
            (RuleId::MagicNumber, "src/x y.ts"),
        ]);
        outcome.findings[1].location.line = 42;
        let again = outcome_of(&[
            (RuleId::LongFunction, "src/x y.ts"),
            (RuleId::LongFunction, "src/x y.ts"),
        ]);
        let page = page_of(&outcome);
        assert!(
            page.contains("<details id=\"src/x_y.ts\" class=\"file\""),
            "{page}"
        );
        assert!(page.contains("<article id=\"src/x_y.ts:1\" class=\"finding\">"));
        assert!(page.contains("<article id=\"src/x_y.ts:42\" class=\"finding\">"));
        let page = page_of(&again);
        assert!(
            page.contains("<article id=\"src/x_y.ts:1-2\" class=\"finding\">"),
            "{page}"
        );
    }

    #[test]
    fn every_file_ends_with_a_way_back_to_the_top() {
        let outcome = outcome_of(&[
            (RuleId::LongFunction, "a.ts"),
            (RuleId::LongFunction, "b.ts"),
        ]);
        let page = page_of(&outcome);
        assert_eq!(page.matches("<a href=\"#top\">Back to top</a>").count(), 2);
    }

    #[test]
    fn an_empty_report_says_what_happened() {
        let outcome = outcome_with(&[], 3);
        let page = page_of(&outcome);
        assert!(
            page.contains("No findings \u{2014} every scanned file passed."),
            "{page}"
        );
        assert!(!page.contains("id=\"families\""), "nothing to break down");
        assert_well_formed(&page);

        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let mut report = Report::scan(&outcome, &FindingFilter::default());
        report.findings.clear();
        assert!(rendered(&report).contains("narrowed to none of them"));
    }

    #[test]
    fn a_waived_table_names_its_rows() {
        let mut outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        let finding = outcome.findings[0].clone();
        outcome.suppressed.push(rqx_core::suppress::Suppressed {
            at: finding.location.clone(),
            finding,
            reason: None,
            until: None,
        });
        let page = page_of(&outcome);
        assert!(
            page.contains("<th scope=\"row\">long-function</th>"),
            "{page}"
        );
        assert!(page.contains("href=\"#waived\""));
    }

    #[test]
    fn the_style_prints_light_and_keeps_findings_whole() {
        assert!(STYLE.contains("@media print{"));
        assert!(STYLE.contains(".finding{break-inside:avoid}"));
        assert!(STYLE.contains(":focus-visible{outline:2px solid var(--accent)"));
        assert!(STYLE.contains("max-width:1040px"));
    }

    #[test]
    fn the_footer_names_the_version() {
        let page = page_of(&outcome_of(&[(RuleId::LongFunction, "a.ts")]));
        assert!(page.contains(&format!("<footer>rqx {} ", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn a_long_span_elides_its_middle_where_the_terminal_does() {
        let source: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        let scratch = Scratch::with("gap", "long.ts", &source);
        let mut outcome = outcome_of(&[(RuleId::LongFunction, "long.ts")]);
        outcome.root = scratch.0.clone();
        outcome.findings[0].location.end_line = 30;
        outcome.findings[0].location.end = source.len() as u32;

        let page = page_of(&outcome);
        assert!(page.contains("more lines</div>"), "{page}");
        assert_well_formed(&page);
    }

    /// Text is read on the ground, on the panel a code frame sits on and on the tint of
    /// the offending lines, and the reverse-video badge carries ground on a tone.
    #[test]
    fn both_themes_clear_the_contrast_floor() {
        for (name, theme) in [("dark", Theme::DARK), ("print", Theme::LIGHT)] {
            for (ink, colour) in [
                ("text", theme.text),
                ("muted", theme.muted),
                ("accent", theme.accent),
                ("danger", theme.danger),
                ("warn", theme.warn),
                ("good", theme.good),
                ("type", theme.kind),
            ] {
                for (ground, base) in [
                    ("ground", theme.ground),
                    ("panel", theme.panel),
                    ("tint", theme.tint),
                ] {
                    let ratio = contrast(colour, base);
                    assert!(ratio >= 4.5, "{name} {ink} on {ground} is {ratio:.2}:1");
                }
            }
            for (ink, tone) in [
                ("danger", theme.danger),
                ("warn", theme.warn),
                ("good", theme.good),
                ("accent", theme.accent),
            ] {
                let ratio = contrast(theme.ground, tone);
                assert!(ratio >= 4.5, "{name} badge text on {ink} is {ratio:.2}:1");
            }
            let border = contrast(theme.line, theme.ground);
            assert!(border >= 3.0, "{name} line is {border:.2}:1");
        }
    }

    #[test]
    fn the_dark_theme_is_the_brand_palette() {
        let dark = Theme::DARK;
        assert_eq!(hex(dark.ground), "#0f0c09");
        assert_eq!(hex(dark.panel), "#17120d");
        assert_eq!(hex(dark.rule), "#3a3127");
        assert_eq!(hex(dark.accent), "#c89b6a");
        assert_eq!(hex(dark.shade), "#785d3f");
        assert_eq!(hex(dark.kind), "#e6cfae");
    }
}
