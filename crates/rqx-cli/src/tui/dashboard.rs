use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::brand::{Ink, Palette, TAGLINE};
use super::mascot::Mascot;
use super::sprite::Sprite;
use crate::scan::{ScanOutcome, ScanProgress};

pub enum Phase {
    Scanning(ScanProgress),
    Done(Box<ScanOutcome>),
    Failed(String),
}

pub struct View<'a> {
    pub palette: Palette,
    pub wordmark: &'a Sprite,
    pub rex: &'a Mascot,
    pub phase: &'a Phase,
    pub root: &'a str,
}

pub fn render(frame: &mut Frame<'_>, view: &View<'_>) {
    let [head, body, footer] = Layout::vertical([
        Constraint::Length(view.wordmark.cell_height() + 1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_head(frame, view, head);
    render_body(frame, view, body);
    frame.render_widget(Paragraph::new(footer_line(view)), footer);
}

fn render_head(frame: &mut Frame<'_>, view: &View<'_>, area: Rect) {
    let [logo, tagline] = Layout::vertical([
        Constraint::Length(view.wordmark.cell_height()),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(view.wordmark.render(view.palette)), logo);
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("{TAGLINE}   ·   {}", view.root),
            dim(view.palette),
        )),
        tagline,
    );
}

fn render_body(frame: &mut Frame<'_>, view: &View<'_>, area: Rect) {
    let [rex, stats] =
        Layout::horizontal([Constraint::Length(view.rex.width() + 2), Constraint::Min(0)])
            .areas(area);
    frame.render_widget(Paragraph::new(view.rex.sprite().render(view.palette)), rex);
    frame.render_widget(Paragraph::new(stat_lines(view)), stats);
}

fn stat_lines(view: &View<'_>) -> Vec<Line<'static>> {
    let palette = view.palette;
    match view.phase {
        Phase::Scanning(progress) => vec![
            Line::styled(
                format!("scanning {}", view.root),
                Style::new().fg(palette.color(Ink::Bone)),
            ),
            Line::default(),
            Line::styled(
                format!("{} files", thousands(progress.files_scanned)),
                dim(palette),
            ),
            Line::styled(
                format!("{} findings", thousands(progress.findings)),
                dim(palette),
            ),
        ],
        Phase::Done(outcome) => {
            let score = outcome.counts.weighted_health_score(outcome.code_lines, &outcome.impact);
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Health  ", dim(palette)),
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
                    Style::new().fg(palette.color(Ink::Bone)),
                ),
            ];
            lines.extend(outcome.counts.by_rule.iter().take(4).map(|(rule, count)| {
                Line::styled(format!("{count:>5}  {}", rule.slug()), dim(palette))
            }));
            lines
        }
        Phase::Failed(error) => vec![Line::styled(
            format!("scan failed: {error}"),
            Style::new().fg(palette.color(Ink::Terracotta)),
        )],
    }
}

/// Written into scrollback on the way out, so quitting leaves the result behind
/// instead of erasing the only thing the run produced.
pub fn scrollback_summary(outcome: &ScanOutcome, palette: Palette, root: &str) -> Vec<Line<'static>> {
    let score = outcome.counts.weighted_health_score(outcome.code_lines, &outcome.impact);
    vec![
        Line::from(vec![
            Span::styled("  rqx  ", Style::new().fg(palette.color(Ink::Amber))),
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
                dim(palette),
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
    Line::styled(keys, dim(view.palette))
}

const fn score_ink(score: u32) -> Ink {
    match score {
        80..=100 => Ink::Moss,
        50..=79 => Ink::Amber,
        _ => Ink::Terracotta,
    }
}

fn dim(palette: Palette) -> Style {
    Style::new().fg(palette.color(Ink::Sediment))
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
        assert_eq!(score_ink(100), Ink::Moss);
        assert_eq!(score_ink(80), Ink::Moss);
        assert_eq!(score_ink(79), Ink::Amber);
        assert_eq!(score_ink(50), Ink::Amber);
        assert_eq!(score_ink(49), Ink::Terracotta);
        assert_eq!(score_ink(0), Ink::Terracotta);
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

    use super::super::brand::ColorDepth;
    use super::super::mascot::{Mascot, Mood};
    use super::*;

    const WORDMARK: &str = include_str!("../../assets/wordmark.txt");

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
            counts,
            analyzed: Default::default(),
            impact: rqx_core::impact::ImpactMap::default(),
            import_edges: Default::default(),
            findings: Vec::new(),
            omitted: 0,
            code_lines: 12_000,
            files_scanned: 340,
            skipped: Vec::new(),
            elapsed_ms: 41,
        }
    }

    fn draw(phase: Phase, mood: Mood, width: u16, height: u16) -> String {
        let wordmark = Sprite::parse(WORDMARK);
        let mut rex = Mascot::new();
        rex.set_mood(mood);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &View {
                        palette: Palette::with_depth(ColorDepth::Monochrome),
                        wordmark: &wordmark,
                        rex: &rex,
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
        let rendered = draw(Phase::Done(Box::new(outcome(7))), Mood::Idle, 80, 16);
        assert!(rendered.contains("Health"), "{rendered}");
        assert!(rendered.contains("7 findings in 340 files"), "{rendered}");
        assert!(rendered.contains("long-function"), "{rendered}");
        assert!(rendered.contains("enter browse"), "{rendered}");
        assert!(rendered.contains("every finding, a reason"), "{rendered}");
    }

    #[test]
    fn a_clean_scan_does_not_offer_a_browser_with_nothing_in_it() {
        let rendered = draw(Phase::Done(Box::new(outcome(0))), Mood::Idle, 80, 16);
        assert!(!rendered.contains("enter browse"), "{rendered}");
        assert!(rendered.contains("q quit"), "{rendered}");
    }

    #[test]
    fn a_running_scan_shows_live_counts_and_a_cancel_key() {
        let phase = Phase::Scanning(ScanProgress { files_scanned: 8412, findings: 331 });
        let rendered = draw(phase, Mood::Walking, 80, 16);
        assert!(rendered.contains("8,412 files"), "{rendered}");
        assert!(rendered.contains("331 findings"), "{rendered}");
        assert!(rendered.contains("esc cancel"), "{rendered}");
    }

    /// The mascot is the first thing clipped when the layout is wrong, and it fails
    /// silently — the screen still renders, just without a dinosaur on it.
    #[test]
    fn rex_is_drawn_and_not_clipped_away() {
        let rendered = draw(Phase::Done(Box::new(outcome(7))), Mood::Idle, 80, 16);
        let blocks = rendered.chars().filter(|c| "\u{2588}\u{2580}\u{2584}".contains(*c)).count();
        assert!(blocks > 120, "expected a wordmark and a dinosaur, got {blocks} block glyphs");
    }

    #[test]
    fn the_minimum_supported_terminal_still_renders_without_panicking() {
        let rendered = draw(Phase::Done(Box::new(outcome(7))), Mood::Idle, 60, 20);
        assert!(rendered.contains("Health"), "{rendered}");
    }
}
