//! What a file declares for other files to use: the names it binds outside any
//! function, and the signature of every named function. `rqx fix` compares the
//! outline before and after an edit, because one file's re-analysis cannot see a
//! caller or a reader in another file.

use std::collections::{BTreeMap, BTreeSet};

use rqx_core::Language;
use tree_sitter::Node;

use crate::profile::LanguageProfile;
use crate::scope::BindingKind;
use crate::tree::{descendants, text_of};
use crate::{AnalysisError, support};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outline {
    /// Names bound outside any function, with how many times each is bound. A name
    /// a file binds twice still exists after one of the two goes.
    pub declared: BTreeMap<String, u32>,
    /// Function headers by name, the receiver included for a method. A list because
    /// overloads share a name.
    pub signatures: BTreeMap<String, Vec<String>>,
}

impl Outline {
    pub fn of(language: Language, markup: bool, source: &str) -> Result<Self, AnalysisError> {
        let support = support(language);
        let tree = crate::parse(language, support.profile, markup, source)?;
        Ok(Self::of_tree(support.profile, source, tree.root_node()))
    }

    fn of_tree(profile: &'static LanguageProfile, source: &str, root: Node<'_>) -> Self {
        let scopes = crate::scope::resolve(profile, source, root);
        let mut declared: BTreeMap<String, u32> = BTreeMap::new();
        for binding in &scopes.bindings {
            if binding.kind != BindingKind::Parameter && !scopes.is_local(binding.scope) {
                *declared.entry(binding.name.clone()).or_default() += 1;
            }
        }

        let mut signatures: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for function in descendants(root)
            .into_iter()
            .filter(|node| profile.is_function(*node))
        {
            let Some(name) = function.child_by_field_name("name") else {
                continue;
            };
            let key = match function.child_by_field_name("receiver") {
                Some(receiver) => format!(
                    "{} {}",
                    squashed(text_of(receiver, source)),
                    text_of(name, source)
                ),
                None => text_of(name, source).to_owned(),
            };
            signatures
                .entry(key)
                .or_default()
                .push(header(function, source));
        }
        for headers in signatures.values_mut() {
            headers.sort();
        }
        Self {
            declared,
            signatures,
        }
    }

    /// Names this outline binds that `after` binds fewer times, or not at all.
    pub fn removed(&self, after: &Outline) -> Vec<String> {
        self.declared
            .iter()
            .filter(|(name, count)| after.declared.get(*name).is_none_or(|left| left < count))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Functions present on both sides whose header changed, by name.
    pub fn changed_signatures(&self, after: &Outline) -> Vec<String> {
        let mut changed = BTreeSet::new();
        for (key, headers) in &self.signatures {
            if after.signatures.get(key).is_some_and(|now| now != headers) {
                changed.insert(key.rsplit(' ').next().unwrap_or(key).to_owned());
            }
        }
        changed.into_iter().collect()
    }
}

/// Everything a function says before its body: name, parameters, results.
fn header(function: Node<'_>, source: &str) -> String {
    let end = function
        .child_by_field_name("body")
        .map_or(function.end_byte(), |body| body.start_byte());
    squashed(source.get(function.start_byte()..end).unwrap_or_default())
}

fn squashed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_level_var_is_declared_and_a_local_is_not() {
        let source = "package p\n\nvar hook func()\n\nfunc f() {\n\tlocal := 1\n\t_ = local\n}\n";
        let outline = Outline::of(Language::Go, false, source).expect("parses");
        assert!(outline.declared.contains_key("hook"), "{outline:?}");
        assert!(outline.declared.contains_key("f"), "{outline:?}");
        assert!(!outline.declared.contains_key("local"), "{outline:?}");
    }

    #[test]
    fn a_dropped_result_is_a_changed_signature() {
        let before =
            "package p\n\nfunc (c *C) Get(name string) (Func, bool) {\n\treturn nil, false\n}\n";
        let after = "package p\n\nfunc (c *C) Get(name string) () {\n\treturn nil, false\n}\n";
        let before = Outline::of(Language::Go, false, before).unwrap();
        let after = Outline::of(Language::Go, false, after).unwrap();
        assert_eq!(before.changed_signatures(&after), ["Get"]);
        assert!(before.removed(&after).is_empty());
    }

    #[test]
    fn removing_a_package_level_var_is_a_removed_name() {
        let before = "package p\n\nvar hook func()\n";
        let after = "package p\n";
        let before = Outline::of(Language::Go, false, before).unwrap();
        let after = Outline::of(Language::Go, false, after).unwrap();
        assert_eq!(before.removed(&after), ["hook"]);
    }
}
