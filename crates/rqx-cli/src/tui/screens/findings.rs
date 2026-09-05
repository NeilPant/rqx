use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState, Wrap,
};

use rqx_core::LineIndex;
use rqx_core::finding::{Finding, Severity};

use crate::brand::{Ink, severity_ink};
use crate::preview::{self, Role};
use crate::report::FindingFilter;
use crate::tui::app::{Nav, Screen, Session, global};

#[derive(Copy, Clone, PartialEq, Eq)]
enum Group {
    Rank,
    File,
    Rule,
}

impl Group {
    const fn label(self) -> &'static str {
        match self {
            Self::Rank => "rank",
            Self::File => "file",
            Self::Rule => "rule",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Rank => Self::File,
            Self::File => Self::Rule,
            Self::Rule => Self::Rank,
        }
    }
}

/// A row in the table. Grouping inserts headings rather than only re-sorting, because
/// a sorted list with no headings makes the reader find the boundaries themselves.
enum Line_ {
    Heading(String),
    Item(usize),
}

/// The three filters the command line takes as `--rule`, `--category` and `--path`.
/// Held as typed text so the form can be edited field by field, and turned into a
/// `FindingFilter` on apply so the glob and the names mean exactly what they mean on
/// the command line.
#[derive(Default, Clone)]
struct Fields {
    rules: String,
    families: String,
    paths: String,
}

impl Fields {
    fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.families.is_empty() && self.paths.is_empty()
    }

    fn split(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    fn build(&self, floor: Option<Severity>) -> anyhow::Result<FindingFilter> {
        FindingFilter::new(
            floor,
            &Self::split(&self.rules),
            &Self::split(&self.families),
            &Self::split(&self.paths),
        )
    }

    fn summary(&self) -> String {
        [
            ("rule", &self.rules),
            ("family", &self.families),
            ("path", &self.paths),
        ]
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(label, value)| format!("{label}:{value}"))
        .collect::<Vec<_>>()
        .join("  ")
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Field {
    Rules,
    Families,
    Paths,
}

impl Field {
    const fn label(self) -> &'static str {
        match self {
            Self::Rules => "rule",
            Self::Families => "family",
            Self::Paths => "path",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Rules => Self::Families,
            Self::Families => Self::Paths,
            Self::Paths => Self::Rules,
        }
    }
}

/// What the screen is composing. Only one thing is ever being typed into, and the tab
/// key is held back while any of them is.
enum Composing {
    Nothing,
    Search,
    Form(Field),
    Reason(String),
}

/// A fix that has been proven and not yet written. Held so the reader sees what would
/// change before it changes, the way `rqx fix` without `--write` reports rather than
/// applies.
struct Pending {
    file: String,
    edits: usize,
    after: String,
}

pub struct Findings {
    rows: Vec<Line_>,
    state: TableState,
    filter: String,
    fields: Fields,
    /// The last field set that parsed. A name the catalogue does not know is reported
    /// rather than applied, so the list never silently keeps showing everything.
    applied: Fields,
    refusal: Option<String>,
    composing: Composing,
    group: Option<Group>,
    floor: Option<Severity>,
    source: Option<(String, String)>,
    pending: Option<Pending>,
    ready: bool,
}

impl Default for Findings {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            state: TableState::default(),
            filter: String::new(),
            fields: Fields::default(),
            applied: Fields::default(),
            refusal: None,
            composing: Composing::Nothing,
            group: None,
            floor: None,
            source: None,
            pending: None,
            ready: false,
        }
    }
}

impl Findings {
    /// Narrows the screen to one file, for the Files view's "what is wrong here".
    pub fn only(&mut self, file: &str) {
        self.fields = Fields {
            paths: file.to_owned(),
            ..Fields::default()
        };
        self.applied = self.fields.clone();
        self.refusal = None;
        self.ready = false;
    }

    fn reorder(&mut self, session: &Session) {
        let Some(outcome) = &session.outcome else {
            return;
        };
        let filter = match self.applied.build(self.floor) {
            Ok(filter) => filter.with_text(&self.filter),
            Err(error) => {
                self.refusal = Some(format!("{error}"));
                FindingFilter::default().with_text(&self.filter)
            }
        };
        let mut order: Vec<usize> = (0..outcome.findings.len())
            .filter(|index| {
                let finding = &outcome.findings[*index];
                session.in_scope(&finding.location.file) && filter.matches(finding)
            })
            .collect();
        let group = self.group.unwrap_or(Group::Rank);
        match group {
            Group::Rank => {}
            Group::File => order.sort_by(|a, b| {
                let (left, right) = (&outcome.findings[*a], &outcome.findings[*b]);
                left.location
                    .file
                    .cmp(&right.location.file)
                    .then(left.location.line.cmp(&right.location.line))
            }),
            Group::Rule => order.sort_by(|a, b| {
                let (left, right) = (&outcome.findings[*a], &outcome.findings[*b]);
                left.rule
                    .slug()
                    .cmp(right.slug())
                    .then(left.location.line.cmp(&right.location.line))
            }),
        }

        self.rows = Vec::with_capacity(order.len());
        let mut heading: Option<String> = None;
        for index in order {
            let finding = &outcome.findings[index];
            let key = match group {
                Group::Rank => None,
                Group::File => Some(finding.location.file.to_string()),
                Group::Rule => Some(finding.slug().to_owned()),
            };
            if let Some(key) = key
                && heading.as_deref() != Some(key.as_str())
            {
                self.rows.push(Line_::Heading(key.clone()));
                heading = Some(key);
            }
            self.rows.push(Line_::Item(index));
        }

        // A heading is a label rather than a place to be, so the selection lands on
        // the first row that is actually a finding.
        let selected = self
            .state
            .selected()
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
        self.state.select(self.nearest_item(selected));
        self.source = None;
        self.pending = None;
        self.ready = true;
    }

    fn nearest_item(&self, from: usize) -> Option<usize> {
        (from..self.rows.len())
            .chain((0..from).rev())
            .find(|at| matches!(self.rows[*at], Line_::Item(_)))
    }

    fn selected<'a>(&self, session: &'a Session) -> Option<&'a Finding> {
        let outcome = session.outcome.as_ref()?;
        match self.rows.get(self.state.selected()?)? {
            Line_::Item(index) => outcome.findings.get(*index),
            Line_::Heading(_) => None,
        }
    }

    /// Steps over headings, so holding an arrow key never stalls on a label.
    fn step(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let mut at = self.state.selected().unwrap_or(0) as isize;
        let last = self.rows.len() as isize - 1;
        for _ in 0..self.rows.len() {
            at = (at + delta).clamp(0, last);
            if matches!(self.rows[at as usize], Line_::Item(_)) {
                self.state.select(Some(at as usize));
                self.source = None;
                self.pending = None;
                return;
            }
            if at == 0 || at == last {
                break;
            }
        }
    }

    /// Proves the file's fixes without writing them: the same plan and the same
    /// verification `rqx fix` runs, so nothing is offered here that the command line
    /// would withhold.
    fn prove(&mut self, session: &Session) {
        self.pending = None;
        let Some(finding) = self.selected(session) else {
            return;
        };
        let Some(outcome) = &session.outcome else {
            return;
        };
        let file = finding.location.file.to_string();
        let path = outcome.root.join(&file);
        let Some(language) = rqx_core::Language::of_path(&path) else {
            return;
        };
        let Ok(source) = std::fs::read_to_string(&path) else {
            self.refusal = Some(format!("cannot read {file}"));
            return;
        };
        let findings: Vec<Finding> = outcome
            .findings
            .iter()
            .filter(|other| *other.location.file == *file)
            .cloned()
            .collect();
        match crate::fix::for_file(
            &file,
            &source,
            language,
            &findings,
            session.config.for_file(&path),
        ) {
            Some((plan, after)) => {
                self.pending = Some(Pending {
                    file,
                    edits: plan.kept.len(),
                    after,
                });
                self.refusal = None;
            }
            None => self.refusal = Some(format!("nothing in {file} can be fixed mechanically")),
        }
    }

    fn write_pending(&mut self, session: &Session) -> Nav {
        let Some(pending) = self.pending.take() else {
            return Nav::Stay;
        };
        let Some(outcome) = &session.outcome else {
            return Nav::Stay;
        };
        let path = outcome.root.join(&pending.file);
        match crate::fix::write_atomically(&path, &pending.after) {
            // A rescan rather than a local edit to the list: a fix changes what every
            // cross-file rule says, and a screen that patched its own rows would
            // disagree with the next scan.
            Ok(()) => Nav::Scan,
            Err(error) => {
                self.refusal = Some(format!("{error:#}"));
                Nav::Redraw
            }
        }
    }

    fn waive(&mut self, session: &Session, reason: &str) -> Nav {
        let Some(finding) = self.selected(session) else {
            return Nav::Stay;
        };
        let Some(outcome) = &session.outcome else {
            return Nav::Stay;
        };
        let file = finding.location.file.to_string();
        let line = finding.location.line;
        let rule = finding.slug().to_owned();
        let path = outcome.root.join(&file);
        let Some(language) = rqx_core::Language::of_path(&path) else {
            return Nav::Stay;
        };
        let Ok(source) = std::fs::read_to_string(&path) else {
            self.refusal = Some(format!("cannot read {file}"));
            return Nav::Redraw;
        };
        let Some(waiver) =
            rqx_core::suppress::waiver(&source, language, line, &rule, reason, None, false)
        else {
            self.refusal = Some("a waiver needs a reason rqx will honour".to_owned());
            return Nav::Redraw;
        };
        let mut lines: Vec<&str> = source.split_inclusive('\n').collect();
        let at = (waiver.line as usize - 1).min(lines.len());
        lines.insert(at, &waiver.text);
        match crate::fix::write_atomically(&path, &lines.concat()) {
            Ok(()) => Nav::Scan,
            Err(error) => {
                self.refusal = Some(format!("{error:#}"));
                Nav::Redraw
            }
        }
    }

    fn field_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Rules => &mut self.fields.rules,
            Field::Families => &mut self.fields.families,
            Field::Paths => &mut self.fields.paths,
        }
    }
}

impl Screen for Findings {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        if !self.ready {
            self.reorder(session);
        }
        let Some(outcome) = &session.outcome else {
            super::nothing_yet(frame, area, session, crate::tui::app::Tab::Findings);
            return;
        };

        let form = matches!(self.composing, Composing::Form(_));
        let [bar, form_row, list, detail] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(u16::from(form)),
            Constraint::Percentage(42),
            Constraint::Min(6),
        ])
        .areas(area);

        let shown = self
            .rows
            .iter()
            .filter(|row| matches!(row, Line_::Item(_)))
            .count();
        let total = outcome
            .findings
            .iter()
            .filter(|finding| session.in_scope(&finding.location.file))
            .count();
        let mut header = vec![Span::styled(
            format!("  {shown} of {total} findings"),
            Style::new().fg(palette.color(Ink::Text)),
        )];
        if let Some(floor) = self.floor {
            header.push(Span::styled(
                format!("  \u{2265} {}", floor.label()),
                palette.dim(),
            ));
        }
        if let Some(group) = self.group {
            header.push(Span::styled(
                format!("  by {}", group.label()),
                palette.dim(),
            ));
        }
        if !self.applied.is_empty() {
            header.push(Span::styled(
                format!("  {}", self.applied.summary()),
                Style::new().fg(palette.color(Ink::Accent)),
            ));
        }
        if matches!(self.composing, Composing::Search) || !self.filter.is_empty() {
            header.push(Span::styled(
                format!(
                    "  /{}{}",
                    self.filter,
                    if matches!(self.composing, Composing::Search) {
                        "\u{2588}"
                    } else {
                        ""
                    }
                ),
                Style::new().fg(palette.color(Ink::Accent)),
            ));
        }
        if let Composing::Reason(reason) = &self.composing {
            header.push(Span::styled(
                format!("  waive because: {reason}\u{2588}"),
                Style::new().fg(palette.color(Ink::Warn)),
            ));
        }
        if let Some(refusal) = &self.refusal {
            header.push(Span::styled(
                format!("  {refusal}"),
                Style::new().fg(palette.color(Ink::Danger)),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(header)), bar);

        if let Composing::Form(active) = self.composing {
            let mut spans = vec![Span::styled("  ", palette.dim())];
            for field in [Field::Rules, Field::Families, Field::Paths] {
                let value = match field {
                    Field::Rules => &self.fields.rules,
                    Field::Families => &self.fields.families,
                    Field::Paths => &self.fields.paths,
                };
                let style = if field == active {
                    Style::new().fg(palette.color(Ink::Accent))
                } else {
                    palette.dim()
                };
                spans.push(Span::styled(format!("{}:", field.label()), palette.dim()));
                spans.push(Span::styled(
                    format!("{value}{}  ", if field == active { "\u{2588}" } else { "" }),
                    style,
                ));
            }
            spans.push(Span::styled(
                "  tab next field \u{b7} enter apply \u{b7} esc cancel",
                palette.dim(),
            ));
            frame.render_widget(Paragraph::new(Line::from(spans)), form_row);
        }

        let rows: Vec<Row<'static>> = self
            .rows
            .iter()
            .map(|row| match row {
                Line_::Heading(label) => Row::new(vec![
                    Span::styled(" \u{25be} ", palette.dim()),
                    Span::styled(
                        label.clone(),
                        Style::new()
                            .fg(palette.color(Ink::Text))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::default(),
                ]),
                Line_::Item(index) => {
                    let finding = &outcome.findings[*index];
                    Row::new(vec![
                        Span::styled(
                            format!(" {} ", severity_mark(finding.severity)),
                            Style::new().fg(palette.color(severity_ink(finding.severity))),
                        ),
                        Span::styled(
                            finding.slug().to_owned(),
                            Style::new().fg(palette.color(Ink::Accent)),
                        ),
                        Span::styled(finding.location.to_string(), palette.dim()),
                    ])
                }
            })
            .collect();

        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(3),
                    Constraint::Length(26),
                    Constraint::Min(10),
                ],
            )
            .row_highlight_style(
                Style::new()
                    .fg(palette.color(Ink::Text))
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            )
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
            list,
            &mut self.state,
        );

        let mut scrollbar =
            ScrollbarState::new(self.rows.len()).position(self.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(palette.dim()),
            list,
            &mut scrollbar,
        );

        self.render_detail(frame, detail, session);
    }

    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav {
        match &self.composing {
            Composing::Search => {
                match key.code {
                    KeyCode::Esc => {
                        self.filter.clear();
                        self.composing = Composing::Nothing;
                    }
                    KeyCode::Enter => self.composing = Composing::Nothing,
                    KeyCode::Backspace => {
                        self.filter.pop();
                    }
                    KeyCode::Char(character) => self.filter.push(character),
                    _ => return Nav::Stay,
                }
                self.reorder(session);
                return Nav::Redraw;
            }
            Composing::Form(active) => {
                let active = *active;
                match key.code {
                    KeyCode::Esc => {
                        self.fields = self.applied.clone();
                        self.composing = Composing::Nothing;
                    }
                    KeyCode::Tab | KeyCode::Down => self.composing = Composing::Form(active.next()),
                    KeyCode::Enter => {
                        self.refusal = None;
                        self.applied = self.fields.clone();
                        self.composing = Composing::Nothing;
                        self.reorder(session);
                    }
                    KeyCode::Backspace => {
                        self.field_mut(active).pop();
                    }
                    KeyCode::Char(character) => self.field_mut(active).push(character),
                    _ => return Nav::Stay,
                }
                return Nav::Redraw;
            }
            Composing::Reason(reason) => {
                let mut reason = reason.clone();
                match key.code {
                    KeyCode::Esc => self.composing = Composing::Nothing,
                    KeyCode::Enter => {
                        self.composing = Composing::Nothing;
                        if reason.trim().is_empty() {
                            self.refusal =
                                Some("a waiver rqx will honour needs a reason".to_owned());
                            return Nav::Redraw;
                        }
                        return self.waive(session, reason.trim());
                    }
                    KeyCode::Backspace => {
                        reason.pop();
                        self.composing = Composing::Reason(reason);
                    }
                    KeyCode::Char(character) => {
                        reason.push(character);
                        self.composing = Composing::Reason(reason);
                    }
                    _ => return Nav::Stay,
                }
                return Nav::Redraw;
            }
            Composing::Nothing => {}
        }

        // A proven fix waiting to be written owns `esc` and `y`, so neither leaves the
        // screen with an unanswered question on it.
        if self.pending.is_some() {
            match key.code {
                KeyCode::Char('y') => return self.write_pending(session),
                KeyCode::Esc => {
                    self.pending = None;
                    return Nav::Redraw;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::PageDown => self.step(10),
            KeyCode::PageUp => self.step(-10),
            KeyCode::Home => self.state.select(self.nearest_item(0)),
            KeyCode::End => {
                let last = self.rows.len().saturating_sub(1);
                self.state.select(self.nearest_item(last));
            }
            KeyCode::Char('/') => self.composing = Composing::Search,
            KeyCode::Char('f') => self.composing = Composing::Form(Field::Rules),
            KeyCode::Char('g') => {
                self.group = Some(self.group.unwrap_or(Group::Rank).next());
                self.reorder(session);
            }
            KeyCode::Char('s') => {
                self.floor = match self.floor {
                    None => Some(Severity::Warning),
                    Some(Severity::Warning) => Some(Severity::Error),
                    Some(_) => None,
                };
                self.reorder(session);
            }
            KeyCode::Char('a') => self.prove(session),
            KeyCode::Char('x') => self.composing = Composing::Reason(String::new()),
            KeyCode::Enter | KeyCode::Char('o') => {
                let Some(finding) = self.selected(session) else {
                    return Nav::Stay;
                };
                return Nav::Open(finding.location.file.to_string(), finding.location.line);
            }
            _ => return global(key).unwrap_or(Nav::Stay),
        }
        Nav::Redraw
    }

    fn capturing(&self) -> bool {
        !matches!(self.composing, Composing::Nothing)
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("\u{2191}\u{2193}", "move"),
            ("enter", "open"),
            ("/", "search"),
            ("f", "filter"),
            ("g", "group"),
            ("s", "severity"),
            ("a", "fix"),
            ("x", "waive"),
            ("esc", "back"),
        ]
    }
}

const fn severity_mark(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "\u{2716}",
        Severity::Warning => "\u{26a0}",
        Severity::Info => "\u{2139}",
    }
}

impl Findings {
    fn render_detail(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let Some(finding) = self.selected(session) else {
            frame.render_widget(
                Paragraph::new(Line::styled("  no findings match", palette.dim())).block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_style(palette.rule()),
                ),
                area,
            );
            return;
        };

        let file = finding.location.file.to_string();
        if self.source.as_ref().is_none_or(|(path, _)| *path != file) {
            let root = session.outcome.as_ref().map(|outcome| outcome.root.clone());
            let text = root
                .map(|root| root.join(&file))
                .and_then(|path| std::fs::read_to_string(path).ok());
            self.source = text.map(|text| (file.clone(), text));
        }

        let mut lines = Vec::new();
        if let Some((_, text)) = &self.source {
            let index = LineIndex::new(text);
            if let Some(view) = preview::of(&index, &finding.location, 2) {
                let ink = severity_ink(finding.severity);
                for line in &view.lines {
                    let body = match (&line.mark, line.role) {
                        (Some(mark), _) => vec![
                            Span::styled(line.text[..mark.start].to_owned(), palette.dim()),
                            Span::styled(
                                line.text[mark.clone()].to_owned(),
                                Style::new()
                                    .bg(palette.color(ink))
                                    .fg(palette.color(Ink::Ground)),
                            ),
                            Span::styled(line.text[mark.end..].to_owned(), palette.dim()),
                        ],
                        (None, Role::Offence) => vec![Span::styled(
                            line.text.clone(),
                            Style::new().fg(palette.color(Ink::Text)),
                        )],
                        (None, Role::Context) => {
                            vec![Span::styled(line.text.clone(), palette.dim())]
                        }
                    };
                    let mut spans = vec![Span::styled(
                        format!("{:>6} \u{2502} ", line.number),
                        palette.dim(),
                    )];
                    spans.extend(body);
                    lines.push(Line::from(spans));
                }
            }
        }

        let spec = finding.rule.spec();
        lines.push(Line::default());
        if let Some(pending) = &self.pending {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} proven edits in {}", pending.edits, pending.file),
                    Style::new()
                        .fg(palette.color(Ink::Good))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   y to write them \u{b7} esc to leave them", palette.dim()),
            ]));
            lines.push(Line::default());
        }
        lines.push(Line::styled(
            finding.evidence.to_string(),
            Style::new().fg(palette.color(Ink::Text)),
        ));
        lines.push(Line::from(vec![
            Span::styled("Why  ", palette.dim()),
            Span::styled(spec.why, Style::new().fg(palette.color(Ink::Text))),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Fix  ", palette.dim()),
            Span::styled(
                spec.fixes
                    .iter()
                    .map(|fix| fix.name())
                    .collect::<Vec<_>>()
                    .join(" / "),
                Style::new().fg(palette.color(Ink::Good)),
            ),
            Span::styled(format!("  \u{2014} {}", finding.fix_hint), palette.dim()),
        ]));

        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(palette.rule()),
            ),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::{ColorDepth, Palette};
    use crate::report::tests::outcome_of;
    use rqx_core::rule::RuleId;
    use std::sync::Arc;

    fn session(findings: &[(RuleId, &str)]) -> Session {
        let mut session = Session::probe(Palette::with_depth(ColorDepth::Truecolor));
        session.outcome = Some(Arc::new(outcome_of(findings)));
        session
    }

    fn headings(screen: &Findings) -> Vec<String> {
        screen
            .rows
            .iter()
            .filter_map(|row| match row {
                Line_::Heading(label) => Some(label.clone()),
                Line_::Item(_) => None,
            })
            .collect()
    }

    /// Grouping used to be a sort with nothing to see: the rows moved and the reader
    /// had to find the boundaries themselves.
    #[test]
    fn grouping_by_file_puts_a_heading_above_each_one() {
        let session = session(&[
            (RuleId::LongFunction, "b.ts"),
            (RuleId::MagicNumber, "a.ts"),
            (RuleId::LongFunction, "a.ts"),
        ]);
        let mut screen = Findings::default();
        screen.group = Some(Group::File);
        screen.reorder(&session);
        assert_eq!(headings(&screen), ["a.ts", "b.ts"]);
        assert_eq!(screen.rows.len(), 5, "two headings and three findings");
    }

    #[test]
    fn ungrouped_rows_carry_no_headings() {
        let session = session(&[(RuleId::LongFunction, "a.ts")]);
        let mut screen = Findings::default();
        screen.reorder(&session);
        assert!(headings(&screen).is_empty());
    }

    /// A heading is a label rather than a place to be, so neither the first selection
    /// nor a step ever lands on one.
    #[test]
    fn the_selection_never_rests_on_a_heading() {
        let session = session(&[
            (RuleId::LongFunction, "a.ts"),
            (RuleId::MagicNumber, "b.ts"),
        ]);
        let mut screen = Findings::default();
        screen.group = Some(Group::File);
        screen.reorder(&session);

        let is_item = |screen: &Findings| {
            matches!(
                screen.rows[screen.state.selected().expect("a selection")],
                Line_::Item(_)
            )
        };
        assert!(is_item(&screen), "the first selection skipped the heading");
        for _ in 0..screen.rows.len() {
            screen.step(1);
            assert!(is_item(&screen), "a step landed on a heading");
        }
        for _ in 0..screen.rows.len() {
            screen.step(-1);
            assert!(is_item(&screen), "a step back landed on a heading");
        }
    }

    /// The three filters are the command line's own, so a name the catalogue does not
    /// know is refused here exactly as `--rule` refuses it.
    #[test]
    fn an_unknown_rule_in_the_form_is_reported_rather_than_applied() {
        let session = session(&[(RuleId::LongFunction, "a.ts")]);
        let mut screen = Findings::default();
        screen.applied = Fields {
            rules: "no-such-rule".to_owned(),
            ..Fields::default()
        };
        screen.reorder(&session);
        assert!(screen.refusal.is_some(), "the typo was silently accepted");
        assert_eq!(screen.rows.len(), 1, "the list still shows what it had");
    }

    #[test]
    fn narrowing_to_one_file_keeps_only_that_file() {
        let session = session(&[
            (RuleId::LongFunction, "a.ts"),
            (RuleId::MagicNumber, "b.ts"),
        ]);
        let mut screen = Findings::default();
        screen.only("a.ts");
        screen.reorder(&session);
        assert_eq!(screen.rows.len(), 1);
        assert_eq!(
            screen
                .selected(&session)
                .map(|finding| finding.location.file.to_string()),
            Some("a.ts".to_owned())
        );
    }
}
