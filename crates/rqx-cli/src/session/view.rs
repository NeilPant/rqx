//! What the session draws: the live rows of the viewport, and the blocks it prints into
//! scrollback. Nothing here reads the terminal or keeps state; each function turns the
//! values it is given into lines.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use rqx_core::Cleanliness;
use rqx_core::finding::Finding;
use unicode_width::UnicodeWidthStr;

use crate::brand::{Ink, Palette, score_ink, severity_ink};
use crate::scan::{ScanOutcome, ScanProgress};

use super::text::{clip, seconds, thousands};

/// Rows the viewport needs while a scan runs: the owl's height, and the hints under it.
pub const SCANNING_ROWS: u16 = 7;
/// The owl's height, which is what the viewport needs while it waits for a command and
/// the menu is shut: the owl, with the prompt beside its face.
pub const IDLE_ROWS: u16 = 6;
/// What sits beside the owl above the menu: a gap, a rule, the prompt, and below the
/// menu a rule and the status line.
const PROMPT_ROWS: u16 = 5;

const TOP_FINDINGS: usize = 5;
const OWL_GAP: &str = "  ";

pub struct Look {
    pub palette: Palette,
}

impl Look {
    fn style(&self, ink: Ink) -> Style {
        Style::new().fg(self.palette.color(ink))
    }

    fn bold(&self, ink: Ink) -> Style {
        self.style(ink).add_modifier(Modifier::BOLD)
    }

    fn dim(&self) -> Style {
        self.palette.dim()
    }
}

/// The owl on the left and what the session is busy with beside it. `text` is built
/// for the room `text_width` says there is.
pub fn working(
    frame: &mut Frame<'_>,
    look: &Look,
    owl: Vec<Line<'static>>,
    text: Vec<Line<'static>>,
) {
    let [body, hints] =
        Layout::vertical([Constraint::Length(SCANNING_ROWS - 1), Constraint::Length(1)])
            .areas(frame.area());
    let [art, beside] = Layout::horizontal([
        Constraint::Length(owl_columns(&owl) + OWL_GAP.len() as u16),
        Constraint::Min(0),
    ])
    .areas(body);
    frame.render_widget(Paragraph::new(owl), art);
    frame.render_widget(Paragraph::new(text), beside);
    frame.render_widget(
        Paragraph::new(Line::styled(" ctrl-c cancels", look.dim())),
        hints,
    );
}

fn owl_columns(owl: &[Line<'static>]) -> u16 {
    owl.first().map_or(0, |line| line.width() as u16)
}

/// How many columns are left beside the owl on a window `width` wide.
pub fn text_width(owl: &[Line<'static>], width: u16) -> usize {
    usize::from(width.saturating_sub(owl_columns(owl) + OWL_GAP.len() as u16))
}

/// What a command that is not a scan shows while it runs.
pub fn busy_lines(look: &Look, spinner: char, doing: &str) -> Vec<Line<'static>> {
    vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{spinner} "), look.bold(Ink::Accent)),
            Span::styled(doing.to_owned(), look.bold(Ink::Accent)),
        ]),
    ]
}

pub fn progress_lines(
    look: &Look,
    spinner: char,
    progress: &ScanProgress,
    width: usize,
) -> Vec<Line<'static>> {
    let files = match progress.total {
        Some(total) => format!(
            "{} of {} files",
            thousands(progress.files_scanned),
            thousands(total)
        ),
        None => format!("{} files", thousands(progress.files_scanned)),
    };
    let current = progress.file.as_deref().unwrap_or_default();
    vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{spinner} "), look.bold(Ink::Accent)),
            Span::styled(progress.phase.label().to_owned(), look.bold(Ink::Accent)),
        ]),
        Line::styled(files, look.style(Ink::Text)),
        Line::styled(clip(current, width), look.dim()),
        Line::styled(
            format!(
                "{} findings so far \u{b7} {}",
                thousands(progress.findings),
                seconds(u128::from(progress.elapsed_ms))
            ),
            look.dim(),
        ),
    ]
}

/// Rows the completion menu reserves under the prompt while it is open.
pub const MENU_ROWS: u16 = 6;

pub struct MenuRow<'a> {
    pub label: &'a str,
    pub about: &'a str,
}

pub struct Menu<'a> {
    pub rows: &'a [MenuRow<'a>],
    pub selected: usize,
}

pub struct Idle<'a> {
    pub input: &'a str,
    pub cursor: usize,
    pub menu: Option<Menu<'a>>,
    pub hints: &'a str,
    pub status: &'a str,
}

/// The rows the viewport needs for this state of the prompt.
pub const fn idle_rows(menu_open: bool) -> u16 {
    if menu_open {
        PROMPT_ROWS + MENU_ROWS
    } else {
        IDLE_ROWS
    }
}

/// The owl where it was while the scan ran, and the prompt beside it.
pub fn idle(frame: &mut Frame<'_>, look: &Look, owl: Vec<Line<'static>>, state: &Idle<'_>) {
    let [art, beside] = Layout::horizontal([
        Constraint::Length(owl_columns(&owl) + OWL_GAP.len() as u16),
        Constraint::Min(0),
    ])
    .areas(frame.area());
    let owl_height = owl.len() as u16;
    frame.render_widget(
        Paragraph::new(owl),
        ratatui::layout::Rect {
            height: owl_height.min(art.height),
            ..art
        },
    );

    let menu_height = if state.menu.is_some() { MENU_ROWS } else { 0 };
    let [_, top, prompt, below, bottom, status, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(menu_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(beside);
    let rule = Line::styled("\u{2500}".repeat(top.width as usize), look.palette.rule());
    frame.render_widget(Paragraph::new(rule.clone()), top);
    frame.render_widget(Paragraph::new(rule), bottom);

    let (shown, column) = visible(state.input, state.cursor, prompt.width.saturating_sub(2));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("> ", look.bold(Ink::Accent)),
            Span::raw(shown),
        ])),
        prompt,
    );
    frame.set_cursor_position(Position::new(prompt.x + 2 + column, prompt.y));
    if let Some(menu) = &state.menu {
        frame.render_widget(Paragraph::new(menu_lines(look, menu, below.width)), below);
    }
    frame.render_widget(
        Paragraph::new(status_line(look, state.hints, state.status, status.width)),
        status,
    );
}

fn menu_lines(look: &Look, menu: &Menu<'_>, width: u16) -> Vec<Line<'static>> {
    let label_width = menu
        .rows
        .iter()
        .map(|row| row.label.width())
        .max()
        .unwrap_or(0);
    let room = usize::from(width).saturating_sub(label_width + 5);
    menu.rows
        .iter()
        .enumerate()
        .map(|(at, row)| {
            let (marker, label) = if at == menu.selected {
                (" \u{203a} ", look.bold(Ink::Accent))
            } else {
                ("   ", look.style(Ink::Text))
            };
            let padding = " ".repeat(label_width - row.label.width());
            Line::from(vec![
                Span::styled(marker, label),
                Span::styled(format!("{}{padding}  ", row.label), label),
                Span::styled(clip(row.about, room), look.dim()),
            ])
        })
        .collect()
}

/// The hints on the left and the project's state on the right. When both do not fit
/// the hints give way, because the state is what the line is for.
fn status_line(look: &Look, hints: &str, status: &str, width: u16) -> Line<'static> {
    let right = format!("{status} ");
    let left = format!(" {hints}");
    let room = usize::from(width);
    let left = if left.width() + right.width() < room {
        left
    } else {
        String::new()
    };
    let gap = room.saturating_sub(left.width() + right.width());
    Line::from(vec![
        Span::styled(left, look.dim()),
        Span::raw(" ".repeat(gap)),
        Span::styled(right, look.dim()),
    ])
}

/// The part of the input that fits, ending at the cursor when the line is longer than
/// the row, and the column the cursor lands in.
fn visible(input: &str, cursor: usize, width: u16) -> (String, u16) {
    let width = usize::from(width);
    let before: String = input.chars().take(cursor).collect();
    let after: String = input.chars().skip(cursor).collect();
    let mut start = 0;
    let before_chars: Vec<char> = before.chars().collect();
    while before_chars[start..].iter().collect::<String>().width() >= width && start < cursor {
        start += 1;
    }
    let kept: String = before_chars[start..].iter().collect();
    let column = kept.width() as u16;
    let mut shown = kept;
    shown.push_str(&after);
    (shown.chars().take(width).collect(), column)
}

/// The box the session opens with: the owl, what was scanned and how it came out.
pub fn header(
    look: &Look,
    owl: &[Line<'static>],
    outcome: &ScanOutcome,
    root: &str,
    health: u32,
    width: u16,
) -> Vec<Line<'static>> {
    let grade = Cleanliness::of(health, outcome.counts.total);
    let facts = [
        Line::from(Span::styled(
            format!("rqx {}", env!("CARGO_PKG_VERSION")),
            look.bold(Ink::Accent),
        )),
        Line::from(Span::styled(
            format!("{root} \u{b7} {} files", thousands(outcome.files_scanned)),
            look.style(Ink::Text),
        )),
        Line::from(vec![
            Span::styled(format!("health {health}/100"), look.bold(score_ink(health))),
            Span::styled(
                format!(
                    " \u{b7} {} \u{b7} {} findings",
                    grade.label(),
                    thousands(outcome.counts.total)
                ),
                look.style(Ink::Text),
            ),
        ]),
    ];
    let art = owl.first().map_or(0, Line::width);
    let wanted = facts.iter().map(Line::width).max().unwrap_or(0);
    let inner = (art + OWL_GAP.len() + wanted + 2).min(usize::from(width).saturating_sub(2));
    let text_room = inner.saturating_sub(art + OWL_GAP.len() + 2);

    let border = look.palette.rule();
    let mut lines = vec![Line::styled(
        format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(inner)),
        border,
    )];
    for (row, art_line) in owl.iter().enumerate() {
        let mut spans = vec![Span::styled("\u{2502}", border), Span::raw(" ")];
        spans.extend(art_line.spans.iter().cloned());
        spans.push(Span::raw(OWL_GAP));
        let used = if let Some(fact) = row.checked_sub(1).and_then(|at| facts.get(at)) {
            let clipped = clip_line(fact, text_room);
            let used = clipped.width();
            spans.extend(clipped.spans);
            used
        } else {
            0
        };
        let padding = inner.saturating_sub(1 + art + OWL_GAP.len() + used);
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled("\u{2502}", border));
        lines.push(Line::from(spans));
    }
    lines.push(Line::styled(
        format!("\u{2570}{}\u{256f}", "\u{2500}".repeat(inner)),
        border,
    ));
    lines
}

/// A line cut to `width` columns. Only the last span is cut, which is enough because
/// the facts are short and the text that can be long comes last.
fn clip_line(line: &Line<'static>, width: usize) -> Line<'static> {
    if line.width() <= width {
        return line.clone();
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for span in &line.spans {
        let room = width.saturating_sub(used);
        if room == 0 {
            break;
        }
        let text = clip(span.content.as_ref(), room);
        used += text.width();
        spans.push(Span::styled(text, span.style));
    }
    Line::from(spans)
}

/// One line of what the scan did, then the worst findings and how many more there are.
pub fn summary(look: &Look, outcome: &ScanOutcome) -> Vec<Line<'static>> {
    let mut took = format!(
        "Scanned {} files in {}",
        thousands(outcome.files_scanned),
        seconds(outcome.elapsed_ms)
    );
    let lookups = outcome.stats.cache_hits + outcome.stats.cache_misses;
    if lookups > 0 {
        took.push_str(&format!(
            " \u{b7} cache {}/{}",
            thousands(outcome.stats.cache_hits),
            thousands(lookups)
        ));
    }
    let mut lines = vec![Line::from(vec![
        Span::styled(" \u{2713} ", look.bold(Ink::Good)),
        Span::styled(took, look.style(Ink::Text)),
    ])];
    if outcome.findings.is_empty() {
        lines.push(Line::styled(" Nothing to report.", look.dim()));
        return lines;
    }
    let worst = worst_findings(outcome);
    let slug_width = worst.iter().map(|f| f.slug().len()).max().unwrap_or(0);
    for (at, finding) in worst.iter().enumerate() {
        lines.push(finding_line(look, at + 1, finding, slug_width));
    }
    let more = outcome.counts.total as usize - worst.len().min(outcome.counts.total as usize);
    if more > 0 {
        lines.push(Line::styled(
            format!(
                "   \u{2026} {} more \u{b7} /findings to list them \u{b7} /open 1 to edit one",
                thousands(more as u32)
            ),
            look.dim(),
        ));
    }
    lines
}

/// Where each finding in the summary is, in the order it was listed, which is what
/// `/open 2` counts in.
pub fn listed_places(outcome: &ScanOutcome) -> Vec<(String, u32)> {
    worst_findings(outcome)
        .into_iter()
        .map(|finding| (finding.location.file.to_string(), finding.location.line))
        .collect()
}

fn worst_findings(outcome: &ScanOutcome) -> Vec<&Finding> {
    let mut ranked: Vec<&Finding> = outcome.findings.iter().collect();
    ranked.sort_by_key(|finding| std::cmp::Reverse(finding.rank()));
    ranked.truncate(TOP_FINDINGS);
    ranked
}

fn finding_line(look: &Look, number: usize, finding: &Finding, slug_width: usize) -> Line<'static> {
    let ink = severity_ink(finding.severity);
    Line::from(vec![
        Span::styled(format!(" {number} "), look.dim()),
        Span::styled("\u{25cf} ", look.style(ink)),
        Span::styled(format!("{:<8}", finding.severity.label()), look.style(ink)),
        Span::styled(
            format!("{:<slug_width$}  ", finding.slug()),
            look.style(Ink::Text),
        ),
        Span::styled(
            format!("{}:{}", finding.location.file, finding.location.line),
            look.dim(),
        ),
    ])
}

/// What was typed, kept in scrollback above whatever it produced.
pub fn echo(look: &Look, line: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("> ", look.bold(Ink::Accent)),
        Span::styled(line.to_owned(), look.style(Ink::Text)),
    ])
}

/// A line that did not work, short enough to read at a glance.
pub fn failure(look: &Look, message: &str) -> Line<'static> {
    Line::styled(format!(" \u{2717} {message}"), look.style(Ink::Danger))
}

/// What a watch saw change, in the ordinary text colour: it is news, not a footnote.
pub fn update(look: &Look, message: &str) -> Line<'static> {
    Line::styled(format!(" {message}"), look.style(Ink::Text))
}

pub fn note(look: &Look, message: &str) -> Line<'static> {
    Line::styled(format!(" {message}"), look.dim())
}

/// Every command and what it is for, in the order they are offered.
pub fn help(look: &Look, rows: &[(String, String)], width: u16) -> Vec<Line<'static>> {
    let name_width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let mut lines = vec![
        Line::styled(
            " Slash commands are the rqx commands, run on the scan above.",
            look.style(Ink::Text),
        ),
        Line::styled(
            " Text without a slash searches the findings. `/<command> --help` lists its flags.",
            look.dim(),
        ),
        Line::raw(""),
    ];
    let room = usize::from(width).saturating_sub(name_width + 6);
    for (name, about) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("   /{name:<name_width$}  "), look.bold(Ink::Accent)),
            Span::styled(clip(about, room), look.style(Ink::Text)),
        ]));
    }
    lines
}

/// The last line printed when the session ends.
pub fn farewell(outcome: &ScanOutcome, health: u32) -> String {
    format!(
        "\n  rqx looked at {} files \u{b7} health {health}/100 \u{b7} {} findings in {}\n  `rqx scan` prints the whole report.\n\n",
        thousands(outcome.files_scanned),
        thousands(outcome.counts.total),
        seconds(outcome.elapsed_ms)
    )
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use rqx_core::finding::Severity;

    use super::*;
    use crate::brand::ColorDepth;
    use crate::report::tests::outcome_with;
    use crate::session::sprite::Sprite;

    fn look() -> Look {
        Look {
            palette: Palette::with_depth(ColorDepth::Monochrome),
        }
    }

    fn owl(look: &Look) -> Vec<Line<'static>> {
        let sprite = Sprite::parse(include_str!("../../assets/mascot.txt"));
        sprite.render_frame(look.palette, sprite.frame("idle"), Ink::Accent, usize::MAX)
    }

    fn plain(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    fn screen(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn the_header_is_a_box_whose_rows_are_all_as_wide_as_its_border() {
        let look = look();
        let outcome = outcome_with(&[("src/a.ts", Severity::Error)], 3456);
        let lines = plain(&header(
            &look,
            &owl(&look),
            &outcome,
            "~/work/billing",
            82,
            60,
        ));
        assert!(lines[0].starts_with('\u{256d}') && lines[0].ends_with('\u{256e}'));
        assert!(lines.last().unwrap().starts_with('\u{2570}'));
        let widths: Vec<usize> = lines.iter().map(|line| line.width()).collect();
        assert!(widths.iter().all(|width| *width == widths[0]), "{widths:?}");
        let text = lines.join("\n");
        assert!(text.contains(&format!("rqx {}", env!("CARGO_PKG_VERSION"))));
        assert!(text.contains("~/work/billing \u{b7} 3,456 files"));
        assert!(text.contains("health 82/100"));
    }

    #[test]
    fn a_narrow_terminal_clips_the_header_text_rather_than_the_box() {
        let look = look();
        let outcome = outcome_with(&[("src/a.ts", Severity::Error)], 3);
        let lines = plain(&header(
            &look,
            &owl(&look),
            &outcome,
            &"x".repeat(80),
            40,
            40,
        ));
        assert!(lines.iter().all(|line| line.width() == 40), "{lines:?}");
    }

    #[test]
    fn the_summary_names_the_worst_findings_first_and_counts_the_rest() {
        let look = look();
        let many: Vec<(&str, Severity)> = (0..7)
            .map(|_| ("src/a.ts", Severity::Info))
            .chain([("src/api/client.ts", Severity::Error)])
            .collect();
        let outcome = outcome_with(&many, 10);
        let lines = plain(&summary(&look, &outcome));
        assert!(lines[0].contains("Scanned 10 files"));
        assert!(lines[1].contains("error") && lines[1].contains("src/api/client.ts:1"));
        assert!(lines[1].trim_start().starts_with("1 "), "{lines:?}");
        assert!(lines.last().unwrap().contains("3 more"), "{lines:?}");
    }

    #[test]
    fn a_clean_scan_says_so_instead_of_listing_nothing() {
        let outcome = outcome_with(&[], 12);
        let lines = plain(&summary(&look(), &outcome));
        assert!(lines[1].contains("Nothing to report"));
    }

    /// The part of each row beside the owl, which is where the prompt lives.
    fn beside_the_owl(rows: &[String]) -> Vec<String> {
        rows.iter()
            .map(|row| row.chars().skip(14).collect::<String>())
            .collect()
    }

    #[test]
    fn the_owl_keeps_the_left_and_the_prompt_sits_beside_its_face() {
        let mut terminal = Terminal::new(TestBackend::new(60, IDLE_ROWS)).unwrap();
        let state = Idle {
            input: "/plan --budget 20",
            cursor: 17,
            menu: None,
            hints: "/help  /plan",
            status: "health 82",
        };
        terminal
            .draw(|frame| idle(frame, &look(), owl(&look()), &state))
            .unwrap();
        let shown = screen(&terminal);
        assert!(shown[0].contains('\u{2584}'), "the owl is on the left");
        let rows = beside_the_owl(&shown);
        assert_eq!(rows[0], "");
        assert!(rows[1].starts_with('\u{2500}') && rows[3].starts_with('\u{2500}'));
        assert_eq!(rows[2], "> /plan --budget 20");
        assert!(rows[4].starts_with(" /help  /plan"), "{rows:?}");
        assert!(rows[4].ends_with("health 82"));
        terminal.backend_mut().assert_cursor_position((14 + 19, 2));
    }

    #[test]
    fn an_open_menu_sits_under_the_prompt_with_the_choice_marked() {
        let mut terminal = Terminal::new(TestBackend::new(60, idle_rows(true))).unwrap();
        let choices = [
            MenuRow {
                label: "/plan",
                about: "Rank the work",
            },
            MenuRow {
                label: "/perf",
                about: "Where the work grows",
            },
        ];
        let state = Idle {
            input: "/p",
            cursor: 2,
            menu: Some(Menu {
                rows: &choices,
                selected: 1,
            }),
            hints: "/help",
            status: "health 82",
        };
        terminal
            .draw(|frame| idle(frame, &look(), owl(&look()), &state))
            .unwrap();
        let shown = beside_the_owl(&screen(&terminal));
        assert_eq!(shown[2], "> /p");
        assert_eq!(shown[3], "   /plan  Rank the work");
        assert_eq!(shown[4], " \u{203a} /perf  Where the work grows");
        assert!(shown[3 + usize::from(MENU_ROWS)].starts_with('\u{2500}'));
        assert!(shown[4 + usize::from(MENU_ROWS)].ends_with("health 82"));
    }

    #[test]
    fn a_closed_menu_takes_no_rows() {
        assert_eq!(idle_rows(false), IDLE_ROWS);
        assert_eq!(idle_rows(true), PROMPT_ROWS + MENU_ROWS);
    }

    #[test]
    fn the_status_gives_up_its_hints_before_its_state() {
        let line = status_line(&look(), "a hint that is far too long", "health 82", 20).to_string();
        assert_eq!(line.trim(), "health 82");
        assert_eq!(line.width(), 20);
    }

    #[test]
    fn the_scanning_rows_show_the_phase_the_count_and_the_file() {
        let mut terminal = Terminal::new(TestBackend::new(60, SCANNING_ROWS)).unwrap();
        let look = look();
        let progress = ScanProgress {
            files_scanned: 1204,
            total: Some(3456),
            findings: 31,
            file: Some("src/api/client.ts".into()),
            ..ScanProgress::default()
        };
        terminal
            .draw(|frame| {
                let art = owl(&look);
                let text = progress_lines(&look, '\u{280b}', &progress, text_width(&art, 60));
                working(frame, &look, art, text);
            })
            .unwrap();
        let text = screen(&terminal).join("\n");
        assert!(text.contains("1,204 of 3,456 files"), "{text}");
        assert!(text.contains("src/api/client.ts"));
        assert!(text.contains("31 findings so far"));
        assert!(text.contains("ctrl-c cancels"));
    }

    #[test]
    fn a_long_line_scrolls_to_keep_the_cursor_in_view() {
        let (shown, column) = visible("abcdefghij", 10, 5);
        assert_eq!(shown, "ghij");
        assert_eq!(column, 4);
    }

    #[test]
    fn a_short_line_is_shown_whole_with_the_cursor_where_it_is() {
        let (shown, column) = visible("abc", 1, 20);
        assert_eq!(shown, "abc");
        assert_eq!(column, 1);
    }
}
