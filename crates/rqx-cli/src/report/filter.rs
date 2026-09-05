use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rqx_core::finding::{Confidence, Finding, Severity};
use rqx_core::rule::{Category, RuleId};

/// Narrows what is shown, never what is scanned. A finding the filter hides is
/// still counted, still scored and still judged by the gate, because a health
/// score that moved with a display flag would not mean anything.
#[derive(Default)]
pub struct FindingFilter {
    severity: Option<Severity>,
    rules: Vec<RuleId>,
    categories: Vec<Category>,
    paths: Option<Gitignore>,
    /// The files a revision says changed. `None` when the run was not narrowed to a
    /// revision at all, which is not the same as a revision that changed nothing.
    touched: Option<std::collections::HashSet<String>>,
    text: String,
    /// What to leave out. Kept apart from what to keep because they answer different
    /// questions: `--rule` is "only this", `--not-rule` is "everything but this", and
    /// a project uses one or the other rather than both.
    without_rules: Vec<RuleId>,
    without_categories: Vec<Category>,
    without_paths: Option<Gitignore>,
    confidence: Option<Confidence>,
    fixable: bool,
}

impl FindingFilter {
    pub fn new(
        severity: Option<Severity>,
        rules: &[String],
        categories: &[String],
        paths: &[String],
    ) -> anyhow::Result<Self> {
        let rules = rules
            .iter()
            .map(|name| {
                RuleId::parse(name).ok_or_else(|| {
                    super::unknown(
                        "rule",
                        name,
                        &RuleId::ALL
                            .iter()
                            .map(|rule| rule.slug())
                            .collect::<Vec<_>>(),
                        "Run `rqx rules` for the whole catalogue.",
                    )
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let categories = categories
            .iter()
            .map(|name| {
                Category::parse(name).ok_or_else(|| {
                    super::unknown(
                        "family",
                        name,
                        &Category::ALL.map(Category::slug),
                        &format!(
                            "The families are: {}.",
                            Category::ALL.map(Category::slug).join(", ")
                        ),
                    )
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let paths = if paths.is_empty() {
            None
        } else {
            let mut builder = GitignoreBuilder::new("");
            for pattern in paths {
                builder.add_line(None, pattern)?;
            }
            Some(builder.build()?)
        };

        Ok(Self {
            severity,
            rules,
            categories,
            paths,
            ..Self::default()
        })
    }

    /// The negative of the three list filters. Unknown names are refused the same way
    /// they are on the positive side; a typo that silently hid nothing would be worse
    /// here, because the reader would see a full list and believe it was narrowed.
    pub fn without(
        mut self,
        rules: &[String],
        categories: &[String],
        paths: &[String],
    ) -> anyhow::Result<Self> {
        let named = Self::new(None, rules, categories, paths)?;
        self.without_rules = named.rules;
        self.without_categories = named.categories;
        self.without_paths = named.paths;
        Ok(self)
    }

    pub const fn at_confidence(mut self, confidence: Option<Confidence>) -> Self {
        self.confidence = confidence;
        self
    }

    /// Only findings whose rule can fix itself — the set `rqx fix` would act on.
    pub const fn fixable(mut self, only: bool) -> Self {
        self.fixable = only;
        self
    }

    /// Narrow to the files a revision changed. Unlike `check --base`, nothing that
    /// imports them is pulled in: this is a view of the diff, not a review of it.
    pub fn touching(mut self, files: impl IntoIterator<Item = String>) -> Self {
        self.touched = Some(files.into_iter().collect());
        self
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_lowercase();
        self
    }

    pub fn matches(&self, finding: &Finding) -> bool {
        if self.severity.is_some_and(|floor| finding.severity < floor) {
            return false;
        }
        if !self.rules.is_empty() && !self.rules.contains(&finding.rule) {
            return false;
        }
        if !self.categories.is_empty() && !self.categories.contains(&finding.category()) {
            return false;
        }
        if self.without_rules.contains(&finding.rule)
            || self.without_categories.contains(&finding.category())
        {
            return false;
        }
        if let Some(paths) = &self.without_paths
            && paths
                .matched(finding.location.file.as_ref(), false)
                .is_ignore()
        {
            return false;
        }
        if self
            .confidence
            .is_some_and(|wanted| finding.confidence != wanted)
        {
            return false;
        }
        if self.fixable && !finding.rule.spec().autofix.is_automatic() {
            return false;
        }
        if let Some(touched) = &self.touched
            && !touched.contains(finding.location.file.as_ref())
        {
            return false;
        }
        if let Some(paths) = &self.paths
            && !paths
                .matched(finding.location.file.as_ref(), false)
                .is_ignore()
        {
            return false;
        }
        // The evidence is searched as well as the name, because "cognitive complexity
        // 25" is what a reader remembers seeing rather than the slug that produced it.
        // Only the interface's own search sets this; the command line filters by
        // `--rule`, `--category` and `--path`, none of which read it.
        if !self.text.is_empty()
            && !finding.slug().contains(&self.text)
            && !finding.location.file.to_lowercase().contains(&self.text)
            && !finding
                .evidence
                .to_string()
                .to_lowercase()
                .contains(&self.text)
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rqx_core::finding::{Confidence, Evidence, SourceRange};

    use super::*;

    fn finding(rule: RuleId, severity: Severity, file: &str) -> Finding {
        Finding {
            rule,
            location: SourceRange {
                file: Arc::from(file),
                start: 0,
                end: 1,
                line: 1,
                column: 1,
                end_line: 1,
            },
            severity,
            confidence: Confidence::Verified,
            evidence: Evidence::StatementCount {
                count: 99,
                threshold: 40,
            },
            fix_hint: String::new(),
            related: Vec::new(),
            known: false,
        }
    }

    #[test]
    fn an_empty_filter_keeps_everything() {
        let filter = FindingFilter::default();
        assert!(filter.matches(&finding(RuleId::LongFunction, Severity::Info, "a.ts")));
    }

    #[test]
    fn severity_is_a_floor_not_an_equality() {
        let filter = FindingFilter::new(Some(Severity::Warning), &[], &[], &[]).unwrap();
        assert!(!filter.matches(&finding(RuleId::LongFunction, Severity::Info, "a.ts")));
        assert!(filter.matches(&finding(RuleId::LongFunction, Severity::Warning, "a.ts")));
        assert!(filter.matches(&finding(RuleId::LongFunction, Severity::Error, "a.ts")));
    }

    #[test]
    fn an_unknown_rule_says_where_the_real_ones_are() {
        let error = FindingFilter::new(None, &["no-such-rule".to_owned()], &[], &[])
            .err()
            .expect("an unknown rule must be rejected");
        let message = error.to_string();
        assert!(message.contains("no-such-rule"), "{message}");
        assert!(message.contains("rqx rules"), "{message}");
    }

    /// Sixty slugs printed as a list is not an answer to a typo.
    #[test]
    fn a_mistyped_rule_is_offered_the_one_it_almost_named() {
        let error = FindingFilter::new(None, &["long-funtcion".to_owned()], &[], &[])
            .err()
            .expect("an unknown rule must be rejected");
        assert!(
            error.to_string().contains("Did you mean `long-function`?"),
            "{error}"
        );
    }

    #[test]
    fn a_path_glob_matches_by_suffix_and_directory() {
        let filter = FindingFilter::new(None, &[], &[], &["src/**".to_owned()]).unwrap();
        assert!(filter.matches(&finding(RuleId::LongFunction, Severity::Info, "src/a/b.ts")));
        assert!(!filter.matches(&finding(RuleId::LongFunction, Severity::Info, "test/b.ts")));
    }

    /// The dashboard has always searched the evidence and the interface never did, so
    /// the same query answered two different questions depending on where it was typed.
    #[test]
    fn text_search_covers_the_evidence_too() {
        let filter = FindingFilter::default().with_text("99 statements");
        assert!(filter.matches(&finding(RuleId::LongFunction, Severity::Info, "a.ts")));
        let filter = FindingFilter::default().with_text("nothing like it");
        assert!(!filter.matches(&finding(RuleId::LongFunction, Severity::Info, "a.ts")));
    }

    /// The command line never sets a text search — it filters by rule, family and
    /// path — so widening what the text matches cannot change what `rqx scan` reports.
    #[test]
    fn the_named_filters_are_untouched_by_the_text_search() {
        let filter = FindingFilter::new(None, &[], &[], &["src/**".to_owned()]).unwrap();
        assert!(!filter.matches(&finding(RuleId::LongFunction, Severity::Info, "test/b.ts")));
    }

    #[test]
    fn text_search_covers_the_rule_and_the_path() {
        let filter = FindingFilter::default().with_text("LONG");
        assert!(filter.matches(&finding(RuleId::LongFunction, Severity::Info, "a.ts")));
        let filter = FindingFilter::default().with_text("orders");
        assert!(filter.matches(&finding(
            RuleId::LongFunction,
            Severity::Info,
            "src/Orders.ts"
        )));
        assert!(!filter.matches(&finding(RuleId::LongFunction, Severity::Info, "src/a.ts")));
    }
}
