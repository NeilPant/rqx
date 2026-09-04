use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rqx_core::finding::{Finding, Severity};
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
    text: String,
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
            text: String::new(),
        })
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
        if !self.categories.is_empty() && !self.categories.contains(&finding.rule.spec().category) {
            return false;
        }
        if let Some(paths) = &self.paths
            && !paths
                .matched(finding.location.file.as_ref(), false)
                .is_ignore()
        {
            return false;
        }
        if !self.text.is_empty()
            && !finding.rule.slug().contains(&self.text)
            && !finding.location.file.to_lowercase().contains(&self.text)
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
