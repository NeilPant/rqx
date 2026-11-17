//! What the `/` menu offers while a command is being typed.
//!
//! Command names first, then the flags of the command named, then the values a flag
//! takes: files the scan found, rule slugs, families and the choices clap lists. The
//! flags come from the clap definition the command line parses with, so a flag added
//! there is completable here with no second list to keep.

use rqx_core::rule::{Category, RuleId};

use super::commands::{SESSION_ONLY, TWINS, subcommand_of};
use super::fuzzy::score;

/// How many suggestions the menu shows.
pub const SHOWN: usize = 6;

/// What a flag, or a command's own argument, takes after it.
#[derive(Clone)]
enum Values {
    /// A switch, or a value nothing can be suggested for.
    Nothing,
    Choices(Vec<String>),
    Files,
    Rules,
    Families,
}

struct Flag {
    long: String,
    about: String,
    values: Values,
}

struct Entry {
    name: String,
    about: String,
    flags: Vec<Flag>,
    argument: Values,
}

pub struct Catalogue {
    entries: Vec<Entry>,
    files: Vec<String>,
}

/// One row of the menu, and what accepting it does to the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub label: String,
    pub about: String,
    /// The characters of the typed line this replaces, from here to the cursor.
    pub from: usize,
    /// The text left of the cursor that this stands for. When it is already what the
    /// suggestion would put there, accepting changes nothing and Enter should submit.
    pub typed: String,
    pub insert: String,
}

impl Suggestion {
    pub fn changes_nothing(&self) -> bool {
        self.insert.trim_end() == self.typed
    }
}

impl Catalogue {
    pub fn new(files: impl IntoIterator<Item = String>) -> Self {
        let mut parser = crate::parser();
        parser.build();
        let mut entries = Vec::new();
        for name in TWINS {
            let Some(command) = parser.find_subcommand(subcommand_of(name)) else {
                continue;
            };
            entries.push(Entry {
                name: (*name).to_owned(),
                about: crate::session::commands::about_of(command),
                flags: command
                    .get_arguments()
                    .filter(|argument| !argument.is_hide_set() && !argument.is_global_set())
                    .filter_map(flag_of)
                    .collect(),
                argument: if name == "explain" {
                    Values::Rules
                } else {
                    Values::Nothing
                },
            });
        }
        for (name, about) in SESSION_ONLY {
            entries.push(Entry {
                name: (*name).to_owned(),
                about: (*about).to_owned(),
                flags: Vec::new(),
                argument: match name {
                    "watch" => Values::Choices(vec!["on".to_owned(), "off".to_owned()]),
                    "open" => Values::Files,
                    _ => Values::Nothing,
                },
            });
        }
        Self {
            entries,
            files: files.into_iter().collect(),
        }
    }

    /// Suggestions for the line left of the cursor. Empty when it is not a command.
    pub fn suggest(&self, before: &str) -> Vec<Suggestion> {
        let Some(typed) = before.strip_prefix('/') else {
            return Vec::new();
        };
        let words: Vec<&str> = typed.split_whitespace().collect();
        let fresh = typed.ends_with(char::is_whitespace);
        if words.is_empty() || (words.len() == 1 && !fresh) {
            return self.commands(words.first().copied().unwrap_or_default());
        }
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.name == words[0] || subcommand_of(&entry.name) == words[0])
        else {
            return Vec::new();
        };
        let (current, earlier) = if fresh {
            ("", &words[1..])
        } else {
            (words[words.len() - 1], &words[1..words.len() - 1])
        };
        let from = before.chars().count() - current.chars().count();
        let slot = Slot { current, from };
        if current.starts_with('-') {
            return flags(entry, &slot);
        }
        if let Some(values) = earlier.last().and_then(|last| value_of(entry, last)) {
            return self.values(&values, &slot);
        }
        if matches!(entry.argument, Values::Nothing) {
            return flags(entry, &slot);
        }
        self.values(&entry.argument, &slot)
    }

    fn commands(&self, typed: &str) -> Vec<Suggestion> {
        let slot = Slot {
            current: typed,
            from: 0,
        };
        let commands = self.entries.iter().map(|entry| {
            (
                format!("/{}", entry.name),
                entry.about.clone(),
                format!("/{} ", entry.name),
                entry.name.clone(),
            )
        });
        ranked(commands, &slot, true)
            .into_iter()
            .map(|suggestion| Suggestion {
                typed: format!("/{typed}"),
                ..suggestion
            })
            .collect()
    }

    fn values(&self, values: &Values, slot: &Slot<'_>) -> Vec<Suggestion> {
        let candidates: Vec<String> = match values {
            Values::Nothing => return Vec::new(),
            Values::Choices(choices) => choices.clone(),
            Values::Files => self.files.clone(),
            Values::Rules => RuleId::ALL
                .iter()
                .map(|rule| rule.spec().slug.to_owned())
                .collect(),
            Values::Families => Category::ALL
                .iter()
                .map(|family| family.slug().to_owned())
                .collect(),
        };
        ranked(
            candidates
                .into_iter()
                .map(|value| (value.clone(), String::new(), format!("{value} "), value)),
            slot,
            false,
        )
    }
}

/// Where in the line the word being completed starts, and what has been typed of it.
struct Slot<'a> {
    current: &'a str,
    from: usize,
}

fn flags(entry: &Entry, slot: &Slot<'_>) -> Vec<Suggestion> {
    let typed = slot.current.trim_start_matches('-');
    ranked(
        entry.flags.iter().map(|flag| {
            (
                format!("--{}", flag.long),
                flag.about.clone(),
                format!("--{} ", flag.long),
                flag.long.clone(),
            )
        }),
        &Slot {
            current: typed,
            ..*slot
        },
        false,
    )
    .into_iter()
    .map(|suggestion| Suggestion {
        typed: slot.current.to_owned(),
        ..suggestion
    })
    .collect()
}

/// What the flag just before the cursor takes, if it is one that takes anything.
fn value_of(entry: &Entry, word: &str) -> Option<Values> {
    let long = word.strip_prefix("--")?;
    let flag = entry.flags.iter().find(|flag| flag.long == long)?;
    (!matches!(flag.values, Values::Nothing)).then(|| flag.values.clone())
}

/// Scores each `(label, about, insert, searched)` against what was typed, best first,
/// ties broken alphabetically. With nothing typed the order is alphabetical, which for
/// commands is the order a reader would look for one in.
fn ranked(
    candidates: impl Iterator<Item = (String, String, String, String)>,
    slot: &Slot<'_>,
    keep_order: bool,
) -> Vec<Suggestion> {
    let mut scored: Vec<(usize, usize, Suggestion)> = candidates
        .enumerate()
        .filter_map(|(position, (label, about, insert, searched))| {
            let rank = score(&searched, slot.current)?;
            Some((
                rank,
                position,
                Suggestion {
                    label,
                    about,
                    from: slot.from,
                    typed: slot.current.to_owned(),
                    insert,
                },
            ))
        })
        .collect();
    scored.sort_by(|left, right| {
        left.0.cmp(&right.0).then_with(|| {
            if keep_order {
                left.1.cmp(&right.1)
            } else {
                left.2.label.cmp(&right.2.label)
            }
        })
    });
    scored
        .into_iter()
        .take(SHOWN)
        .map(|(_, _, suggestion)| suggestion)
        .collect()
}

fn flag_of(argument: &clap::Arg) -> Option<Flag> {
    let long = argument.get_long()?;
    if matches!(long, "help" | "version") {
        return None;
    }
    Some(Flag {
        long: long.to_owned(),
        about: argument
            .get_help()
            .map(ToString::to_string)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        values: values_of(argument, long),
    })
}

fn values_of(argument: &clap::Arg, long: &str) -> Values {
    if !argument.get_action().takes_values() {
        return Values::Nothing;
    }
    let choices: Vec<String> = argument
        .get_possible_values()
        .iter()
        .filter(|choice| !choice.is_hide_set())
        .map(|choice| choice.get_name().to_owned())
        .collect();
    if !choices.is_empty() {
        return Values::Choices(choices);
    }
    match long {
        "rule" | "not-rule" => Values::Rules,
        "category" | "not-category" => Values::Families,
        "focus" => Values::Files,
        _ => Values::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue() -> Catalogue {
        Catalogue::new([
            "src/api/client.ts".to_owned(),
            "src/billing/invoice.ts".to_owned(),
            "src/billing/totals.ts".to_owned(),
        ])
    }

    fn labels(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn a_bare_slash_offers_the_commands_in_the_order_help_lists_them() {
        let suggestions = catalogue().suggest("/");
        assert_eq!(labels(&suggestions)[0], "/findings");
        assert_eq!(suggestions.len(), SHOWN);
    }

    #[test]
    fn a_few_letters_of_a_command_reach_it_and_a_prefix_wins() {
        let found = catalogue().suggest("/pl");
        assert_eq!(found[0].label, "/plan");
        assert_eq!(found[0].insert, "/plan ");
        assert_eq!(found[0].from, 0);
        assert_eq!(catalogue().suggest("/cplx")[0].label, "/complexity");
    }

    #[test]
    fn text_that_is_not_a_command_gets_no_menu() {
        assert!(catalogue().suggest("plan").is_empty());
        assert!(catalogue().suggest("").is_empty());
        assert!(catalogue().suggest("/zzzz").is_empty());
    }

    #[test]
    fn flags_come_from_the_command_lines_own_definition() {
        let found = catalogue().suggest("/plan --b");
        assert_eq!(found[0].label, "--budget");
        assert_eq!(found[0].from, 6);
        assert!(!found[0].about.is_empty());
        let everything = catalogue().suggest("/findings --");
        assert!(
            labels(&everything)
                .iter()
                .all(|label| label.starts_with("--"))
        );
        assert!(
            catalogue()
                .suggest("/findings --sev")
                .iter()
                .any(|s| s.label == "--severity")
        );
    }

    #[test]
    fn a_new_word_after_a_command_offers_its_flags() {
        let found = catalogue().suggest("/plan ");
        assert!(found.iter().all(|s| s.label.starts_with("--")), "{found:?}");
        assert_eq!(found[0].from, 6);
    }

    #[test]
    fn the_flags_every_command_shares_are_not_offered_on_each() {
        let found = catalogue().suggest("/plan --no-");
        assert!(found.iter().all(|s| s.label != "--no-tui"), "{found:?}");
    }

    #[test]
    fn a_flag_that_takes_a_choice_offers_the_choices() {
        let found = catalogue().suggest("/findings --severity ");
        assert_eq!(labels(&found), ["error", "info", "warning"]);
        let narrowed = catalogue().suggest("/findings --severity w");
        assert_eq!(labels(&narrowed), ["warning"]);
        assert_eq!(narrowed[0].from, 21);
    }

    #[test]
    fn rule_flags_offer_rule_slugs() {
        let found = catalogue().suggest("/findings --rule empty-c");
        assert!(found.iter().any(|s| s.label == "empty-catch"), "{found:?}");
        let excluded = catalogue().suggest("/findings --not-rule empty-c");
        assert!(excluded.iter().any(|s| s.label == "empty-catch"));
    }

    #[test]
    fn family_flags_offer_families() {
        let found = catalogue().suggest("/findings --category ");
        assert_eq!(found.len(), SHOWN.min(Category::ALL.len()));
    }

    #[test]
    fn graph_focus_offers_the_files_the_scan_found() {
        let found = catalogue().suggest("/graph --focus bill");
        assert_eq!(
            labels(&found),
            ["src/billing/invoice.ts", "src/billing/totals.ts"]
        );
        let scattered = catalogue().suggest("/graph --focus apicl");
        assert_eq!(labels(&scattered), ["src/api/client.ts"]);
    }

    #[test]
    fn explain_takes_a_rule_straight_after_its_name() {
        let found = catalogue().suggest("/explain unused-ex");
        assert_eq!(found[0].label, "unused-export");
        assert_eq!(found[0].from, 9);
    }

    #[test]
    fn watch_offers_on_and_off() {
        assert_eq!(labels(&catalogue().suggest("/watch ")), ["off", "on"]);
    }

    #[test]
    fn a_command_typed_in_full_changes_nothing_when_accepted() {
        let found = catalogue().suggest("/plan");
        assert!(found[0].changes_nothing());
        let partial = catalogue().suggest("/pla");
        assert!(!partial[0].changes_nothing());
    }

    #[test]
    fn the_findings_alias_and_scan_complete_the_same_flags() {
        assert!(
            catalogue()
                .suggest("/findings --fix")
                .iter()
                .any(|s| s.label == "--fixable")
        );
    }

    #[test]
    fn every_offered_flag_is_one_clap_would_accept() {
        let parser = crate::parser();
        for name in TWINS {
            let command = parser.find_subcommand(subcommand_of(name)).unwrap();
            let offered = catalogue().suggest(&format!("/{name} --"));
            for suggestion in offered {
                let long = suggestion.label.trim_start_matches("--");
                assert!(
                    command
                        .get_arguments()
                        .any(|arg| arg.get_long() == Some(long)),
                    "/{name} offered --{long}"
                );
            }
        }
    }
}
