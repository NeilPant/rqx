use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::brand::Ink;
use crate::tui::app::{Nav, Page, Screen, Session, Tab, Task, global, is_press, row_at, wheel};

const DEFAULT_BASE: &str = "origin/HEAD";

/// What one row does. `Ask` is the one entry that cannot run on being chosen: a
/// review needs a revision, and guessing one would run the wrong scan.
#[derive(Clone)]
enum Does {
    Go(Nav),
    Ask,
}

struct Entry {
    label: &'static str,
    about: &'static str,
    does: Does,
}

struct Group {
    title: &'static str,
    /// One line saying what the whole group is for, shown under the list.
    about: &'static str,
}

const GROUPS: [Group; 5] = [
    Group {
        title: "Analyse",
        about: "run the analysis, or run it again",
    },
    Group {
        title: "Browse",
        about: "what the last scan found, one question per view",
    },
    Group {
        title: "Report",
        about: "the answers that read as prose rather than as a table",
    },
    Group {
        title: "Set up",
        about: "the commands that write a file rather than answer a question",
    },
    Group {
        title: "Session",
        about: "the settings this run uses, and the way out",
    },
];

/// The entries of one group. Built rather than declared because two of them change
/// with the session: watching toggles, and every browsable view is a tab.
fn entries(group: usize, session: &Session) -> Vec<Entry> {
    match group {
        0 => vec![
            Entry {
                label: "Scan",
                about: "this project, every file",
                does: Does::Go(Nav::Scan),
            },
            Entry {
                label: "Review a diff",
                about: "only what changed against a revision",
                does: Does::Ask,
            },
            Entry {
                label: "Refresh",
                about: "re-analyse only what changed on disk",
                does: Does::Go(Nav::Refresh),
            },
            Entry {
                label: if session.watching { "Unwatch" } else { "Watch" },
                about: "re-analyse on every save",
                does: Does::Go(Nav::Watch),
            },
        ],
        1 => Tab::ALL
            .iter()
            .map(|tab| Entry {
                label: tab.title(),
                about: tab.promise(),
                does: Does::Go(Nav::To(*tab)),
            })
            .collect(),
        2 => Page::ALL
            .iter()
            .map(|page| Entry {
                label: page.menu_title(),
                about: page.about(),
                does: Does::Go(Nav::Read(*page)),
            })
            .collect(),
        3 => [Task::WriteConfig, Task::Baseline, Task::ClearCache]
            .into_iter()
            .map(|task| Entry {
                label: task.title(),
                about: task.about(),
                does: Does::Go(Nav::Do(task)),
            })
            .collect(),
        _ => vec![
            Entry {
                label: "Settings",
                about: Tab::Config.promise(),
                does: Does::Go(Nav::To(Tab::Config)),
            },
            Entry {
                label: "Quit",
                about: "leave, and print what the session found",
                does: Does::Go(Nav::Quit),
            },
        ],
    }
}

/// One blank line above the first row of either column, so the two read as parallel
/// lists rather than as one staggered against the other.
const ABOVE: u16 = 1;
const GROUPS_WIDTH: u16 = 16;
/// Long enough for `Record a baseline` to keep a gap before the description it is
/// read against.
const LABEL_WIDTH: usize = 20;

#[derive(Default)]
pub struct Menu {
    group: usize,
    cursor: usize,
    groups_at: Rect,
    entries_at: Rect,
    /// Non-empty while the base revision is being typed, which is the one entry that
    /// needs an answer before it can run.
    base: Option<String>,
}

impl Menu {
    fn choose(&mut self, session: &Session) -> Nav {
        let entries = entries(self.group, session);
        match entries.get(self.cursor).map(|entry| entry.does.clone()) {
            Some(Does::Go(nav)) => nav,
            Some(Does::Ask) => {
                self.base = Some(DEFAULT_BASE.to_owned());
                Nav::Redraw
            }
            None => Nav::Stay,
        }
    }

    fn open(&mut self, group: usize) {
        self.group = group.min(GROUPS.len() - 1);
        self.cursor = 0;
    }

    fn step(&mut self, delta: isize, session: &Session) {
        let last = entries(self.group, session).len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }
}

impl Screen for Menu {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let entries = entries(self.group, session);
        self.cursor = self.cursor.min(entries.len().saturating_sub(1));

        let [about, body, prompt] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .areas(area);
        let [groups, list] =
            Layout::horizontal([Constraint::Length(GROUPS_WIDTH), Constraint::Min(24)]).areas(body);
        self.groups_at = groups;
        self.entries_at = list;

        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("  {}", GROUPS[self.group].about),
                palette.dim(),
            )),
            about,
        );

        let mut heads = vec![Line::default()];
        heads.extend(GROUPS.iter().enumerate().map(|(index, group)| {
            let here = index == self.group;
            Line::from(vec![
                Span::styled(
                    format!(" {} ", index + 1),
                    Style::new().fg(palette.color(Ink::Accent)),
                ),
                Span::styled(
                    group.title,
                    if here {
                        Style::new()
                            .fg(palette.color(Ink::Accent))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(palette.color(Ink::Muted))
                    },
                ),
            ])
        }));
        frame.render_widget(Paragraph::new(heads), groups);

        let mut lines = vec![Line::default()];
        lines.extend(entries.iter().enumerate().map(|(index, entry)| {
            let here = index == self.cursor;
            Line::from(vec![
                Span::styled(
                    format!("  {} ", if here { "\u{25b8}" } else { " " }),
                    Style::new().fg(palette.color(Ink::Accent)),
                ),
                Span::styled(
                    format!("{:<LABEL_WIDTH$}", entry.label),
                    if here {
                        Style::new()
                            .fg(palette.color(Ink::Accent))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(palette.color(Ink::Text))
                    },
                ),
                Span::styled(entry.about, palette.dim()),
            ])
        }));
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(palette.rule()),
            ),
            list,
        );

        let mut trailer = Vec::new();
        if let Some(base) = &self.base {
            trailer.push(Line::from(vec![
                Span::styled("  review against ", palette.dim()),
                Span::styled(
                    format!("{base}\u{2588}"),
                    Style::new().fg(palette.color(Ink::Accent)),
                ),
            ]));
            trailer.push(Line::styled("  enter to run, esc to cancel", palette.dim()));
        } else if let Some(failure) = &session.failure {
            trailer.push(Line::styled(
                format!("  {failure}"),
                Style::new().fg(palette.color(Ink::Danger)),
            ));
        } else if let Some(notice) = &session.notice {
            trailer.push(Line::styled(
                format!("  {notice}"),
                Style::new().fg(palette.color(Ink::Good)),
            ));
        } else {
            trailer.push(Line::styled(
                "  ctrl-p reaches all of this by name, and every rule and file with it",
                palette.dim(),
            ));
        }
        frame.render_widget(Paragraph::new(trailer), prompt);
    }

    fn click(&mut self, event: MouseEvent, session: &Session) -> Nav {
        if self.base.is_some() {
            return Nav::Stay;
        }
        if let Some(rows) = wheel(event) {
            self.step(rows, session);
            return Nav::Redraw;
        }
        if !is_press(event) {
            return Nav::Stay;
        }
        if let Some(row) = row_at(self.groups_at, ABOVE, 0, event)
            && row < GROUPS.len()
        {
            self.open(row);
            return Nav::Redraw;
        }
        let Some(row) = row_at(self.entries_at, ABOVE, 0, event) else {
            return Nav::Stay;
        };
        if row >= entries(self.group, session).len() {
            return Nav::Stay;
        }
        self.cursor = row;
        self.choose(session)
    }

    fn handle(&mut self, key: KeyEvent, session: &Session) -> Nav {
        if let Some(base) = &mut self.base {
            return match key.code {
                KeyCode::Char(letter) => {
                    base.push(letter);
                    Nav::Redraw
                }
                KeyCode::Backspace => {
                    base.pop();
                    Nav::Redraw
                }
                KeyCode::Enter if base.trim().is_empty() => Nav::Stay,
                KeyCode::Enter => {
                    let base = self.base.take().unwrap_or_default();
                    Nav::Review(base.trim().to_owned())
                }
                KeyCode::Esc => {
                    self.base = None;
                    Nav::Redraw
                }
                _ => Nav::Stay,
            };
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(1, session),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1, session),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.step(isize::MAX / 2, session),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                self.open((self.group + 1) % GROUPS.len());
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
                self.open((self.group + GROUPS.len() - 1) % GROUPS.len());
            }
            // The digit picks a group rather than an entry: the entries move with the
            // session and a number that meant a different thing each run is no number.
            KeyCode::Char(digit @ '1'..='9') if (digit as usize - '1' as usize) < GROUPS.len() => {
                self.open(digit as usize - '1' as usize);
            }
            KeyCode::Enter => return self.choose(session),
            // `esc` at the root has nowhere to go back to, so it leaves.
            _ => {
                return global(key).map_or(Nav::Stay, |nav| {
                    if nav == Nav::Back { Nav::Quit } else { nav }
                });
            }
        }
        Nav::Redraw
    }

    fn capturing(&self) -> bool {
        self.base.is_some()
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("\u{2190}\u{2192}", "group"),
            ("\u{2191}\u{2193}", "move"),
            ("enter", "run"),
            ("ctrl-p", "by name"),
            ("?", "keys"),
            ("q", "quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::brand::{ColorDepth, Palette};

    use super::*;

    fn session() -> Session {
        Session::probe(Palette::with_depth(ColorDepth::Monochrome))
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    /// The menu used to reach five things out of twenty-five, so the rest existed only
    /// for someone who already knew the key.
    #[test]
    fn every_tab_and_every_page_is_reachable_from_the_menu() {
        let session = session();
        let mut reached: Vec<Nav> = Vec::new();
        for group in 0..GROUPS.len() {
            for entry in entries(group, &session) {
                if let Does::Go(nav) = entry.does {
                    reached.push(nav);
                }
            }
        }
        for tab in Tab::ALL {
            assert!(
                reached.contains(&Nav::To(tab)),
                "{tab:?} is not in the menu"
            );
        }
        for page in Page::ALL {
            assert!(
                reached.contains(&Nav::Read(page)),
                "{page:?} is not in the menu"
            );
        }
        for task in [Task::WriteConfig, Task::Baseline, Task::ClearCache] {
            assert!(
                reached.contains(&Nav::Do(task)),
                "{task:?} is not in the menu"
            );
        }
        assert!(reached.contains(&Nav::To(Tab::Config)));
        assert!(reached.contains(&Nav::Scan));
        assert!(reached.contains(&Nav::Quit));
    }

    #[test]
    fn a_digit_opens_a_group_and_the_cursor_starts_at_its_top() {
        let session = session();
        let mut menu = Menu::default();
        menu.step(1, &session);
        assert_eq!(menu.cursor, 1);
        menu.handle(key(KeyCode::Char('2')), &session);
        assert_eq!(menu.group, 1);
        assert_eq!(
            menu.cursor, 0,
            "the cursor kept a row the new group may not have"
        );
    }

    #[test]
    fn groups_wrap_in_both_directions() {
        let session = session();
        let mut menu = Menu::default();
        menu.handle(key(KeyCode::Left), &session);
        assert_eq!(menu.group, GROUPS.len() - 1);
        menu.handle(key(KeyCode::Right), &session);
        assert_eq!(menu.group, 0);
    }

    /// A group's entries move with the session, so a cursor left past the end of a
    /// shorter one would index nothing.
    #[test]
    fn the_cursor_never_leaves_the_group_it_is_in() {
        let session = session();
        let mut menu = Menu::default();
        menu.open(1);
        menu.step(isize::MAX / 2, &session);
        assert_eq!(menu.cursor, Tab::ALL.len() - 1);
        menu.open(3);
        assert!(menu.cursor < entries(3, &session).len());
    }

    #[test]
    fn a_click_on_a_group_opens_it_and_a_click_on_an_entry_runs_it() {
        use crossterm::event::{MouseButton, MouseEventKind};

        let session = session();
        let mut menu = Menu::default();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("a terminal");
        terminal
            .draw(|frame| menu.render(frame, frame.area(), &session))
            .expect("a frame");

        let (groups, list) = (menu.groups_at, menu.entries_at);
        let press = |column: u16, row: u16| crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };

        assert_eq!(
            menu.click(press(groups.x + 2, groups.y + ABOVE + 2), &session),
            Nav::Redraw
        );
        assert_eq!(menu.group, 2, "the third group was not opened");

        terminal
            .draw(|frame| menu.render(frame, frame.area(), &session))
            .expect("a frame");
        assert_eq!(
            menu.click(press(list.x + 4, list.y + ABOVE), &session),
            Nav::Read(Page::ALL[0])
        );
    }
}
