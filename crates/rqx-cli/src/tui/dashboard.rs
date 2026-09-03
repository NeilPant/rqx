use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::brand::{Ink, Palette, TAGLINE, score_ink};
use super::sprite::Sprite;
use crate::scan::{ScanOutcome, ScanProgress};

pub enum Phase {
    Scanning(ScanProgress),
    Done(Box<ScanOutcome>),
    Failed(String),
}

pub struct View<'a> {
    pub palette: Palette,
    pub mascot: &'a Sprite,
    pub phase: &'a Phase,
    pub root: &'a str,
}

pub fn render(frame: &mut Frame<'_>, view: &View<'_>) {
    let [head, body, footer] = Layout::vertical([
        Constraint::Length(view.mascot.cell_height() + 1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_head(frame, view.palette, view.mascot, view.root, head);
    render_body(frame, view, body);
    frame.render_widget(Paragraph::new(footer_line(view)), footer);
}

/// The mascot is the only drawn art; the name is plain text, because a pixel-art
/// wordmark has to be redrawn for every weight and size it is ever shown at, and a
/// terminal already has a font.
pub fn render_head(
    frame: &mut Frame<'_>,
    palette: Palette,
    mascot: &Sprite,
    root: &str,
    area: Rect,
) {
    let [art, text] =
        Layout::horizontal([Constraint::Length(mascot.width() + 3), Constraint::Min(0)])
            .areas(area);

    frame.render_widget(Paragraph::new(mascot.render(palette)), art);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "rqx",
                    Style::new().fg(palette.color(Ink::Text)).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" v{}", env!("CARGO_PKG_VERSION")), palette.dim()),
            ]),
            Line::styled(TAGLINE, palette.dim()),
            Line::styled(root.to_owned(), palette.dim()),
        ]),
        text,
    );
}

fn render_body(frame: &mut Frame<'_>, view: &View<'_>, area: Rect) {
    frame.render_widget(Paragraph::new(stat_lines(view)), area);
}

fn stat_lines(view: &View<'_>) -> Vec<Line<'static>> {
    let palette = view.palette;
    match view.phase {
        Phase::Scanning(progress) => vec![
            Line::styled(
                format!("scanning {}", view.root),
                Style::new().fg(palette.color(Ink::Text)),
            ),
            Line::default(),
            Line::styled(
                format!("{} files", thousands(progress.files_scanned)),
                palette.dim(),
            ),
            Line::styled(
                format!("{} findings", thousands(progress.findings)),
                palette.dim(),
            ),
        ],
        Phase::Done(outcome) => {
            let score = outcome.counts.weighted_health_score(outcome.code_lines, &outcome.impact);
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Health  ", palette.dim()),
                    Span::styled(
                        format!("{score}/100"),
                        Style::new()
                            .fg(palette.color(score_ink(score)))
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::default(),
                Line::styled(
                    format!(
                        "{} findings in {} files",
                        thousands(outcome.counts.total),
                        thousands(outcome.files_scanned)
                    ),
                    Style::new().fg(palette.color(Ink::Text)),
                ),
            ];
            lines.extend(outcome.counts.by_rule.iter().take(4).map(|(rule, count)| {
                Line::styled(format!("{count:>5}  {}", rule.slug()), palette.dim())
            }));
            lines
        }
        Phase::Failed(error) => vec![Line::styled(
            format!("scan failed: {error}"),
            Style::new().fg(palette.color(Ink::Danger)),
        )],
    }
}

/// Written into scrollback on the way out, so quitting leaves the result behind
/// instead of erasing the only thing the run produced.
pub fn scrollback_summary(outcome: &ScanOutcome, palette: Palette, root: &str) -> Vec<Line<'static>> {
    let score = outcome.counts.weighted_health_score(outcome.code_lines, &outcome.impact);
    vec![
        Line::from(vec![
            Span::styled("  rqx  ", Style::new().fg(palette.color(Ink::Accent))),
            Span::styled(
                format!("{score}/100"),
                Style::new().fg(palette.color(score_ink(score))).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "   {} findings · {} files · {}ms   {root}",
                    thousands(outcome.counts.total),
                    thousands(outcome.files_scanned),
                    outcome.elapsed_ms
                ),
                palette.dim(),
            ),
        ]),
        Line::default(),
    ]
}

fn footer_line(view: &View<'_>) -> Line<'static> {
    let keys = match view.phase {
        Phase::Scanning(_) => "esc cancel",
        Phase::Done(outcome) if outcome.counts.total > 0 => "enter browse    r rescan    q quit",
        _ => "r rescan    q quit",
    };
    Line::styled(keys, view.palette.dim())
}

fn thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_groups_from_the_right() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(11_913), "11,913");
        assert_eq!(thousands(468_665), "468,665");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn score_colour_moves_with_the_score() {
        assert_eq!(score_ink(100), Ink::Good);
        assert_eq!(score_ink(80), Ink::Good);
        assert_eq!(score_ink(79), Ink::Warn);
        assert_eq!(score_ink(50), Ink::Warn);
        assert_eq!(score_ink(49), Ink::Danger);
        assert_eq!(score_ink(0), Ink::Danger);
    }
}

#[cfg(test)]
mod snapshots {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use rqx_core::finding::{Confidence, Evidence, Finding, Severity, SourceRange};
    use rqx_core::rule::RuleId;
    use rqx_core::score::FindingCounts;
    use std::sync::Arc;

    use crate::brand::ColorDepth;
    use super::*;

    const MASCOT: &str = include_str!("../../assets/mascot.txt");

    fn outcome(total: u32) -> ScanOutcome {
        let mut counts = FindingCounts::default();
        for _ in 0..total {
            counts.record(&Finding {
                rule: RuleId::LongFunction,
                location: SourceRange {
                    file: Arc::from("a.ts"),
                    start: 0,
                    end: 1,
                    line: 1,
                    column: 1,
                    end_line: 1,
                },
                severity: Severity::Warning,
                confidence: Confidence::Heuristic,
                evidence: Evidence::StatementCount { count: 90, threshold: 40 },
                fix_hint: String::new(),
                related: Vec::new(),
            });
        }
        ScanOutcome {
            root: std::path::PathBuf::from("."),
            counts,
            analyzed: Default::default(),
            impact: rqx_core::impact::ImpactMap::default(),
            import_edges: Default::default(),
            findings: Vec::new(),
            findings_by_file: std::collections::BTreeMap::new(),
            lines_by_file: std::collections::BTreeMap::new(),
            omitted: 0,
            code_lines: 12_000,
            files_scanned: 340,
            skipped: Vec::new(),
            elapsed_ms: 41,
        }
    }

    fn draw(phase: Phase, width: u16, height: u16) -> String {
        let mascot = Sprite::parse(MASCOT);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &View {
                        palette: Palette::with_depth(ColorDepth::Monochrome),
                        mascot: &mascot,
                        phase: &phase,
                        root: "src",
                    },
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_finished_scan_shows_the_score_the_counts_and_the_browse_key() {
        let rendered = draw(Phase::Done(Box::new(outcome(7))), 80, 16);
        assert!(rendered.contains("Health"), "{rendered}");
        assert!(rendered.contains("7 findings in 340 files"), "{rendered}");
        assert!(rendered.contains("long-function"), "{rendered}");
        assert!(rendered.contains("enter browse"), "{rendered}");
        assert!(rendered.contains("every finding, a reason"), "{rendered}");
    }

    #[test]
    fn a_clean_scan_does_not_offer_a_browser_with_nothing_in_it() {
        let rendered = draw(Phase::Done(Box::new(outcome(0))), 80, 16);
        assert!(!rendered.contains("enter browse"), "{rendered}");
        assert!(rendered.contains("q quit"), "{rendered}");
    }

    #[test]
    fn a_running_scan_shows_live_counts_and_a_cancel_key() {
        let phase = Phase::Scanning(ScanProgress { files_scanned: 8412, findings: 331 });
        let rendered = draw(phase, 80, 16);
        assert!(rendered.contains("8,412 files"), "{rendered}");
        assert!(rendered.contains("331 findings"), "{rendered}");
        assert!(rendered.contains("esc cancel"), "{rendered}");
    }

    /// A layout mistake eats the mascot silently: the screen still renders, just
    /// without the thing that identifies it.
    #[test]
    fn the_mascot_is_drawn_and_not_clipped_away() {
        let rendered = draw(Phase::Done(Box::new(outcome(7))), 80, 16);
        let blocks = rendered.chars().filter(|c| "\u{2588}\u{2580}\u{2584}".contains(*c)).count();
        assert!(blocks > 20, "expected the mascot, got {blocks} block glyphs");
    }

    /// The name is plain text now, so it has to appear as characters rather than as
    /// block glyphs — a regression here would leave the header anonymous.
    #[test]
    fn the_name_and_version_render_as_text() {
        let rendered = draw(Phase::Done(Box::new(outcome(7))), 80, 16);
        assert!(rendered.contains("rqx v"), "{rendered}");
        assert!(rendered.contains(env!("CARGO_PKG_VERSION")), "{rendered}");
    }

    #[test]
    fn the_minimum_supported_terminal_still_renders_without_panicking() {
        let rendered = draw(Phase::Done(Box::new(outcome(7))), 60, 20);
        assert!(rendered.contains("Health"), "{rendered}");
    }
}
