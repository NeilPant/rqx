//! The one place a run is judged. A scan, a review of a branch and a comment all end in
//! the same question, "did this pass", and they used to answer it with three
//! near-copies of the same arithmetic. Here the arithmetic is written once and the
//! difference between them is a table: which checks a scope runs.

use std::collections::{BTreeMap, BTreeSet};

use rqx_core::config::{Gate, GateSeverity, RqxConfig};
use rqx_core::finding::Severity;

use crate::scan::ScanOutcome;

pub struct Verdict {
    pub failures: Vec<String>,
    pub counted: u32,
    /// Findings a baseline already recorded. Reported rather than counted, so a
    /// reader can see the debt is still there while the gate judges only what is new.
    pub known: u32,
}

impl Verdict {
    pub const fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// What a verdict is about: the whole project, or the files a review names.
pub enum Scope<'a> {
    Project,
    Files(&'a BTreeSet<String>),
}

impl Scope<'_> {
    fn covers(&self, file: &str) -> bool {
        match self {
            Self::Project => true,
            Self::Files(files) => files.contains(file),
        }
    }

    fn files<'s>(&'s self, outcome: &'s ScanOutcome) -> &'s BTreeSet<String> {
        match self {
            Self::Project => &outcome.analyzed,
            Self::Files(files) => files,
        }
    }
}

/// Which of the gate's checks a scope runs, in the order they are reported.
#[derive(Debug, PartialEq, Eq)]
struct Checks {
    severity: bool,
    health: bool,
    grade: bool,
    incomplete: bool,
    ratchet: bool,
}

/// Today's behaviour, written down. A project gate keeps its grade, half applied and
/// ratchet checks when `fail_on` is `never`; a review turns severity, health and
/// grade off with it.
fn checks(scope: &Scope<'_>, gate: &Gate) -> Checks {
    let counts_severity = threshold(gate).is_some();
    match scope {
        Scope::Project => Checks {
            severity: counts_severity,
            health: counts_severity,
            grade: true,
            incomplete: true,
            ratchet: true,
        },
        Scope::Files(_) => Checks {
            severity: counts_severity,
            health: counts_severity,
            grade: counts_severity,
            incomplete: true,
            ratchet: true,
        },
    }
}

pub fn judge(outcome: &ScanOutcome, scope: Scope<'_>, config: &RqxConfig) -> Verdict {
    let gate = &config.gate;
    let checks = checks(&scope, gate);

    let (counted, known) = match threshold(gate) {
        Some(severity) if checks.severity => tally(outcome, &scope, severity),
        _ => (0, 0),
    };

    let mut failures = Vec::new();
    if checks.severity {
        failures.extend(severity_failure(counted, gate));
    }
    if checks.health {
        failures.extend(health_failure(outcome, gate));
    }
    if checks.grade {
        failures.extend(grade_failure(outcome, scope.files(outcome), gate));
    }
    if checks.incomplete {
        failures.extend(half_applied(outcome, &scope, gate));
    }
    if checks.ratchet {
        failures.extend(ratchet(outcome, gate));
    }
    Verdict {
        failures,
        counted,
        known,
    }
}

/// What counts against the gate, and what a baseline already holds. A project is
/// counted from `counts`, which is exact; a review from the findings kept for
/// display, which the cap has already thinned.
fn tally(outcome: &ScanOutcome, scope: &Scope<'_>, threshold: Severity) -> (u32, u32) {
    match scope {
        Scope::Project => {
            let at_or_above = |counts: &BTreeMap<Severity, u32>| -> u32 {
                counts
                    .iter()
                    .filter(|(severity, _)| **severity >= threshold)
                    .map(|(_, count)| *count)
                    .sum()
            };
            let total = at_or_above(&outcome.counts.by_severity);
            let counted = at_or_above(&outcome.counts.new_by_severity);
            (counted, total - counted)
        }
        Scope::Files(_) => {
            let reviewed: Vec<_> = outcome
                .findings
                .iter()
                .filter(|finding| scope.covers(&finding.location.file))
                .filter(|finding| finding.severity >= threshold)
                .collect();
            // With a baseline, the gate is about what this change added; without one,
            // every finding is new by definition, because nothing recorded otherwise.
            let known = reviewed.iter().filter(|finding| finding.known).count() as u32;
            (reviewed.len() as u32 - known, known)
        }
    }
}

const fn threshold(gate: &Gate) -> Option<Severity> {
    match gate.fail_on {
        GateSeverity::Never => None,
        GateSeverity::Error => Some(Severity::Error),
        GateSeverity::Warning => Some(Severity::Warning),
        GateSeverity::Info => Some(Severity::Info),
    }
}

fn severity_failure(counted: u32, gate: &Gate) -> Option<String> {
    (counted > gate.max_findings).then(|| {
        format!(
            "{counted} findings at or above `{}`, limit is {}",
            match gate.fail_on {
                GateSeverity::Error => "error",
                GateSeverity::Warning => "warning",
                GateSeverity::Info => "info",
                GateSeverity::Never => "never",
            },
            gate.max_findings
        )
    })
}

fn health_failure(outcome: &ScanOutcome, gate: &Gate) -> Option<String> {
    if gate.min_health == 0 {
        return None;
    }
    let health = outcome
        .counts
        .weighted_health_score(outcome.code_lines, &outcome.impact);
    (health < gate.min_health).then(|| {
        format!(
            "health score {health} is below the required {}",
            gate.min_health
        )
    })
}

/// A baseline says which debt is tolerated. The ratchet says it may not grow, which
/// is the difference between recording debt and accepting it. The baseline records
/// one number for the project, so a review compares the project's total too: debt
/// that grew anywhere is debt the ratchet exists to stop.
fn ratchet(outcome: &ScanOutcome, gate: &Gate) -> Option<String> {
    let recorded = gate.ratchet.then_some(outcome.baseline_size).flatten()?;
    let total = outcome.counts.total;
    (total > recorded).then(|| {
        format!(
            "{total} findings against the {recorded} the baseline recorded; \
             remove {} or run `rqx baseline --update`",
            total - recorded
        )
    })
}

/// The one route from advice to an exit code, and only for the stance that names
/// something broken. A recognised pattern is inventory and one that would merely fit
/// is a suggestion; failing a build on either would make the gate meaningless. A
/// review answers for the patterns in the files it names, not for the whole tree.
fn half_applied(outcome: &ScanOutcome, scope: &Scope<'_>, gate: &Gate) -> Option<String> {
    if !gate.fail_on_incomplete {
        return None;
    }
    let broken: BTreeSet<&str> = outcome
        .advice
        .iter()
        .filter(|advice| advice.advisory.stance() == rqx_core::Stance::Incomplete)
        .filter(|advice| scope.covers(&advice.location.file))
        .map(|advice| advice.advisory.pattern().name())
        .collect();
    if broken.is_empty() {
        return None;
    }
    Some(format!(
        "{} half applied: {}",
        crate::report::plural(broken.len() as u32, "pattern", "patterns"),
        broken.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

/// A single file rotting on its own is invisible to a project-wide score, which is
/// why the grade gate looks per file rather than at the average.
fn grade_failure(outcome: &ScanOutcome, files: &BTreeSet<String>, gate: &Gate) -> Option<String> {
    let floor = gate.min_grade.floor()?;
    let mut below: Vec<(&str, rqx_core::Cleanliness)> = files
        .iter()
        .filter_map(|file| {
            let lines = outcome
                .lines_by_file
                .get(file.as_str())
                .copied()
                .unwrap_or(0);
            let findings = outcome
                .findings_by_file
                .get(file.as_str())
                .copied()
                .unwrap_or(0);
            let grade = rqx_core::Cleanliness::of(outcome.counts.file_score(file, lines), findings);
            (grade < floor).then_some((file.as_str(), grade))
        })
        .collect();
    below.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(right.0)));

    let (file, grade) = below.first()?;
    let extra = below.len() - 1;
    let tail = if extra > 0 {
        format!(" and {extra} more")
    } else {
        String::new()
    };
    Some(format!(
        "`{file}` grades {} against a floor of {}{tail}",
        grade.label(),
        floor.label()
    ))
}

/// What each gate decides today, pinned exactly. These are characterisation tests:
/// they record the verdicts the code gives now, including the ones that are known
/// gaps, so a refactor of the gate cannot change an exit code without a test saying
/// so. A pinned gap is fixed on purpose and its expectation changed in the same
/// commit.
#[cfg(test)]
mod gates {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use rqx_core::config::{Gate, GateGrade, GateSeverity};
    use rqx_core::finding::{Severity, SourceRange};
    use rqx_core::{Advice, AdviceId, RqxConfig, Signal};

    use super::*;
    use crate::check::Review;
    use crate::report::tests::outcome_with;

    /// Three files with deliberately different grades. `bad.ts` carries ten errors in
    /// ten lines and grades critical; `ok.ts` carries a warning and an info and grades
    /// clean; `clean0.ts` has nothing. Across 200 code lines the project scores 54.
    fn scenario() -> ScanOutcome {
        let mut findings = vec![("bad.ts", Severity::Error); 10];
        findings.push(("ok.ts", Severity::Warning));
        findings.push(("ok.ts", Severity::Info));
        let mut outcome = outcome_with(&findings, 3);
        outcome.code_lines = 200;
        outcome.lines_by_file.insert(Arc::from("bad.ts"), 10);
        outcome.lines_by_file.insert(Arc::from("ok.ts"), 100);
        outcome
    }

    fn reviewing(outcome: ScanOutcome, files: &[&str]) -> Review {
        Review {
            outcome,
            reviewed: files.iter().map(|file| (*file).to_owned()).collect(),
            base: "main".to_owned(),
        }
    }

    fn everything() -> Review {
        reviewing(scenario(), &["bad.ts", "ok.ts", "clean0.ts"])
    }

    fn judge_review(review: &Review, config: &RqxConfig) -> Verdict {
        review.judge(config)
    }

    fn judge_scan(outcome: &ScanOutcome, config: &RqxConfig) -> Verdict {
        judge(outcome, Scope::Project, config)
    }

    fn config(gate: Gate) -> RqxConfig {
        RqxConfig {
            gate,
            ..RqxConfig::default()
        }
    }

    fn gate(fail_on: GateSeverity, min_health: u32, min_grade: GateGrade) -> Gate {
        Gate {
            fail_on,
            min_health,
            min_grade,
            ..Gate::default()
        }
    }

    fn half_applied_advice() -> Advice {
        advice(AdviceId::DecoratorNotSubstitutable)
    }

    fn advice(advisory: AdviceId) -> Advice {
        Advice {
            advisory,
            location: SourceRange {
                file: Arc::from("ok.ts"),
                start: 0,
                end: 1,
                line: 1,
                column: 1,
                end_line: 1,
            },
            signal: Signal::Shape {
                class: "Wrapper".to_owned(),
                detail: "forwards everything".to_owned(),
            },
            note: String::new(),
            related: Vec::new(),
        }
    }

    struct Case {
        name: &'static str,
        gate: Gate,
        tweak: fn(&mut ScanOutcome),
        failures: &'static [&'static str],
        counted: u32,
        known: u32,
    }

    fn untouched(_: &mut ScanOutcome) {}

    fn recorded_as_baseline(outcome: &mut ScanOutcome) {
        for finding in &mut outcome.findings {
            finding.known = finding.severity == Severity::Error;
        }
        outcome.counts.new_by_severity.remove(&Severity::Error);
    }

    fn with_half_applied_pattern(outcome: &mut ScanOutcome) {
        outcome.advice.push(half_applied_advice());
    }

    fn with_only_fitting_and_recognised_patterns(outcome: &mut ScanOutcome) {
        outcome.advice.push(advice(AdviceId::CommandFits));
        outcome.advice.push(advice(AdviceId::DecoratorFound));
    }

    fn with_baseline_of_five(outcome: &mut ScanOutcome) {
        outcome.baseline_size = Some(5);
    }

    fn with_baseline_of_twelve(outcome: &mut ScanOutcome) {
        outcome.baseline_size = Some(12);
    }

    fn assert_case(case: &Case, verdict: &Verdict) {
        assert_eq!(verdict.failures, case.failures, "{}: failures", case.name);
        assert_eq!(verdict.counted, case.counted, "{}: counted", case.name);
        assert_eq!(verdict.known, case.known, "{}: known", case.name);
    }

    use GateGrade::{Never as NoGrade, Poor, Pristine};
    use GateSeverity::{Error, Info, Never, Warning};

    const ERRORS: &str = "10 findings at or above `error`, limit is 0";
    const WARNINGS: &str = "11 findings at or above `warning`, limit is 0";
    const INFOS: &str = "12 findings at or above `info`, limit is 0";
    const HEALTH: &str = "health score 54 is below the required 60";
    const BAD_BELOW_POOR: &str = "`bad.ts` grades critical against a floor of poor";
    const TWO_BELOW_PRISTINE: &str =
        "`bad.ts` grades critical against a floor of pristine and 1 more";

    #[test]
    fn the_checks_each_scope_runs_are_written_down() {
        let files = BTreeSet::new();
        let table = |scope: Scope<'_>, fail_on| checks(&scope, &gate(fail_on, 0, NoGrade));

        let all = || Checks {
            severity: true,
            health: true,
            grade: true,
            incomplete: true,
            ratchet: true,
        };
        assert_eq!(table(Scope::Project, Error), all());
        assert_eq!(
            table(Scope::Project, Never),
            Checks {
                severity: false,
                health: false,
                ..all()
            }
        );
        assert_eq!(table(Scope::Files(&files), Error), all());
        assert_eq!(
            table(Scope::Files(&files), Never),
            Checks {
                severity: false,
                health: false,
                grade: false,
                ..all()
            }
        );
    }

    #[test]
    fn the_scenario_is_what_the_expectations_assume() {
        let outcome = scenario();
        assert_eq!(outcome.counts.total, 12);
        assert_eq!(
            outcome
                .counts
                .weighted_health_score(outcome.code_lines, &outcome.impact),
            54
        );
    }

    #[test]
    fn judge_decides_a_review_by_severity_health_and_grade() {
        let cases = [
            Case {
                name: "fail_on error counts the errors",
                gate: gate(Error, 0, NoGrade),
                tweak: untouched,
                failures: &[ERRORS],
                counted: 10,
                known: 0,
            },
            Case {
                name: "fail_on warning adds the warnings",
                gate: gate(Warning, 0, NoGrade),
                tweak: untouched,
                failures: &[WARNINGS],
                counted: 11,
                known: 0,
            },
            Case {
                name: "fail_on info counts everything",
                gate: gate(Info, 0, NoGrade),
                tweak: untouched,
                failures: &[INFOS],
                counted: 12,
                known: 0,
            },
            Case {
                name: "fail_on never passes",
                gate: gate(Never, 0, NoGrade),
                tweak: untouched,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a health floor above the score adds a failure after the count",
                gate: gate(Error, 60, NoGrade),
                tweak: untouched,
                failures: &[ERRORS, HEALTH],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a health floor at or below the score adds nothing",
                gate: gate(Error, 54, NoGrade),
                tweak: untouched,
                failures: &[ERRORS],
                counted: 10,
                known: 0,
            },
            Case {
                // pinned: known gap, fixed in 0.3.0 (A9–A11)
                name: "fail_on never skips the health floor",
                gate: gate(Never, 60, NoGrade),
                tweak: untouched,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a grade floor names the worst file",
                gate: gate(Error, 0, Poor),
                tweak: untouched,
                failures: &[ERRORS, BAD_BELOW_POOR],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a grade floor counts the other files below it",
                gate: gate(Error, 0, Pristine),
                tweak: untouched,
                failures: &[ERRORS, TWO_BELOW_PRISTINE],
                counted: 10,
                known: 0,
            },
            Case {
                name: "health and grade failures follow the count, in that order",
                gate: gate(Error, 60, Poor),
                tweak: untouched,
                failures: &[ERRORS, HEALTH, BAD_BELOW_POOR],
                counted: 10,
                known: 0,
            },
            Case {
                // pinned: known gap, fixed in 0.3.0 (A9–A11)
                name: "fail_on never skips the grade floor",
                gate: gate(Never, 0, Pristine),
                tweak: untouched,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "findings the baseline holds are known, not counted",
                gate: gate(Error, 0, NoGrade),
                tweak: recorded_as_baseline,
                failures: &[],
                counted: 0,
                known: 10,
            },
            Case {
                name: "a baseline does not hide the debt from the health floor",
                gate: gate(Error, 60, NoGrade),
                tweak: recorded_as_baseline,
                failures: &[HEALTH],
                counted: 0,
                known: 10,
            },
            Case {
                name: "a baseline does not hide the debt from the grade floor",
                gate: gate(Error, 0, Poor),
                tweak: recorded_as_baseline,
                failures: &[BAD_BELOW_POOR],
                counted: 0,
                known: 10,
            },
            Case {
                name: "max_findings allows up to that many",
                gate: Gate {
                    max_findings: 10,
                    ..gate(Error, 0, NoGrade)
                },
                tweak: untouched,
                failures: &[],
                counted: 10,
                known: 0,
            },
            Case {
                name: "max_findings one short still fails",
                gate: Gate {
                    max_findings: 9,
                    ..gate(Error, 0, NoGrade)
                },
                tweak: untouched,
                failures: &["10 findings at or above `error`, limit is 9"],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a review fails on a half applied pattern in a file it names",
                gate: Gate {
                    fail_on_incomplete: true,
                    max_findings: 10,
                    ..gate(Error, 0, NoGrade)
                },
                tweak: with_half_applied_pattern,
                failures: &["1 pattern half applied: Decorator"],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a review enforces the ratchet",
                gate: Gate {
                    ratchet: true,
                    max_findings: 10,
                    ..gate(Error, 0, NoGrade)
                },
                tweak: with_baseline_of_five,
                failures: &[
                    "12 findings against the 5 the baseline recorded; remove 7 or run `rqx baseline --update`",
                ],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a review holds a half applied pattern when fail_on is never",
                gate: Gate {
                    fail_on_incomplete: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: with_half_applied_pattern,
                failures: &["1 pattern half applied: Decorator"],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a review holds the ratchet when fail_on is never",
                gate: Gate {
                    ratchet: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: with_baseline_of_five,
                failures: &[
                    "12 findings against the 5 the baseline recorded; remove 7 or run `rqx baseline --update`",
                ],
                counted: 0,
                known: 0,
            },
        ];

        for case in &cases {
            let mut review = everything();
            (case.tweak)(&mut review.outcome);
            assert_case(case, &judge_review(&review, &config(case.gate.clone())));
        }
    }

    #[test]
    fn judge_looks_only_at_the_files_the_review_names() {
        let only_ok = || reviewing(scenario(), &["ok.ts"]);

        let verdict = judge_review(&only_ok(), &config(gate(Error, 0, NoGrade)));
        assert!(verdict.passed(), "{:?}", verdict.failures);
        assert_eq!((verdict.counted, verdict.known), (0, 0));

        let verdict = judge_review(&only_ok(), &config(gate(Warning, 0, NoGrade)));
        assert_eq!(
            verdict.failures,
            ["1 findings at or above `warning`, limit is 0"]
        );
        assert_eq!(verdict.counted, 1);

        let verdict = judge_review(&only_ok(), &config(gate(Info, 0, Pristine)));
        assert_eq!(
            verdict.failures,
            [
                "2 findings at or above `info`, limit is 0",
                "`ok.ts` grades clean against a floor of pristine"
            ],
            "bad.ts is not in the review, so its grade is nobody's business here"
        );

        let verdict = judge_review(&only_ok(), &config(gate(Error, 60, NoGrade)));
        assert_eq!(
            verdict.failures,
            [HEALTH],
            "health is the whole project's, whichever files were reviewed"
        );
    }

    #[test]
    fn a_review_answers_only_for_the_patterns_in_its_files() {
        let mut outcome = scenario();
        with_half_applied_pattern(&mut outcome);
        let config = config(Gate {
            fail_on_incomplete: true,
            ..gate(Never, 0, NoGrade)
        });

        let in_the_review = reviewing(outcome, &["ok.ts"]);
        assert_eq!(
            judge_review(&in_the_review, &config).failures,
            ["1 pattern half applied: Decorator"]
        );

        let elsewhere = reviewing(in_the_review.outcome, &["bad.ts"]);
        assert!(judge_review(&elsewhere, &config).passed());
    }

    #[test]
    fn judge_scan_decides_a_whole_project_by_severity_health_grade_patterns_and_ratchet() {
        let cases = [
            Case {
                name: "fail_on error counts the errors",
                gate: gate(Error, 0, NoGrade),
                tweak: untouched,
                failures: &[ERRORS],
                counted: 10,
                known: 0,
            },
            Case {
                name: "fail_on warning adds the warnings",
                gate: gate(Warning, 0, NoGrade),
                tweak: untouched,
                failures: &[WARNINGS],
                counted: 11,
                known: 0,
            },
            Case {
                name: "fail_on info counts everything",
                gate: gate(Info, 0, NoGrade),
                tweak: untouched,
                failures: &[INFOS],
                counted: 12,
                known: 0,
            },
            Case {
                name: "fail_on never passes",
                gate: gate(Never, 0, NoGrade),
                tweak: untouched,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a health floor above the score adds a failure after the count",
                gate: gate(Error, 60, NoGrade),
                tweak: untouched,
                failures: &[ERRORS, HEALTH],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a health floor at or below the score adds nothing",
                gate: gate(Error, 54, NoGrade),
                tweak: untouched,
                failures: &[ERRORS],
                counted: 10,
                known: 0,
            },
            Case {
                // pinned: known gap, fixed in 0.3.0 (A9–A11)
                name: "fail_on never skips the health floor",
                gate: gate(Never, 60, NoGrade),
                tweak: untouched,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a grade floor names the worst file",
                gate: gate(Error, 0, Poor),
                tweak: untouched,
                failures: &[ERRORS, BAD_BELOW_POOR],
                counted: 10,
                known: 0,
            },
            Case {
                name: "a grade floor counts the other files below it",
                gate: gate(Error, 0, Pristine),
                tweak: untouched,
                failures: &[ERRORS, TWO_BELOW_PRISTINE],
                counted: 10,
                known: 0,
            },
            Case {
                name: "fail_on never still enforces the grade floor",
                gate: gate(Never, 0, Poor),
                tweak: untouched,
                failures: &[BAD_BELOW_POOR],
                counted: 0,
                known: 0,
            },
            Case {
                name: "findings the baseline holds are known, not counted",
                gate: gate(Error, 0, NoGrade),
                tweak: recorded_as_baseline,
                failures: &[],
                counted: 0,
                known: 10,
            },
            Case {
                name: "a baseline does not hide the debt from the health floor",
                gate: gate(Error, 60, NoGrade),
                tweak: recorded_as_baseline,
                failures: &[HEALTH],
                counted: 0,
                known: 10,
            },
            Case {
                name: "a half applied pattern fails only when asked to",
                gate: gate(Never, 0, NoGrade),
                tweak: with_half_applied_pattern,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "fail_on_incomplete names the half applied pattern",
                gate: Gate {
                    fail_on_incomplete: true,
                    ..gate(Error, 0, NoGrade)
                },
                tweak: with_half_applied_pattern,
                failures: &[ERRORS, "1 pattern half applied: Decorator"],
                counted: 10,
                known: 0,
            },
            Case {
                name: "fail_on_incomplete holds when fail_on is never",
                gate: Gate {
                    fail_on_incomplete: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: with_half_applied_pattern,
                failures: &["1 pattern half applied: Decorator"],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a fitting or recognised pattern never fails the gate",
                gate: Gate {
                    fail_on_incomplete: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: with_only_fitting_and_recognised_patterns,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "the ratchet fails when the total outgrows the baseline",
                gate: Gate {
                    ratchet: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: with_baseline_of_five,
                failures: &[
                    "12 findings against the 5 the baseline recorded; remove 7 or run `rqx baseline --update`",
                ],
                counted: 0,
                known: 0,
            },
            Case {
                name: "the ratchet passes at exactly the baseline",
                gate: Gate {
                    ratchet: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: with_baseline_of_twelve,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "the ratchet needs a baseline to compare against",
                gate: Gate {
                    ratchet: true,
                    ..gate(Never, 0, NoGrade)
                },
                tweak: untouched,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "a baseline size means nothing without the ratchet",
                gate: gate(Never, 0, NoGrade),
                tweak: with_baseline_of_five,
                failures: &[],
                counted: 0,
                known: 0,
            },
            Case {
                name: "every failure is reported, in a fixed order",
                gate: Gate {
                    fail_on_incomplete: true,
                    ratchet: true,
                    ..gate(Error, 60, Poor)
                },
                tweak: |outcome| {
                    with_half_applied_pattern(outcome);
                    with_baseline_of_five(outcome);
                },
                failures: &[
                    ERRORS,
                    HEALTH,
                    BAD_BELOW_POOR,
                    "1 pattern half applied: Decorator",
                    "12 findings against the 5 the baseline recorded; remove 7 or run `rqx baseline --update`",
                ],
                counted: 10,
                known: 0,
            },
        ];

        for case in &cases {
            let mut outcome = scenario();
            (case.tweak)(&mut outcome);
            assert_case(case, &judge_scan(&outcome, &config(case.gate.clone())));
        }
    }

    #[test]
    fn judge_scan_counts_from_the_exact_tally_not_the_retained_findings() {
        let mut outcome = scenario();
        outcome.findings.clear();
        outcome.analyzed = BTreeSet::from(["bad.ts".to_owned()]);

        let verdict = judge_scan(&outcome, &config(gate(Error, 0, NoGrade)));
        assert_eq!(verdict.failures, [ERRORS]);
        assert_eq!(verdict.counted, 10);
    }
}
