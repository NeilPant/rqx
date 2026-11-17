//! The one edit an editor's quick fix makes to a config file: disabling a rule.

/// A change to the config as a position and the text to put there, so an editor can
/// apply it as a `WorkspaceEdit` rather than being handed a rewritten file.
pub struct Insertion {
    /// Zero-based.
    pub line: u32,
    pub column: u32,
    pub text: String,
}

/// Applies an insertion the way an editor would, padding with empty lines when it
/// lands past the end of the file. Only the tests apply one: a real client is the
/// editor, and it applies the edit itself.
#[cfg(test)]
fn apply(source: &str, insertion: &Insertion) -> String {
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let line = insertion.line as usize;
    while lines.len() <= line {
        lines.push(String::new());
    }
    let column = (insertion.column as usize).min(lines[line].len());
    lines[line].insert_str(column, &insertion.text);
    let mut out = lines.join("\n");
    if source.ends_with('\n') || source.is_empty() {
        out.push('\n');
    }
    out
}

/// The lines of the top-level `rules:` mapping, found by indentation. Anything but a
/// plain block mapping — a flow `rules: {…}`, a second `rules:` — is not read, since
/// a wrong edit leaves the project with a config that no longer parses.
struct RulesBlock {
    header: usize,
    /// Line numbers of everything indented under the header.
    body: Vec<usize>,
}

fn rules_block(lines: &[&str]) -> Option<Option<RulesBlock>> {
    let mut headers = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("rules:"))
        .map(|(number, _)| number);
    let Some(header) = headers.next() else {
        return Some(None);
    };
    if headers.next().is_some() || lines[header].trim_end() != "rules:" {
        return None;
    }
    let body = (header + 1..lines.len())
        .take_while(|number| {
            let line = lines[*number];
            line.trim().is_empty() || line.starts_with(' ') || line.trim_start().starts_with('#')
        })
        .filter(|number| !lines[*number].trim().is_empty())
        .collect();
    Some(Some(RulesBlock { header, body }))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn names_in(list: &str) -> impl Iterator<Item = &str> {
    list.split(',')
        .map(|entry| entry.trim().trim_matches(['"', '\'']))
        .filter(|entry| !entry.is_empty())
}

/// Adds `name` to `rules.disabled`: into the list when there is one, as a new key
/// under an existing `rules:`, or as the whole mapping — in that order. A shape it
/// cannot read safely yields nothing, as does a rule that is already disabled.
pub fn disable_entry(source: &str, name: &str) -> Option<Insertion> {
    let lines: Vec<&str> = source.lines().collect();
    let Some(rules) = rules_block(&lines)? else {
        return Some(append_rules(source, &lines, name));
    };
    let child_indent = rules
        .body
        .first()
        .map_or(2, |number| indent_of(lines[*number]));
    let disabled = rules.body.iter().copied().find(|number| {
        indent_of(lines[*number]) == child_indent
            && lines[*number].trim_start().starts_with("disabled:")
    });
    match disabled {
        None => Some(Insertion {
            line: rules.header as u32 + 1,
            column: 0,
            text: format!("{}disabled: [{name}]\n", " ".repeat(child_indent)),
        }),
        Some(number) => extend_list(&lines, &rules, number, child_indent, name),
    }
}

fn extend_list(
    lines: &[&str],
    rules: &RulesBlock,
    at: usize,
    child_indent: usize,
    name: &str,
) -> Option<Insertion> {
    let line = lines[at];
    let value = line.trim_start().strip_prefix("disabled:")?;
    let value = value.split('#').next().unwrap_or("").trim();

    if let Some(inside) = value.strip_prefix('[') {
        let entries = inside.strip_suffix(']')?;
        if names_in(entries).any(|entry| entry == name) {
            return None;
        }
        let open = line.find('[')? + 1;
        let text = if entries.trim().is_empty() {
            name.to_owned()
        } else {
            format!("{name}, ")
        };
        return Some(Insertion {
            line: at as u32,
            column: open as u32,
            text,
        });
    }
    if !value.is_empty() {
        return None;
    }

    let items: Vec<usize> = rules
        .body
        .iter()
        .copied()
        .filter(|number| *number > at)
        .take_while(|number| {
            indent_of(lines[*number]) > child_indent
                || lines[*number].trim_start().starts_with("- ")
        })
        .filter(|number| lines[*number].trim_start().starts_with("- "))
        .collect();
    let last = *items.last()?;
    if items.iter().any(|number| {
        lines[*number].trim_start()[2..]
            .trim()
            .trim_matches(['"', '\''])
            == name
    }) {
        return None;
    }
    Some(Insertion {
        line: last as u32 + 1,
        column: 0,
        text: format!("{}- {name}\n", " ".repeat(indent_of(lines[last]))),
    })
}

fn append_rules(source: &str, lines: &[&str], name: &str) -> Insertion {
    let separator = if source.is_empty() || source.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Insertion {
        line: lines.len() as u32,
        column: 0,
        text: format!("{separator}\nrules:\n  disabled: [{name}]\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the edit the way a client would, then parses the result — an edit that
    /// produces a file rqx cannot read is worse than no quick fix at all.
    fn applied(source: &str, name: &str) -> String {
        let insertion = disable_entry(source, name).expect("an edit");
        let out = apply(source, &insertion);
        let tree: serde_json::Value = serde_saphyr::from_str(&out)
            .unwrap_or_else(|error| panic!("the edit broke the file: {error}\n{out}"));
        let disabled = &tree["rules"]["disabled"];
        assert!(
            disabled
                .as_array()
                .is_some_and(|list| list.iter().any(|entry| entry == name)),
            "{name} is not in {disabled}\n{out}"
        );
        out
    }

    #[test]
    fn an_empty_list_gains_the_first_entry() {
        let out = applied("rules:\n  disabled: []\n", "magic-number");
        assert!(out.contains("disabled: [magic-number]"), "{out}");
    }

    #[test]
    fn a_populated_list_keeps_what_was_there() {
        let out = applied("rules:\n  disabled: [long-function]\n", "magic-number");
        assert!(
            out.contains("disabled: [magic-number, long-function]"),
            "{out}"
        );
    }

    #[test]
    fn a_block_list_gains_a_line() {
        let out = applied(
            "rules:\n  disabled:\n    - long-function\n  enabled: []\n",
            "magic-number",
        );
        assert!(
            out.contains("    - long-function\n    - magic-number\n  enabled"),
            "{out}"
        );
    }

    /// The template writes a line of prose above every key, so an edit that dropped
    /// comments would silently delete the explanation of the setting being changed.
    #[test]
    fn comments_survive_the_edit() {
        let source = "# why this table exists\nrules:\n  # slugs or families\n  disabled: []\n";
        let out = applied(source, "magic-number");
        assert!(out.contains("# why this table exists"), "{out}");
        assert!(out.contains("# slugs or families"), "{out}");
    }

    #[test]
    fn a_rules_mapping_without_the_key_gains_it() {
        let out = applied("rules:\n  enabled: [tiny-switch]\n", "magic-number");
        assert!(out.contains("  disabled: [magic-number]"), "{out}");
        assert!(out.contains("enabled: [tiny-switch]"), "{out}");
    }

    #[test]
    fn a_file_without_the_mapping_gains_one() {
        let out = applied("scan:\n  threads: 4\n", "magic-number");
        assert!(out.contains("rules:\n  disabled: [magic-number]"), "{out}");
    }

    #[test]
    fn a_rule_already_disabled_is_not_added_twice() {
        assert!(disable_entry("rules:\n  disabled: [magic-number]\n", "magic-number").is_none());
        assert!(
            disable_entry("rules:\n  disabled:\n    - magic-number\n", "magic-number").is_none()
        );
    }

    /// The name appearing anywhere in the file used to count as already-disabled, so a
    /// commented-out example silently swallowed the quick fix.
    #[test]
    fn a_name_mentioned_elsewhere_does_not_count_as_disabled() {
        let source = "# e.g. disabled: [magic-number]\nrules:\n  disabled: []\n";
        let out = applied(source, "magic-number");
        assert!(out.contains("  disabled: [magic-number]"), "{out}");
    }

    #[test]
    fn a_flow_mapping_is_left_alone_rather_than_guessed_at() {
        assert!(disable_entry("rules: { enabled: [] }\n", "x").is_none());
    }

    #[test]
    fn a_second_rules_mapping_is_refused() {
        assert!(disable_entry("rules:\n  enabled: []\nscan: {}\nrules:\n", "x").is_none());
    }

    #[test]
    fn a_file_with_no_trailing_newline_keeps_its_last_line() {
        let out = applied("scan:\n  threads: 4", "magic-number");
        assert!(out.contains("threads: 4"), "{out}");
        assert!(out.contains("rules:"), "{out}");
    }

    #[test]
    fn an_empty_file_gains_the_mapping() {
        applied("", "magic-number");
    }
}
