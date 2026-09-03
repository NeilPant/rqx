use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use rqx_core::finding::{Finding, Severity};
use rqx_ts::LineIndex;

use crate::brand::{Ink, Palette};
use crate::report::FindingFilter;
use crate::scan::ScanOutcome;

/// Findings are read against the file on disk, which is re-read rather than held
/// from the scan; caching the last one keeps arrow-key paging from re-reading the
/// same file on every keystroke.
struct SourceCache {
    path: String,
    text: String,
}

pub struct Browser<'a> {
    outcome: &'a ScanOutcome,
    palette: Palette,
    order: Vec<usize>,
    state: ListState,
    group_by_file: bool,
    filter: String,
    editing_filter: bool,
    showing_help: bool,
    cache: Option<SourceCache>,
    pub should_quit: bool,
}

impl<'a> Browser<'a> {
    pub fn new(outcome: &'a ScanOutcome, palette: Palette) -> Self {
        let mut browser = Self {
            outcome,
            palette,
            order: Vec::new(),
            state: ListState::default(),
            group_by_file: false,
            filter: String::new(),
            editing_filter: false,
            showing_help: false,
            cache: None,
            should_quit: false,
        };
        browser.reorder();
        browser
    }

    fn reorder(&mut self) {
        let filter = FindingFilter::default().with_text(&self.filter);
        let mut order: Vec<usize> = (0..self.outcome.findings.len())
            .filter(|index| filter.matches(&self.outcome.findings[*index]))
            .collect();
        if self.group_by_file {
            order.sort_by(|a, b| {
                let (left, right) = (&self.outcome.findings[*a], &self.outcome.findings[*b]);
                left.location
                    .file
                    .cmp(&right.location.file)
                    .then(left.location.line.cmp(&right.location.line))
            });
        }
        self.order = order;
        let selected = self.state.selected().unwrap_or(0).min(self.order.len().saturating_sub(1));
        self.state.select((!self.order.is_empty()).then_some(selected));
        self.cache = None;
    }

    fn selected(&self) -> Option<&'a Finding> {
        let index = *self.order.get(self.state.selected()?)?;
        self.outcome.findings.get(index)
    }

    fn step(&mut self, delta: isize) {
        if self.order.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        let last = self.order.len() as isize - 1;
        self.state.select(Some(current.saturating_add(delta).clamp(0, last) as usize));
    }

    pub fn handle(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        if self.editing_filter {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.editing_filter = false;
                    self.reorder();
                }
                KeyCode::Enter => self.editing_filter = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.reorder();
                }
                KeyCode::Char(character) => {
                    self.filter.push(character);
                    self.reorder();
                }
                _ => return false,
            }
            return true;
        }

        if self.showing_help {
            self.showing_help = false;
            return true;
        }

        match key.code {
            KeyCode::Char('?') => self.showing_help = true,
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::PageDown => self.step(10),
            KeyCode::PageUp => self.step(-10),
            KeyCode::Home => self.state.select(Some(0)),
            KeyCode::End => self.state.select(Some(self.order.len().saturating_sub(1))),
            KeyCode::Tab => {
                self.group_by_file = !self.group_by_file;
                self.reorder();
            }
            KeyCode::Char('/') => self.editing_filter = true,
            _ => return false,
        }
        true
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let [header, list, detail, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(45),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        frame.render_widget(Paragraph::new(self.header_line()), header);
        self.render_list(frame, list);
        self.render_detail(frame, detail);
        frame.render_widget(Paragraph::new(self.footer_line()), footer);
        if self.showing_help {
            self.render_help(frame);
        }
    }

    fn render_help(&self, frame: &mut Frame<'_>) {
        const KEYS: [(&str, &str); 8] = [
            ("\u{2191} \u{2193}  j k", "move through the findings"),
            ("pgup pgdn", "move a screen at a time"),
            ("home end", "first and last finding"),
            ("tab", "group by file, or by rule"),
            ("/", "filter by rule name or path"),
            ("esc", "clear the filter, or leave"),
            ("?", "this help"),
            ("q", "quit"),
        ];

        let area = centred(frame.area(), 52, KEYS.len() as u16 + 4);
        frame.render_widget(ratatui::widgets::Clear, area);

        let mut lines = vec![Line::default()];
        lines.extend(KEYS.map(|(key, meaning)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:>12}  "),
                    Style::new().fg(self.palette.color(Ink::Accent)),
                ),
                Span::styled(meaning, Style::new().fg(self.palette.color(Ink::Text))),
            ])
        }));
        lines.push(Line::default());
        lines.push(Line::styled("  any key to close", self.palette.dim()));

        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.palette.dim())
                    .title(" keys "),
            ),
            area,
        );
    }

    fn header_line(&self) -> Line<'static> {
        let shown = self.order.len();
        let total = self.outcome.counts.total;
        let mut spans = vec![
            Span::styled("  rqx  ", Style::new().fg(self.palette.color(Ink::Accent))),
            Span::styled(
                format!("{shown} of {total} findings"),
                Style::new().fg(self.palette.color(Ink::Text)),
            ),
        ];
        if self.outcome.omitted > 0 {
            spans.push(Span::styled(
                format!("  ({} not retained for display)", self.outcome.omitted),
                self.palette.dim(),
            ));
        }
        if !self.filter.is_empty() || self.editing_filter {
            spans.push(Span::styled(
                format!("   /{}{}", self.filter, if self.editing_filter { "\u{2588}" } else { "" }),
                Style::new().fg(self.palette.color(Ink::Accent)),
            ));
        }
        Line::from(spans)
    }

    fn render_list(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let items: Vec<ListItem<'_>> = self
            .order
            .iter()
            .map(|index| {
                let finding = &self.outcome.findings[*index];
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:>4} ", severity_mark(finding.severity)),
                        Style::new().fg(self.palette.color(severity_ink(finding.severity))),
                    ),
                    Span::styled(
                        format!("{:<24}", finding.rule.slug()),
                        Style::new().fg(self.palette.color(Ink::Accent)),
                    ),
                    Span::styled(finding.location.to_string(), self.palette.dim()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(self.palette.dim()),
            )
            .highlight_style(
                Style::new()
                    .fg(self.palette.color(Ink::Text))
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            );
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn render_detail(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Some(finding) = self.selected() else {
            frame.render_widget(
                Paragraph::new(Line::styled("no findings match", self.palette.dim())),
                area,
            );
            return;
        };

        let spec = finding.rule.spec();
        let mut lines = self.excerpt(finding);
        lines.push(Line::default());
        lines.push(Line::styled(
            finding.evidence.to_string(),
            Style::new().fg(self.palette.color(Ink::Text)),
        ));
        lines.push(Line::from(vec![
            Span::styled("Why  ", self.palette.dim()),
            Span::styled(spec.why, Style::new().fg(self.palette.color(Ink::Text))),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Fix  ", self.palette.dim()),
            Span::styled(
                spec.fixes.iter().map(|fix| fix.name()).collect::<Vec<_>>().join(" / "),
                Style::new().fg(self.palette.color(Ink::Good)),
            ),
            Span::styled(format!("  \u{2014} {}", finding.fix_hint), self.palette.dim()),
        ]));
        for site in &finding.related {
            lines.push(Line::styled(format!("also at {site}"), self.palette.dim()));
        }

        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::TOP).border_style(self.palette.dim())),
            area,
        );
    }

    fn excerpt(&mut self, finding: &Finding) -> Vec<Line<'static>> {
        let path = finding.location.file.as_ref();
        if self.cache.as_ref().is_none_or(|cache| cache.path != path) {
            match std::fs::read_to_string(self.outcome.root.join(path)) {
                Ok(text) => self.cache = Some(SourceCache { path: path.to_owned(), text }),
                Err(_) => return vec![Line::styled("source unavailable", self.palette.dim())],
            }
        }
        let Some(cache) = &self.cache else {
            return Vec::new();
        };

        let index = LineIndex::new(&cache.text);
        let focus = finding.location.line;
        (focus.saturating_sub(1).max(1)..=focus + 1)
            .map(|number| {
                let text = index.line_text(number);
                let ink = if number == focus { Ink::Text } else { Ink::Muted };
                Line::from(vec![
                    Span::styled(format!("{number:>6} "), self.palette.dim()),
                    Span::styled(
                        text.to_owned(),
                        Style::new().fg(self.palette.color(ink)),
                    ),
                ])
            })
            .collect()
    }

    fn footer_line(&self) -> Line<'static> {
        let keys = if self.editing_filter {
            "type to filter    enter apply    esc clear"
        } else if self.group_by_file {
            "\u{2191}\u{2193} move    tab group by rule    / filter    ? keys    q quit"
        } else {
            "\u{2191}\u{2193} move    tab group by file    / filter    ? keys    q quit"
        };
        Line::styled(format!("  {keys}"), self.palette.dim())
    }
}

/// Clamped rather than centred blindly, because a help box wider than the terminal
/// renders as a torn border instead of a panel.
fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

const fn severity_ink(severity: Severity) -> Ink {
    match severity {
        Severity::Error => Ink::Danger,
        Severity::Warning => Ink::Warn,
        Severity::Info => Ink::Muted,
    }
}

const fn severity_mark(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "\u{2716}",
        Severity::Warning => "\u{26a0}",
        Severity::Info => "\u{2139}",
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rqx_core::finding::{Confidence, Evidence, SourceRange};
    use rqx_core::rule::RuleId;
    use rqx_core::score::FindingCounts;
    use std::sync::Arc;

    use crate::brand::ColorDepth;
    use super::*;

    fn finding(rule: RuleId, file: &str, line: u32) -> Finding {
        Finding {
            rule,
            location: SourceRange {
                file: Arc::from(file),
                start: 0,
                end: 1,
                line,
                column: 1,
                end_line: line,
            },
            severity: rule.spec().severity,
            confidence: Confidence::Heuristic,
            evidence: Evidence::StatementCount { count: 90, threshold: 40 },
            fix_hint: String::new(),
            related: Vec::new(),
        }
    }

    fn outcome() -> ScanOutcome {
        let findings = vec![
            finding(RuleId::LongFunction, "src/orders.ts", 30),
            finding(RuleId::DeepNesting, "src/billing.ts", 10),
            finding(RuleId::LongFunction, "src/billing.ts", 5),
        ];
        let mut counts = FindingCounts::default();
        for item in &findings {
            counts.record(item);
        }
        ScanOutcome {
            root: std::path::PathBuf::from("."),
            counts,
            analyzed: Default::default(),
            impact: rqx_core::impact::ImpactMap::default(),
            import_edges: Default::default(),
            findings,
            findings_by_file: std::collections::BTreeMap::new(),
            omitted: 0,
            code_lines: 900,
            files_scanned: 2,
            skipped: Vec::new(),
            elapsed_ms: 3,
        }
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn browser(outcome: &ScanOutcome) -> Browser<'_> {
        Browser::new(outcome, Palette::with_depth(ColorDepth::Monochrome))
    }

    #[test]
    fn opens_on_the_first_finding() {
        let scan = outcome();
        let browser = browser(&scan);
        assert_eq!(browser.order.len(), 3);
        assert_eq!(browser.selected().map(|f| f.rule), Some(RuleId::LongFunction));
    }

    #[test]
    fn movement_stops_at_both_ends_instead_of_wrapping_or_panicking() {
        let scan = outcome();
        let mut browser = browser(&scan);
        for _ in 0..20 {
            browser.handle(press(KeyCode::Down));
        }
        assert_eq!(browser.state.selected(), Some(2));
        for _ in 0..20 {
            browser.handle(press(KeyCode::Up));
        }
        assert_eq!(browser.state.selected(), Some(0));
    }

    #[test]
    fn filtering_matches_rule_names_and_paths() {
        let scan = outcome();
        let mut browser = browser(&scan);
        browser.handle(press(KeyCode::Char('/')));
        for character in "billing".chars() {
            browser.handle(press(KeyCode::Char(character)));
        }
        assert_eq!(browser.order.len(), 2, "two findings live in billing.ts");

        browser.handle(press(KeyCode::Esc));
        assert_eq!(browser.order.len(), 3, "esc clears the filter");

        browser.handle(press(KeyCode::Char('/')));
        for character in "deep".chars() {
            browser.handle(press(KeyCode::Char(character)));
        }
        assert_eq!(browser.order.len(), 1);
        assert_eq!(browser.selected().map(|f| f.rule), Some(RuleId::DeepNesting));
    }

    /// The selection index survives into a shorter list, so a filter typed while
    /// sitting on the last row must not leave it pointing past the end.
    #[test]
    fn narrowing_the_list_pulls_the_selection_back_in_range() {
        let scan = outcome();
        let mut browser = browser(&scan);
        browser.handle(press(KeyCode::End));
        assert_eq!(browser.state.selected(), Some(2));

        browser.handle(press(KeyCode::Char('/')));
        for character in "deep".chars() {
            browser.handle(press(KeyCode::Char(character)));
        }
        assert_eq!(browser.state.selected(), Some(0));
        assert!(browser.selected().is_some(), "selection must still resolve");
    }

    #[test]
    fn a_filter_matching_nothing_leaves_no_selection_and_does_not_panic() {
        let scan = outcome();
        let mut browser = browser(&scan);
        browser.handle(press(KeyCode::Char('/')));
        for character in "zzzz".chars() {
            browser.handle(press(KeyCode::Char(character)));
        }
        assert!(browser.order.is_empty());
        assert_eq!(browser.state.selected(), None);
        assert!(browser.selected().is_none());
        browser.handle(press(KeyCode::Down));
    }

    #[test]
    fn grouping_by_file_orders_by_path_then_line() {
        let scan = outcome();
        let mut browser = browser(&scan);
        browser.handle(press(KeyCode::Tab));
        let ordered: Vec<(&str, u32)> = browser
            .order
            .iter()
            .map(|index| {
                let location = &scan.findings[*index].location;
                (location.file.as_ref(), location.line)
            })
            .collect();
        assert_eq!(ordered, [("src/billing.ts", 5), ("src/billing.ts", 10), ("src/orders.ts", 30)]);
    }

    #[test]
    fn help_opens_on_question_mark_and_any_key_closes_it() {
        let scan = outcome();
        let mut browser = browser(&scan);
        browser.handle(press(KeyCode::Char('?')));
        assert!(browser.showing_help);
        browser.handle(press(KeyCode::Char('j')));
        assert!(!browser.showing_help);
        assert_eq!(browser.state.selected(), Some(0), "the closing key must not also move");
        assert!(!browser.should_quit);
    }

    #[test]
    fn the_help_panel_never_exceeds_the_terminal() {
        let tiny = Rect { x: 0, y: 0, width: 20, height: 6 };
        let area = centred(tiny, 52, 12);
        assert!(area.width <= tiny.width && area.height <= tiny.height);
        assert!(area.x + area.width <= tiny.width);
        assert!(area.y + area.height <= tiny.height);
    }

    #[test]
    fn typing_a_quit_key_into_the_filter_types_it_instead_of_quitting() {
        let scan = outcome();
        let mut browser = browser(&scan);
        browser.handle(press(KeyCode::Char('/')));
        browser.handle(press(KeyCode::Char('q')));
        assert!(!browser.should_quit, "`q` is a character while filtering");
        assert_eq!(browser.filter, "q");
    }
}
