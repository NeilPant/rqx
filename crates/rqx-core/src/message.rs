//! The sentence a reader sees for a finding. It is written per rule from the evidence
//! the rule recorded, so it can name the subject, the count and the position that the
//! evidence's own text leaves out.
//!
//! The evidence keeps its `Display` text, which feeds finding identities (and so the
//! fingerprints in GitLab and SARIF reports) and the `perf` output. A rule with no
//! sentence here falls back to that text.

use crate::finding::{Evidence, Finding, plural};

impl Finding {
    /// What this finding says about the code, naming the subject and its numbers.
    pub fn message(&self) -> String {
        let slug = self.slug();
        let said = match &self.evidence {
            Evidence::Name { identifier, .. } => named(slug, identifier),
            Evidence::Breadth { found, limit, .. } => counted(slug, *found, *limit),
            Evidence::BranchShape { shape } => shaped(slug, shape),
            Evidence::InLoop { subject, depth, .. } => in_loop(slug, subject, *depth),
            Evidence::Recursion { calls, .. } => recursive(slug, *calls),
            Evidence::ImpreciseType { construct } => imprecise(construct),
            Evidence::MagicNumber {
                literal,
                occurrences,
            } => Some(magic_number(literal, *occurrences)),
            Evidence::ClassShape {
                fields,
                methods,
                trivial,
                ..
            } => class_shape(slug, *fields, *methods, *trivial),
            Evidence::Noise { marker } => noise(slug, marker),
            Evidence::ImportCycle { ring, .. } if !ring.is_empty() => {
                Some(format!("Import cycle: {}.", ring.join(" \u{2192} ")))
            }
            Evidence::TestDirective { directive } => Some(directive_effect(slug, directive)),
            Evidence::MutatedParameter { name } => Some(format!(
                "Parameter `{name}` is assigned a new value inside the function."
            )),
            Evidence::Delegation { delegating, total } => Some(format!(
                "{} of {} only {} the call to another object.",
                delegating,
                plural(*total, "method", "methods"),
                if *delegating == 1 {
                    "forwards"
                } else {
                    "forward"
                }
            )),
            Evidence::MixedConventions { minority, count } => Some(format!(
                "{} in this file {} {minority} while the rest do not.",
                plural(*count, "name", "names"),
                if *count == 1 { "uses" } else { "use" }
            )),
            _ => None,
        };
        said.unwrap_or_else(|| self.evidence.to_string())
    }
}

/// The first thing written between backticks, which is the subject a shape sentence
/// was built around.
fn quoted(text: &str) -> Option<&str> {
    let start = text.find('`')? + 1;
    let end = start + text[start..].find('`')?;
    Some(&text[start..end])
}

fn named(slug: &str, name: &str) -> Option<String> {
    let name = name.trim();
    Some(match slug {
        "abstract-nothing" => format!(
            "`{name}` is declared abstract but has no abstract member for a subclass to finish."
        ),
        "ambient-dependency" => format!(
            "`{name}` reads the clock, the environment or a random source directly, so no \
             caller can supply the value."
        ),
        "assertionless-test" => {
            format!("`{name}` contains no assertion, so it passes whatever the code does.")
        }
        "boolean-literal-argument" => format!(
            "`{name}` is called with a bare `true` or `false`, so the call does not say what \
             the flag switches."
        ),
        "boolean-not-a-question" => {
            format!("`{name}` holds a boolean, and its name does not read as a yes-or-no question.")
        }
        "closure-over-loop-variable" => format!(
            "A closure created in the loop reads `{name}`, the variable the loop advances, so \
             it can see a later value than the one it was made for."
        ),
        "dead-store" => {
            format!("The value assigned to `{name}` is overwritten before anything reads it.")
        }
        "debug-output-in-test" => {
            format!("`{name}` writes to the terminal inside a test, so every run prints it.")
        }
        "discarded-error" => {
            format!("`{name}` returns a result that is assigned to an ignored name and never read.")
        }
        "docstring-restates-signature" => {
            format!("The docstring of `{name}` only repeats its name and parameters.")
        }
        "dropped-promise" => format!(
            "`{name}` returns an asynchronous result that nothing awaits or handles, so a \
             failure goes unseen."
        ),
        "duplicate-assertion" => format!("`{name}` is asserted more than once in the same test."),
        "duplicate-function" => match name.split_once(" and ") {
            Some((first, second)) => format!("`{first}` and `{second}` have the same body."),
            None => format!("`{name}` has the same body as another function."),
        },
        "duplicate-import" => {
            format!("`{name}` is imported again; an earlier import already brings it in.")
        }
        "empty-method" => format!("`{name}` is declared with an empty body."),
        "empty-subclass" => format!("`{name}` extends its parent without adding a member."),
        "empty-test" => {
            format!("`{name}` has an empty body, so it passes without running a check.")
        }
        "generic-throw" => format!(
            "`{name}` is the root exception type, so a caller cannot catch this failure on its own."
        ),
        "global-accessor" => format!(
            "`{name}` hands every caller the same shared value, so a caller cannot choose its own."
        ),
        "import-time-work" => format!(
            "`{name}` is called at the top level of the file, so it runs when the file is loaded."
        ),
        "library-exit" => format!(
            "`{name}` ends the whole process from library code, so a caller cannot recover."
        ),
        "loop-runs-once" => {
            format!("The loop body always ends with `{name}`, so it never reaches a second pass.")
        }
        "mutable-module-state" => {
            format!("`{name}` is module-level state that any function in the file can change.")
        }
        "mutated-while-iterated" => {
            format!("`{name}` is modified inside the loop that iterates over it.")
        }
        "name-repeats-type" => match name.split_once(" (") {
            Some((whole, suffix)) => format!(
                "`{whole}` ends with `{}`, which repeats its type.",
                suffix.trim_end_matches(')')
            ),
            None => format!("`{name}` spells its type into its name."),
        },
        "negated-name" => match name.split_once(" (") {
            Some((whole, prefix)) => format!(
                "`{whole}` starts with `{}`, so a condition on it reads as a double negative.",
                prefix.trim_end_matches(')')
            ),
            None => format!("`{name}` is named for what it is not."),
        },
        "never-reassigned" => format!(
            "`{name}` is declared with a keyword that allows reassignment, and nothing \
             reassigns it."
        ),
        "orphan-todo" => format!("This `{name}` has no owner and no ticket number."),
        "passthrough-function" => match name.split_once(" to ") {
            Some((function, target)) => {
                format!("`{function}` only calls `{target}` with the same arguments.")
            }
            None => format!("`{name}` only forwards its arguments to another function."),
        },
        "passthrough-override" => {
            format!("`{name}` overrides the parent's method and does nothing but call it.")
        }
        "placeholder-body" => match name.split_once(": ") {
            Some((function, body)) => format!(
                "`{function}` has a placeholder body (`{body}`), so callers get no behaviour."
            ),
            None => format!("`{name}` has a placeholder body, so callers get no behaviour."),
        },
        "plural-mismatch" => {
            format!("The name `{name}` and the value it holds disagree on singular or plural.")
        }
        "real-world-test" => format!(
            "`{name}` reaches something outside the process, so the test depends on it being \
             there."
        ),
        "self-built-collaborators" => {
            format!("The class creates its own `{name}`, so a test cannot give it a different one.")
        }
        "shadows-builtin" => format!("`{name}` has the same name as a built-in and hides it here."),
        "single-method-field" => format!("Only one method reads the field `{name}`."),
        "sleep-in-production" => {
            format!("`{name}` pauses the thread for a fixed time outside a test.")
        }
        "string-shape-assertion" => format!(
            "`{name}` is a serialised structure compared as text, so spacing or key order can \
             fail the test."
        ),
        "unchecked-stub" => format!("`{name}` is a stub or mock that no assertion inspects."),
        "unnamed-test" => format!("The name `{name}` does not say what behaviour the test checks."),
        "unused-fixture" => format!("`{name}` is set up before every case and no test reads it."),
        "vague-function-name" => format!("`{name}` says that something runs, not what it does."),
        "boxing-in-loop" => format!(
            "`{name}` is a boxed number updated on every pass, so each pass allocates a new object."
        ),
        "assign-once-field" => format!(
            "`{name}` is assigned only while the object is built and never changed afterwards."
        ),
        "method-ignores-instance" => {
            format!("`{name}` never uses the instance it belongs to.")
        }
        "loop-concatenation" => {
            format!("`{name}` is rebuilt from its own previous value on every pass.")
        }
        "return-of-fresh-local" => {
            format!("`{name}` is declared on one line and returned on the next.")
        }
        _ => return None,
    })
}

fn counted(slug: &str, found: u32, limit: u32) -> Option<String> {
    let things = |one: &str, many: &str| plural(found, one, many);
    Some(match slug {
        "barrel-module" => format!(
            "All {found} imports are re-exported unchanged, so the file only forwards names."
        ),
        "blanket-try" => format!(
            "One `try` covers {}, so its handler cannot tell which one failed.",
            things("statement", "statements")
        ),
        "wide-try" => format!(
            "One `try` covers {} (limit {limit}), so its handler cannot tell which one failed.",
            things("statement", "statements")
        ),
        "branching-test" => format!(
            "The test contains {}, so which path ran depends on the data.",
            things("branch or loop", "branches or loops")
        ),
        "callback-nesting" => format!("Callbacks nest {found} levels deep (limit {limit})."),
        "deep-relative-import" => {
            format!("This import climbs {found} levels up the directory tree (limit {limit}).")
        }
        "eager-test" => format!(
            "One test drives {found} separate units (limit {limit}), so a failure does not \
             say which broke."
        ),
        "guard-cascade" => format!(
            "{} run before the body starts (limit {limit}).",
            things("check", "checks")
        ),
        "many-collaborators" => format!(
            "The class refers to {} (limit {limit}).",
            things("other type", "other types")
        ),
        "many-exits" => format!(
            "The function has {} (limit {limit}).",
            things("exit point", "exit points")
        ),
        "many-locals" => format!(
            "The function declares {} (limit {limit}).",
            things("local name", "local names")
        ),
        "many-results" => format!(
            "The function returns {} at once (limit {limit}).",
            things("value", "values")
        ),
        "no-default-branch" => format!(
            "The `switch` has {} and no default (limit {limit}).",
            things("case", "cases")
        ),
        "over-logging" => format!(
            "{} in this function are log calls, over {limit}% of its body.",
            things("statement", "statements")
        ),
        "split-cohesion" => format!(
            "The class's methods fall into {found} groups that share no state (limit {limit})."
        ),
        "static-only-class" => format!(
            "All {} are static (limit {limit}), so the class is a namespace.",
            things("method", "methods")
        ),
        "unlabelled-assertions" => {
            format!("{found} assertions in a row say nothing about what each one checks.")
        }
        "wide-fan-out" => format!("This file imports {found} modules (limit {limit})."),
        "wide-switch" => format!(
            "The `switch` has {} (limit {limit}).",
            things("case", "cases")
        ),
        "working-constructor" => format!(
            "The constructor runs {} of real work (limit {limit}).",
            things("statement", "statements")
        ),
        _ => return None,
    })
}

fn shaped(slug: &str, shape: &str) -> Option<String> {
    let code = quoted(shape);
    Some(match (slug, code) {
        ("catch-all-handler", Some(code)) => format!(
            "The handler catches `{code}`, so a failure it was not written for is swallowed too."
        ),
        ("discarded-expression", Some(code)) => {
            format!("`{code}` is evaluated and its value is not used.")
        }
        ("duplicate-condition", Some(code)) => format!(
            "`{code}` is tested twice in the same if/else chain, so the second branch cannot run."
        ),
        ("exit-from-finally", Some(code)) => format!(
            "`{code}` sits in the block that runs on the way out, and overrides whatever was \
             in flight."
        ),
        ("float-equality", Some(code)) => format!(
            "`{code}` is compared for exact equality, which floating-point rounding can make false."
        ),
        ("hand-rolled-throws", Some(code)) => format!(
            "`{code}` catches the failure with its own `try`, instead of the framework's \
             expected-failure assertion."
        ),
        ("integer-division", Some(code)) => {
            format!("`{code}` divides two integers, so the fraction is dropped.")
        }
        ("nested-ternary", Some(code)) => {
            format!("This conditional expression is nested in another one: `{code}`.")
        }
        ("nested-then", Some(code)) => format!(
            "A `{code}` callback contains another `{code}`, so the steps nest instead of \
             reading in order."
        ),
        ("negated-comparison", Some(code)) => format!(
            "A negation wraps a `{code}` comparison, which the opposite operator states directly."
        ),
        ("production-assert", Some(code)) => {
            format!("`{code}` is in production code, where assertions can be disabled.")
        }
        ("redundant-jump", Some(code)) => {
            format!("`{code}` is the last statement of the loop body, so it changes nothing.")
        }
        ("rethrow-only-catch", Some(code)) => {
            format!("The catch block only rethrows (`{code}`), so it adds nothing.")
        }
        ("self-asserting-test", Some(code)) => {
            format!("`{code}` checks a value against itself, so it cannot fail.")
        }
        ("sequential-await", Some(code)) => format!(
            "`{code}` does not use the result of the wait before it, so the two waits can run \
             together."
        ),
        ("shadowed-handler", Some(code)) => format!(
            "`{code}` catches everything the handler below it would, so that handler never runs."
        ),
        ("silent-fallthrough", Some(code)) => {
            format!("`{code}` has a body and no `break`, so it continues into the next case.")
        }
        ("collapsible-if", _) => "This `if` is the only statement inside another `if`, so the two \
                                  conditions can be joined."
            .to_owned(),
        _ => return None,
    })
}

fn in_loop(slug: &str, subject: &str, depth: u32) -> Option<String> {
    let nested = if depth > 1 {
        format!(" (a loop nested {depth} deep)")
    } else {
        String::new()
    };
    Some(match slug {
        "scan-in-loop" => {
            format!("`{subject}` scans a whole collection on every pass of the loop{nested}.")
        }
        "query-in-loop" => format!(
            "`{subject}` makes a separate call on every pass of the loop{nested}, one round \
             trip per element."
        ),
        "await-in-loop" => format!(
            "`{subject}` waits for one call per pass{nested}, so the passes run one after another."
        ),
        "invariant-in-loop" => format!(
            "`{subject}` is built again on every pass of the loop{nested}, though nothing in \
             the loop changes it."
        ),
        "accumulate-by-copy" => format!(
            "`{subject}` is copied into a new value on every pass of the loop{nested}, which \
             makes the loop quadratic."
        ),
        "linear-op-in-loop" => format!(
            "`{subject}` takes time proportional to the collection's size and runs on every \
             pass of the loop{nested}."
        ),
        "sort-in-loop" => format!("`{subject}` sorts again on every pass of the loop{nested}."),
        "lock-in-loop" => {
            format!("`{subject}` takes the lock again on every pass of the loop{nested}.")
        }
        "repeated-computation-in-loop" => format!(
            "`{subject}` is computed again on every pass of the loop{nested}, with nothing that \
             changes between passes."
        ),
        _ => return None,
    })
}

fn recursive(slug: &str, calls: u32) -> Option<String> {
    let times = match calls {
        1 => "once".to_owned(),
        calls => format!("{calls} times"),
    };
    match slug {
        "no-base-case" => Some(format!(
            "The function calls itself {times} and has no branch that can stop the recursion."
        )),
        "branching-recursion" => Some(format!(
            "The function calls itself {times} per call and keeps no cache of results, so the \
             work can grow exponentially."
        )),
        _ => None,
    }
}

fn imprecise(construct: &str) -> Option<String> {
    Some(match construct {
        "!" => "`!` asserts that the value is present, and the checker stops asking whether it \
                is."
        .to_owned(),
        "as" => "`as` overrides the type the checker worked out for this value.".to_owned(),
        "cast" => "A cast overrides the type the checker worked out for this value.".to_owned(),
        "any" | "Any" | "interface{}" | "object" | "Object" | "void*" => format!(
            "`{construct}` accepts every value, so the checker cannot tell what this one is."
        ),
        "auto" | "var" => format!(
            "`{construct}` leaves the type unwritten, so a reader has to infer it from the \
             right-hand side."
        ),
        _ => return None,
    })
}

fn magic_number(literal: &str, occurrences: u32) -> String {
    match occurrences {
        0 | 1 => format!("`{literal}` appears here as a bare literal, outside a named constant."),
        occurrences => format!(
            "`{literal}` appears {occurrences} times in this file outside a named constant."
        ),
    }
}

fn class_shape(slug: &str, fields: u32, methods: u32, trivial: u32) -> Option<String> {
    let has = format!(
        "{} and {}",
        plural(fields, "field", "fields"),
        plural(methods, "method", "methods")
    );
    match slug {
        "lazy-class" => Some(format!(
            "The class has {has}, and nothing in it does more than hold or pass on a value."
        )),
        "data-class" if methods == 0 => Some(format!(
            "The class has {has}, so it holds data and nothing else."
        )),
        "data-class" => Some(format!(
            "The class has {has}, and {trivial} of them {} nothing but get or set a field.",
            if trivial == 1 { "does" } else { "do" }
        )),
        _ => None,
    }
}

fn noise(slug: &str, marker: &str) -> Option<String> {
    Some(match slug {
        "apology-comment" => {
            format!("The comment apologises (`{marker}`) instead of describing the code.")
        }
        "chat-residue" => format!("`{marker}` is chat wording left in the code."),
        "example-value" => format!("`{marker}` is a placeholder value left in the code."),
        "banner-comment" => {
            "This comment is a line of decoration, not a statement about the code.".to_owned()
        }
        _ => return None,
    })
}

fn directive_effect(slug: &str, directive: &str) -> String {
    match slug {
        "focused-test" => format!("`{directive}` makes the runner skip every other test."),
        _ => format!("`{directive}` stops this test from running."),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::finding::{Confidence, SourceRange};
    use crate::rule::RuleId;

    fn finding(rule: RuleId, evidence: Evidence) -> Finding {
        Finding {
            rule,
            location: SourceRange {
                file: Arc::from("a.go"),
                start: 0,
                end: 1,
                line: 1,
                column: 1,
                end_line: 1,
            },
            focus: None,
            severity: rule.spec().severity,
            confidence: Confidence::Verified,
            evidence,
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
        }
    }

    fn name(identifier: &str) -> Evidence {
        Evidence::Name {
            identifier: identifier.to_owned(),
            reason: "legacy text".to_owned(),
        }
    }

    #[test]
    fn a_discarded_result_names_the_call_that_returned_it() {
        let said = finding(RuleId::DiscardedError, name("w.Write")).message();
        assert_eq!(
            said,
            "`w.Write` returns a result that is assigned to an ignored name and never read."
        );
    }

    /// A fingerprint written by an earlier release hashes the evidence's own text, so
    /// rewording the sentence a reader sees must not move it.
    #[test]
    fn rewording_the_sentence_leaves_the_finding_identity_alone() {
        let found = finding(RuleId::DiscardedError, name("w.Write"));
        let legacy = "`w.Write` legacy text.";
        let pinned = crate::identity::fnv1a(&["discarded-error", "a.go", legacy]);
        assert_eq!(
            crate::identity::Identity::exact(&found),
            crate::identity::Identity::from_raw(pinned)
        );
        assert_ne!(found.message(), legacy);
    }

    #[test]
    fn a_magic_number_says_how_often_it_appears() {
        let evidence = Evidence::MagicNumber {
            literal: "3".to_owned(),
            occurrences: 4,
        };
        assert_eq!(
            finding(RuleId::MagicNumber, evidence).message(),
            "`3` appears 4 times in this file outside a named constant."
        );
    }

    #[test]
    fn one_count_in_the_evidence_reads_as_singular() {
        let evidence = Evidence::Breadth {
            counted: "legacy".to_owned(),
            found: 1,
            limit: 0,
        };
        assert_eq!(
            finding(RuleId::BranchingTest, evidence).message(),
            "The test contains 1 branch or loop, so which path ran depends on the data."
        );
    }

    #[test]
    fn two_recursion_rules_do_not_share_one_sentence() {
        let evidence = || Evidence::Recursion {
            calls: 1,
            sampled: None,
        };
        let stuck = finding(RuleId::NoBaseCase, evidence()).message();
        let branching = finding(RuleId::BranchingRecursion, evidence()).message();
        assert!(stuck.contains("no branch that can stop"));
        assert!(branching.contains("keeps no cache"));
    }

    #[test]
    fn a_rule_without_a_sentence_keeps_the_evidence_text() {
        let evidence = Evidence::StatementCount {
            count: 47,
            threshold: 40,
        };
        let long = finding(RuleId::LongFunction, evidence);
        assert_eq!(long.message(), long.evidence.to_string());
    }

    #[test]
    fn every_sentence_is_one_line_that_ends_with_a_full_stop() {
        let sentences = [
            finding(RuleId::EmptyTest, name("T")).message(),
            finding(RuleId::GenericThrow, name("Error")).message(),
            finding(
                RuleId::ImpreciseType,
                Evidence::ImpreciseType {
                    construct: "any".into(),
                },
            )
            .message(),
        ];
        for sentence in sentences {
            assert!(
                sentence.ends_with('.') && !sentence.contains('\n'),
                "{sentence}"
            );
        }
    }
}
