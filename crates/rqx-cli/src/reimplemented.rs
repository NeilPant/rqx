//! Functions added since a base revision that repeat functions the base already had.
//!
//! The comparison reuses the clone detector's fingerprints of function bodies. Only a
//! function with no counterpart at the base counts as added, and only a function that
//! exists at both revisions counts as something to call instead: a function that was
//! renamed or moved is not a copy of the one it replaced.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use rqx_core::Language;
use rqx_core::finding::{Evidence, Finding, Likeness, SourceRange};
use rqx_core::rule::RuleId;
use rqx_lang::{BodyPrint, FileShape};

use crate::scan::{PORT_MIN_NODES, PORT_MIN_STATEMENTS};

type Shapes = BTreeMap<Arc<str>, FileShape>;

struct Existing {
    name: String,
    range: SourceRange,
    language: Language,
    print: BodyPrint,
}

/// The functions of the base revision.
pub struct Reference {
    named: Vec<Existing>,
    /// Every named function the base had, with or without a comparable body.
    names: HashSet<(Arc<str>, String)>,
}

impl Reference {
    pub fn of(shapes: &Shapes) -> Self {
        let mut named = Vec::new();
        let mut names = HashSet::new();
        for (file, shape) in shapes {
            let language = language_of(file);
            for function in &shape.functions {
                let Some(name) = &function.name else { continue };
                names.insert((Arc::clone(file), name.clone()));
                if let (Some(print), Some(language)) = (function.body, language) {
                    named.push(Existing {
                        name: name.clone(),
                        range: function.range.clone(),
                        language,
                        print,
                    });
                }
            }
        }
        Self { named, names }
    }

    fn had(&self, file: &Arc<str>, name: &str) -> bool {
        self.names.contains(&(Arc::clone(file), name.to_owned()))
    }
}

/// The functions of the working tree that repeat a function of the base, which is
/// still there to call.
pub fn findings(head: &Shapes, reference: &Reference) -> Vec<Finding> {
    let survivors = Survivors::of(head, reference);
    let mut found = Vec::new();
    for (file, shape) in head {
        let Some(language) = language_of(file) else {
            continue;
        };
        for function in &shape.functions {
            let (Some(name), Some(print)) = (&function.name, function.body) else {
                continue;
            };
            if reference.had(file, name) {
                continue;
            }
            if let Some((existing, likeness)) = survivors.closest(print, language) {
                found.push(finding(name, function.range.clone(), existing, likeness));
            }
        }
    }
    found
}

/// The base functions that the working tree still has, indexed by their fingerprints.
struct Survivors<'a> {
    by_hash: HashMap<u64, Vec<&'a Existing>>,
    by_shape: HashMap<(u64, u64), Vec<&'a Existing>>,
}

impl<'a> Survivors<'a> {
    fn of(head: &Shapes, reference: &'a Reference) -> Self {
        let kept: HashSet<(&str, &str)> = head
            .iter()
            .flat_map(|(file, shape)| {
                shape
                    .functions
                    .iter()
                    .filter_map(move |function| Some((&**file, function.name.as_deref()?)))
            })
            .collect();
        let mut by_hash: HashMap<u64, Vec<&Existing>> = HashMap::new();
        let mut by_shape: HashMap<(u64, u64), Vec<&Existing>> = HashMap::new();
        for existing in &reference.named {
            if !kept.contains(&(&*existing.range.file, existing.name.as_str())) {
                continue;
            }
            by_hash
                .entry(existing.print.hash)
                .or_default()
                .push(existing);
            if is_portable(existing.print) {
                by_shape
                    .entry((existing.print.shape, existing.print.literals))
                    .or_default()
                    .push(existing);
            }
        }
        Self { by_hash, by_shape }
    }

    fn closest(&self, print: BodyPrint, language: Language) -> Option<(&'a Existing, Likeness)> {
        if let Some(existing) = self.by_hash.get(&print.hash).and_then(|all| all.first()) {
            return Some((existing, Likeness::Identical));
        }
        if !is_portable(print) {
            return None;
        }
        let same_logic = self.by_shape.get(&(print.shape, print.literals))?;
        let same_family = same_logic
            .iter()
            .find(|existing| existing.language.family() == language.family());
        match same_family {
            Some(existing) => Some((existing, Likeness::SameLogic)),
            None => same_logic
                .first()
                .map(|existing| (*existing, Likeness::Ported)),
        }
    }
}

/// Matching on shape alone, as the cross-language rule does, needs a body large enough
/// and constants specific enough that two unrelated bodies do not share them.
fn is_portable(print: BodyPrint) -> bool {
    print.statements >= PORT_MIN_STATEMENTS && print.nodes >= PORT_MIN_NODES && print.literals != 0
}

fn language_of(file: &str) -> Option<Language> {
    Language::of_path(std::path::Path::new(file))
}

fn finding(added: &str, location: SourceRange, existing: &Existing, likeness: Likeness) -> Finding {
    let place = format!("{}:{}", existing.range.file, existing.range.line);
    let fix_hint = match likeness {
        Likeness::Identical | Likeness::SameLogic => format!(
            "call `{}` in {place} instead of keeping a second copy",
            existing.name
        ),
        Likeness::Ported => format!(
            "`{}` in {place} is written in another language; give the logic one home and \
             share it, or record why the two differ",
            existing.name
        ),
    };
    Finding {
        rule: RuleId::ReimplementedExisting,
        location,
        severity: RuleId::ReimplementedExisting.spec().severity,
        confidence: likeness.confidence(),
        evidence: Evidence::Reimplemented {
            added: added.to_owned(),
            existing: existing.name.clone(),
            existing_file: existing.range.file.to_string(),
            existing_line: existing.range.line,
            likeness,
        },
        fix_hint,
        related: vec![existing.range.clone()],
        known: false,
    }
}
