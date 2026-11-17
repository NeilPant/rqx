//! TOML text in, YAML text out, with the comments carried over.
//!
//! The TOML is read with `toml_edit` because it is the parser that keeps the comment a
//! writer put above a key. Everything else about the document is rebuilt from its
//! structure, and the result is parsed back and compared with the original before
//! anything is handed out: a migration that quietly changed a threshold would change
//! what a pipeline gates on.

use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use serde_json::Value as Tree;
use toml_edit::{Array, DocumentMut, Item, RawString, Table, Value};

const STEP: usize = 2;
const FLOW_WIDTH: usize = 78;

/// The key a section was called in TOML and what YAML calls it. `[[override]]` becomes
/// `overrides:` because a list is plural, and so is its name in the YAML schema.
const RENAMED: [(&str, &str); 1] = [("override", "overrides")];

pub fn convert(toml_text: &str) -> Result<String> {
    let document: DocumentMut = toml_text.parse().context("it is not valid TOML")?;
    let mut out = String::new();
    write_mapping(document.as_table(), 0, true, &mut out);
    write_comments(Some(document.trailing()), 0, true, &mut out);
    verify(toml_text, &out)?;
    Ok(out)
}

fn write_mapping(table: &Table, indent: usize, top: bool, out: &mut String) {
    for (name, item) in table.iter() {
        let Some((key, _)) = table.get_key_value(name) else {
            continue;
        };
        let name = if top { renamed(name) } else { name };
        write_entry(name, key.leaf_decor().prefix(), item, indent, out);
    }
}

fn renamed(name: &str) -> &str {
    RENAMED
        .iter()
        .find(|(old, _)| *old == name)
        .map_or(name, |(_, new)| new)
}

fn write_entry(name: &str, lead: Option<&RawString>, item: &Item, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    let key = scalar(name);
    match item {
        Item::Value(value) => {
            write_comments(lead, indent, true, out);
            write_value(&key, value, indent, out);
        }
        Item::Table(table) => {
            write_comments(table.decor().prefix(), indent, true, out);
            write_comments(lead, indent, true, out);
            let trailing = trailing_comment(table.decor().suffix());
            if table.is_empty() {
                let _ = writeln!(out, "{pad}{key}: {{}}{trailing}");
            } else {
                let _ = writeln!(out, "{pad}{key}:{trailing}");
                write_mapping(table, indent + STEP, false, out);
            }
        }
        Item::ArrayOfTables(tables) => {
            let _ = writeln!(out, "{pad}{key}:");
            for table in tables {
                write_sequence_mapping(table, indent + STEP, out);
            }
        }
        Item::None => {}
    }
}

fn write_value(key: &str, value: &Value, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    let trailing = trailing_comment(value.decor().suffix());
    match value {
        Value::Array(array) if array.is_empty() => {
            let _ = writeln!(out, "{pad}{key}: []{trailing}");
        }
        Value::Array(array) if flows(array) => {
            let _ = writeln!(out, "{pad}{key}: {}{trailing}", flow(array));
        }
        Value::Array(array) => {
            let _ = writeln!(out, "{pad}{key}:{trailing}");
            write_sequence(array, indent + STEP, out);
        }
        Value::InlineTable(inline) => {
            let table = inline.clone().into_table();
            if table.is_empty() {
                let _ = writeln!(out, "{pad}{key}: {{}}{trailing}");
            } else {
                let _ = writeln!(out, "{pad}{key}:{trailing}");
                write_mapping(&table, indent + STEP, false, out);
            }
        }
        scalar_value => write_scalar_entry(&pad, key, scalar_value, &trailing, out),
    }
}

fn write_scalar_entry(pad: &str, key: &str, value: &Value, trailing: &str, out: &mut String) {
    let rendered = scalar_of(value);
    match rendered.strip_prefix(['|', '>']) {
        Some(_) => {
            let mut lines = rendered.lines();
            let header = lines.next().unwrap_or("|");
            let _ = writeln!(out, "{pad}{key}: {header}{trailing}");
            for line in lines {
                let body = line.strip_prefix(&" ".repeat(STEP)).unwrap_or(line);
                if body.is_empty() {
                    out.push('\n');
                } else {
                    let _ = writeln!(out, "{pad}{}{body}", " ".repeat(STEP));
                }
            }
        }
        None => {
            let _ = writeln!(out, "{pad}{key}: {rendered}{trailing}");
        }
    }
}

/// A list that fits on one line and carries nothing a line cannot hold. The comment a
/// writer put above one element is a reason to give it a line of its own.
fn flows(array: &Array) -> bool {
    let plain = array.iter().all(|element| {
        let scalar = !matches!(element, Value::InlineTable(_) | Value::Array(_));
        let single = !scalar_of_inline(element).contains('\n');
        scalar && single && comments_in(element.decor().prefix()).is_empty()
    });
    plain && comments_in(Some(array.trailing())).is_empty() && flow(array).len() <= FLOW_WIDTH
}

fn flow(array: &Array) -> String {
    let items: Vec<String> = array.iter().map(scalar_of_inline).collect();
    format!("[{}]", items.join(", "))
}

fn write_sequence(array: &Array, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for element in array {
        write_comments(element.decor().prefix(), indent, false, out);
        match element {
            Value::InlineTable(inline) => {
                write_sequence_mapping(&inline.clone().into_table(), indent, out);
            }
            Value::Array(_) => {
                let _ = writeln!(out, "{pad}- {}", scalar_of_inline(element));
            }
            scalar_value => {
                let _ = writeln!(out, "{pad}- {}", scalar_of(scalar_value).replace('\n', " "));
            }
        }
    }
    write_comments(Some(array.trailing()), indent, false, out);
}

/// One mapping as a list entry. The body is written at the entry's own depth and its
/// first key then takes the dash's place, which keeps every nested key aligned without
/// tracking how far in the dash pushed it.
fn write_sequence_mapping(table: &Table, indent: usize, out: &mut String) {
    write_comments(table.decor().prefix(), indent, true, out);
    let mut body = String::new();
    write_mapping(table, indent + STEP, false, &mut body);
    if body.is_empty() {
        let _ = writeln!(out, "{}- {{}}", " ".repeat(indent));
        return;
    }
    let mut dashed = false;
    for line in body.lines() {
        let is_content = !line.trim_start().starts_with('#') && !line.trim().is_empty();
        if !dashed && is_content {
            let _ = writeln!(out, "{}- {}", " ".repeat(indent), line.trim_start());
            dashed = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
}

fn scalar_of(value: &Value) -> String {
    match value {
        Value::String(text) => scalar(text.value()),
        Value::Integer(number) => number.value().to_string(),
        Value::Float(number) => float(*number.value()),
        Value::Boolean(flag) => flag.value().to_string(),
        Value::Datetime(moment) => scalar(&moment.value().to_string()),
        Value::Array(_) | Value::InlineTable(_) => scalar_of_inline(value),
    }
}

/// A value on a single line, nested containers in flow style.
fn scalar_of_inline(value: &Value) -> String {
    match value {
        Value::Array(array) => {
            let items: Vec<String> = array.iter().map(scalar_of_inline).collect();
            format!("[{}]", items.join(", "))
        }
        Value::InlineTable(table) => {
            let entries: Vec<String> = table
                .iter()
                .map(|(name, item)| format!("{}: {}", scalar(name), scalar_of_inline(item)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        other => scalar_of(other).replace('\n', " "),
    }
}

fn float(number: f64) -> String {
    if number.is_nan() {
        ".nan".to_owned()
    } else if number.is_infinite() {
        if number > 0.0 { ".inf" } else { "-.inf" }.to_owned()
    } else if number.fract() == 0.0 {
        format!("{number:.1}")
    } else {
        number.to_string()
    }
}

/// A string in the form YAML reads back as the same string: the serializer quotes what
/// would otherwise read as a number, a boolean or an alias, which a hand-written rule
/// for "does this need quotes" always gets wrong for some word.
fn scalar(text: &str) -> String {
    let rendered = serde_saphyr::to_string(&text).unwrap_or_else(|_| format!("{text:?}"));
    rendered.trim_end_matches('\n').to_owned()
}

fn comments_in(raw: Option<&RawString>) -> Vec<String> {
    raw.and_then(RawString::as_str)
        .map(|text| {
            text.split_inclusive('\n')
                .filter(|line| line.ends_with('\n'))
                .map(str::trim)
                .filter(|line| line.starts_with('#'))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The comment and blank lines a writer left above something; a list entry keeps its
/// comment but not the blank line TOML put after the opening bracket. Only a segment ending in
/// a newline is a line of its own: the text after the last one is the indentation of
/// whatever follows.
fn write_comments(raw: Option<&RawString>, indent: usize, blanks: bool, out: &mut String) {
    let Some(text) = raw.and_then(RawString::as_str) else {
        return;
    };
    let pad = " ".repeat(indent);
    for line in text
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n'))
    {
        let line = line.trim();
        if line.starts_with('#') {
            let _ = writeln!(out, "{pad}{line}");
        } else if blanks && line.is_empty() && !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
}

fn trailing_comment(raw: Option<&RawString>) -> String {
    raw.and_then(RawString::as_str)
        .map(str::trim)
        .filter(|text| text.starts_with('#'))
        .map_or_else(String::new, |text| format!(" {text}"))
}

/// The migrated text has to mean what the original did, or nothing is written.
fn verify(toml_text: &str, yaml: &str) -> Result<()> {
    let mut original: Tree =
        toml::from_str(toml_text).context("reading the TOML back for comparison")?;
    if let Some(table) = original.as_object_mut() {
        for (old, new) in RENAMED {
            if let Some(entries) = table.remove(old) {
                table.insert(new.to_owned(), entries);
            }
        }
    }
    let migrated: Tree = match serde_saphyr::from_str(yaml) {
        Ok(Tree::Null) => Tree::Object(serde_json::Map::new()),
        Ok(tree) => tree,
        Err(error) => bail!("the YAML it wrote does not parse: {error}"),
    };
    if original != migrated {
        bail!("the YAML it wrote does not say what the TOML said, so nothing was converted");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated(toml_text: &str) -> String {
        convert(toml_text).unwrap_or_else(|error| panic!("{error:#}"))
    }

    #[test]
    fn a_table_becomes_a_mapping_and_keeps_the_comment_above_it() {
        let out = migrated("# how strict\n[thresholds]\n# how many is too many\nmax_params = 4\n");
        assert_eq!(
            out,
            "# how strict\nthresholds:\n  # how many is too many\n  max_params: 4\n"
        );
    }

    #[test]
    fn a_comment_after_a_value_stays_on_its_line() {
        let out = migrated("[gate]\nfail_on = \"error\" # CI only\n");
        assert_eq!(out, "gate:\n  fail_on: error # CI only\n");
    }

    #[test]
    fn override_sections_become_a_list_called_overrides() {
        let out = migrated(
            "[[override]]\npaths = [\"legacy/**\"]\n[override.thresholds]\nmax_params = 9\n\n[[override]]\npaths = [\"scripts/**\"]\nlevel = \"relaxed\"\n",
        );
        assert_eq!(
            out,
            "overrides:\n  - paths: [legacy/**]\n    thresholds:\n      max_params: 9\n\n  - paths: [scripts/**]\n    level: relaxed\n"
        );
    }

    #[test]
    fn a_long_list_is_written_one_entry_to_a_line() {
        let out = migrated(
            "[scan]\nprune_dirs = [\"node_modules\", \"dist\", \"build\", \"out\", \"coverage\", \".git\", \"vendor\", \"__pycache__\", \".venv\", \"venv\", \".tox\", \".mypy_cache\"]\n",
        );
        assert!(
            out.contains("  prune_dirs:\n    - node_modules\n    - dist\n"),
            "{out}"
        );
    }

    #[test]
    fn a_comment_above_one_list_entry_survives() {
        let out = migrated(
            "[ignore]\npatterns = [\n  # generated\n  \"**/*.gen.ts\",\n  \"**/*.pb.go\",\n]\n",
        );
        assert!(
            out.contains("    # generated\n    - \"**/*.gen.ts\"\n"),
            "{out}"
        );
    }

    #[test]
    fn strings_that_yaml_would_misread_are_quoted() {
        let out = migrated("a = \"yes\"\nb = \"1\"\nc = \"**/*.d.ts\"\n");
        assert!(out.contains("a: \"yes\""), "{out}");
        assert!(out.contains("b: \"1\""), "{out}");
        assert!(out.contains("c: \"**/*.d.ts\""), "{out}");
    }

    #[test]
    fn a_multi_line_string_becomes_a_block_scalar() {
        let out = migrated("[[rule]]\nquery = '''\n(call_expression) @call\n'''\nid = \"x\"\n");
        assert!(
            out.contains("  - query: |\n      (call_expression) @call\n    id: x\n"),
            "{out}"
        );
    }

    #[test]
    fn an_empty_table_and_an_empty_list_are_kept() {
        let out = migrated("[ignore]\n[entrypoints]\npaths = []\n");
        assert!(out.contains("ignore: {}"), "{out}");
        assert!(out.contains("paths: []"), "{out}");
    }

    #[test]
    fn glob_keys_in_the_architecture_table_are_quoted() {
        let out = migrated("[architecture.paths.\"src/domain/**\"]\nlayer = \"domain\"\n");
        assert!(out.contains("src/domain/**:"), "{out}");
        verify(
            "[architecture.paths.\"src/domain/**\"]\nlayer = \"domain\"\n",
            &out,
        )
        .unwrap();
    }

    #[test]
    fn a_trailing_comment_at_the_end_of_the_file_is_kept() {
        let out = migrated("root = true\n# the end\n");
        assert!(out.ends_with("# the end\n"), "{out}");
    }

    #[test]
    fn text_that_is_not_toml_is_refused() {
        assert!(convert("[gate\nfail_on = ").is_err());
    }
}
