//! Whole-scan behaviour: cancellation, the cache, the baseline and the display cap.

use std::cmp::{Ordering, Reverse};

use super::collect::{Collector, Ranked};
use super::*;

#[test]
fn a_cancelled_scan_reports_no_answer_at_all() {
    let (updates, _drain) = mpsc::sync_channel(1);
    let watch = Watch::new(updates);
    watch
        .cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures");
    let settings = Resolved::of(rqx_core::config::RqxConfig::default());
    let Err(failed) = scan_with_progress(&root, &settings, watch) else {
        panic!("a cancelled scan answered anyway");
    };

    assert!(failed.to_string().contains(CANCELLED), "{failed:#}");
}

#[test]
fn a_forced_policy_uses_the_cache_whatever_the_settings_say() {
    let mut config = rqx_core::config::RqxConfig::default();
    config.scan.cache = false;

    assert!(CachePolicy::Forced.enabled(&config));
    assert!(!CachePolicy::Off.enabled(&config));
    assert!(!CachePolicy::FromSettings.enabled(&config));
}

const HOUR: u64 = 3600;

/// A copy of the TypeScript fixtures that the cache tests are free to write into
/// and edit, removed when it goes out of scope.
struct Project {
    root: PathBuf,
    settings: Resolved,
}

impl Project {
    fn copied_from_fixtures() -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("rqx-cache-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typescript");
        // Only the source files: a scan run on the fixtures by hand leaves its own
        // `.rqx` cache beside them, which is no part of the project under test.
        for entry in std::fs::read_dir(fixtures).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                std::fs::copy(&path, root.join(path.file_name().unwrap())).unwrap();
            }
        }
        let project = Self {
            root,
            settings: Resolved::of(rqx_core::config::RqxConfig::default()),
        };
        for entry in std::fs::read_dir(&project.root).unwrap() {
            project.age(&entry.unwrap().path(), HOUR);
        }
        project
    }

    /// Dated well before any cache is written, which is when a stamp is believed.
    fn age(&self, path: &Path, seconds_ago: u64) {
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds_ago);
        Self::stamp(path, at);
    }

    fn stamp(path: &Path, at: std::time::SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    fn scan(&self) -> ScanOutcome {
        scan_with(&self.root, &self.settings, None, CachePolicy::Forced).unwrap()
    }

    fn findings_of(&self, outcome: &ScanOutcome) -> String {
        serde_json::to_string(&outcome.findings).unwrap()
    }

    fn cached_entries(&self) -> usize {
        crate::cache::stored(&self.root, &self.settings).map_or(0, |stored| stored.entries)
    }

    fn cache_file(&self) -> PathBuf {
        self.root
            .join(crate::cache::CACHE_DIRECTORY)
            .join(crate::cache::CACHE_FILE)
    }

    /// A run that finds every file in the cache has nothing to write, so bytes
    /// appended beforehand are still there afterwards.
    fn next_run_hits_every_file(&self) -> bool {
        let mut marked = std::fs::read(self.cache_file()).unwrap();
        marked.extend_from_slice(b"untouched");
        std::fs::write(self.cache_file(), &marked).unwrap();
        self.scan();
        std::fs::read(self.cache_file()).unwrap() == marked
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_warm_run_serves_the_same_findings_from_the_cache() {
    let project = Project::copied_from_fixtures();

    let cold = project.scan();
    let analysed = cold.analyzed.len();
    assert!(analysed > 1);
    assert_eq!(project.cached_entries(), analysed);

    let warm = project.scan();
    assert_eq!(project.findings_of(&cold), project.findings_of(&warm));
    assert!(project.next_run_hits_every_file());
}

#[test]
fn a_cold_run_misses_every_file_and_walks_the_tree_twice() {
    let project = Project::copied_from_fixtures();

    let stats = project.scan().stats;

    let files = project.cached_entries() as u32;
    assert!(files > 1);
    assert_eq!(stats.cache_misses, files);
    assert_eq!(stats.cache_hits, 0);
    assert_eq!(stats.files_read, files);
    assert_eq!(stats.walks, 2);
}

#[test]
fn the_count_reaches_the_progress_without_the_walk_waiting_for_it() {
    let project = Project::copied_from_fixtures();
    let (updates, drained) = mpsc::sync_channel(1000);

    let outcome = scan_with(
        &project.root,
        &project.settings,
        Some(Watch::new(updates)),
        CachePolicy::Forced,
    )
    .unwrap();

    let last = drained
        .try_iter()
        .last()
        .expect("the scan reported progress");
    assert_eq!(last.total, Some(outcome.files_scanned));
    assert_eq!(outcome.stats.walks, 2, "the walk and the count beside it");
}

#[test]
fn a_cache_from_before_the_container_is_replaced_not_read() {
    let project = Project::copied_from_fixtures();
    let legacy = project
        .root
        .join(crate::cache::CACHE_DIRECTORY)
        .join("cache.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, r#"{"version":1,"entries":{}}"#).unwrap();

    let stats = project.scan().stats;

    assert_eq!(stats.cache_hits, 0);
    assert!(!legacy.exists());
    assert!(project.cache_file().exists());
}

#[test]
fn a_warm_run_hits_every_file_and_misses_none() {
    let project = Project::copied_from_fixtures();
    let files = project.scan().analyzed.len() as u32;

    let stats = project.scan().stats;

    assert_eq!(stats.cache_hits, files);
    assert_eq!(stats.cache_misses, 0);
}

#[test]
fn editing_one_file_leaves_every_other_entry_in_the_cache() {
    let project = Project::copied_from_fixtures();
    let analysed = project.scan().analyzed.len();

    let edited = project.root.join("clean.ts");
    let mut source = std::fs::read_to_string(&edited).unwrap();
    source.push_str("\nexport const edited = 1;\n");
    std::fs::write(&edited, source).unwrap();
    project.age(&edited, HOUR / 2);
    project.scan();

    assert_eq!(project.cached_entries(), analysed);
    assert!(project.next_run_hits_every_file());
}

#[test]
fn blame_runs_only_when_stale_todo_can_report() {
    let project = Project::copied_from_fixtures();
    let blamed = project.scan().stats.blames;
    assert!(blamed > 0, "the fixtures hold a note, so a blame is wanted");

    let mut config = rqx_core::config::RqxConfig::default();
    config.rules.disabled.insert("stale-todo".to_owned());
    let settings = Resolved::of(config);
    let skipped = scan_with(&project.root, &settings, None, CachePolicy::Off).unwrap();

    assert_eq!(skipped.stats.blames, 0);
}

#[test]
fn a_warm_run_opens_no_file() {
    let project = Project::copied_from_fixtures();
    let files = project.scan().analyzed.len() as u32;

    let stats = project.scan().stats;

    assert_eq!(stats.files_read, 0);
    assert_eq!(stats.cache_hits, files);
}

#[test]
fn a_file_touched_without_changing_is_read_once_and_then_not_again() {
    let project = Project::copied_from_fixtures();
    project.scan();
    project.age(&project.root.join("clean.ts"), HOUR / 2);

    let touched = project.scan().stats;
    let after = project.scan().stats;

    assert_eq!(touched.files_read, 1);
    assert_eq!(touched.cache_misses, 0);
    assert_eq!(after.files_read, 0);
}

/// The edit keeps the length and the file's own time, and the time is later than
/// the cache was written, which is the one edit a length and a time cannot see.
#[test]
fn an_edit_that_leaves_the_length_and_the_time_alone_is_still_seen() {
    let project = Project::copied_from_fixtures();
    let edited = project.root.join("clean.ts");
    let just_now = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    Project::stamp(&edited, just_now);
    project.scan();
    let before = std::fs::read_to_string(&edited).unwrap();

    std::fs::write(&edited, before.replacen("first", "other", 1)).unwrap();
    Project::stamp(&edited, just_now);
    let warm = project.scan();

    assert_eq!(
        std::fs::metadata(&edited).unwrap().len(),
        before.len() as u64
    );
    assert_eq!(warm.stats.cache_misses, 1);
    assert_eq!(warm.stats.files_read, 1);
}

#[test]
fn a_baseline_still_applies_to_files_that_are_not_read() {
    let project = Project::copied_from_fixtures();
    let cold = project.scan();
    assert!(!cold.findings.is_empty());
    let mut settings = Resolved::of(rqx_core::config::RqxConfig::default());
    settings.baseline = Some(Arc::new(
        crate::baseline::Baseline::of(&cold).recorded(rqx_core::config::BaselineMatch::Count),
    ));

    let warm = scan_with(&project.root, &settings, None, CachePolicy::Forced).unwrap();

    assert_eq!(warm.stats.files_read, 0);
    assert!(
        warm.findings
            .iter()
            .any(|finding| finding.rule.is_project_scoped())
    );
    assert!(warm.findings.iter().all(|finding| finding.known));
}

fn baseline_holding(findings: &[Finding]) -> crate::baseline::Matcher {
    let mut outcome = crate::report::tests::outcome_with(&[], 0);
    outcome.root = PathBuf::from("/nowhere/rqx-baseline-test");
    outcome.findings = findings.to_vec();
    Arc::new(
        crate::baseline::Baseline::of(&outcome).recorded(rqx_core::config::BaselineMatch::Count),
    )
    .matcher()
}

fn of_every_rule() -> Vec<Finding> {
    RuleId::ALL
        .iter()
        .map(|&rule| Finding {
            rule,
            ..ranked("a.ts", 1).0
        })
        .collect()
}

/// Matching does not depend on the rule or on the stage that built a finding, so no
/// list of rules can leave one out: a finding of any rule that the baseline recorded
/// is known whether it arrived with a file or was built after the walk.
#[test]
fn a_recorded_finding_of_every_rule_is_known_from_a_file_and_from_a_stage() {
    let findings = of_every_rule();
    let identities: Vec<Option<rqx_core::Identity>> = findings
        .iter()
        .map(|finding| Some(rqx_core::Identity::resolved(finding, "")))
        .collect();

    let mut from_files = Collector::new(0, false, false, false, None);
    from_files.baseline = Some(baseline_holding(&findings));
    from_files.accept_all(findings.iter().cloned().zip(identities).collect());

    let mut from_stages = Collector::new(0, false, false, false, None);
    from_stages.baseline = Some(baseline_holding(&findings));
    let mut every_rule_on = rqx_core::config::RqxConfig::default();
    every_rule_on.rules.enabled = RuleId::ALL
        .iter()
        .map(|rule| rule.slug().to_owned())
        .collect();
    from_stages.absorb(
        Staged {
            findings: findings.clone(),
            ..Staged::default()
        },
        &Resolved::of(every_rule_on),
    );

    for collector in [from_files, from_stages] {
        let unknown: Vec<&str> = collector
            .retained
            .iter()
            .filter(|Reverse(Ranked(finding))| !finding.known)
            .map(|Reverse(Ranked(finding))| finding.slug())
            .collect();
        assert_eq!(collector.retained.len(), RuleId::ALL.len());
        assert!(unknown.is_empty(), "reported as new: {unknown:?}");
    }
}

/// A baseline never records a waived finding, so a waived repeat must not use up
/// the one the baseline recorded for the finding that is not waived.
#[test]
fn a_waived_finding_does_not_use_up_what_the_baseline_recorded() {
    let recorded = ranked("a.ts", 9).0;
    let waived = ranked("a.ts", 1).0;
    let mut collector = Collector::new(0, false, false, false, None);
    collector.baseline = Some(baseline_holding(std::slice::from_ref(&recorded)));
    collector.suppressions.insert(
        Arc::from("a.ts"),
        vec![rqx_core::suppress::Suppression {
            span: Span::new(0, 1),
            line: 1,
            rules: vec!["imprecise-type".to_owned()],
            reason: Some("generated".to_owned()),
            until: None,
            whole_file: false,
        }],
    );

    collector.absorb(
        Staged {
            findings: vec![waived, recorded],
            ..Staged::default()
        },
        &Resolved::default(),
    );

    assert_eq!(collector.suppressed.len(), 1);
    let Some(Reverse(Ranked(kept))) = collector.retained.peek() else {
        panic!("the recorded finding was not kept");
    };
    assert!(kept.known);
}

#[test]
fn a_deleted_file_is_evicted_from_the_cache() {
    let project = Project::copied_from_fixtures();
    let analysed = project.scan().analyzed.len();

    std::fs::remove_file(project.root.join("clean.ts")).unwrap();
    project.scan();

    assert_eq!(project.cached_entries(), analysed - 1);
}

fn ranked(file: &str, line: u32) -> Ranked {
    Ranked(Finding {
        rule: RuleId::ImpreciseType,
        location: SourceRange {
            file: Arc::from(file),
            start: line * 10,
            end: line * 10 + 4,
            line,
            column: 1,
            end_line: line,
        },
        severity: rqx_core::finding::Severity::Warning,
        confidence: rqx_core::finding::Confidence::Verified,
        evidence: rqx_core::finding::Evidence::ImpreciseType {
            construct: "as".to_owned(),
        },
        fix_hint: String::new(),
        related: Vec::new(),
        known: false,
        focus: None,
    })
}

/// This order decides which findings survive the display cap, so two findings it
/// calls equal are two findings the arrival order chooses between — and arrival
/// order is thread scheduling. A repository with more findings than the cap would
/// show a different set on every run.
#[test]
fn the_display_cap_never_has_to_choose_between_two_equal_findings() {
    let (first, second) = (ranked("a.ts", 1), ranked("b.ts", 1));
    assert_ne!(first.cmp(&second), Ordering::Equal, "same rule, same line");
    assert_ne!(
        ranked("a.ts", 1).cmp(&ranked("a.ts", 2)),
        Ordering::Equal,
        "same rule, same file"
    );
    assert_eq!(ranked("a.ts", 1).cmp(&ranked("a.ts", 1)), Ordering::Equal);
    // The heap keeps the greatest, and the report reads best-first, so the
    // earlier file has to sort as the greater of two equally severe findings.
    assert_eq!(first.cmp(&second), Ordering::Greater);
}
#[test]
fn a_nested_finding_path_uses_forward_slashes_whatever_the_platform() {
    let root = Path::new("repo");
    let nested = root.join("domain").join("billing").join("invoice.ts");

    assert_eq!(
        Positions::new(root).relative(&nested),
        "domain/billing/invoice.ts"
    );
    assert_eq!(relative_to(root, &nested), "domain/billing/invoice.ts");
}

#[test]
fn a_path_outside_the_root_is_still_written_with_forward_slashes() {
    let elsewhere = Path::new("other").join("pkg").join("a.ts");

    assert_eq!(relative_to(Path::new("repo"), &elsewhere), "other/pkg/a.ts");
}

#[test]
fn a_file_is_read_and_indexed_once_however_many_findings_land_in_it() {
    let root = std::env::temp_dir().join(format!("rqx-positions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("wide.ts");
    std::fs::write(&path, "const é = 1;\nconst b = 2;\n").unwrap();
    let mut at = Positions::new(&root);

    let first = at.range(&path, Span::new(19, 24)).unwrap();
    std::fs::remove_file(&path).unwrap();
    let second = at.range(&path, Span::new(19, 24)).unwrap();

    assert_eq!((first.line, first.column), (2, 6));
    assert_eq!((second.line, second.column), (first.line, first.column));
    std::fs::remove_dir_all(&root).unwrap();
}

/// A stage's finding is admitted under the configuration of the file it is
/// reported in, as a per-file finding is, and not under the root's.
#[test]
fn a_stage_finding_obeys_the_override_of_its_own_file() {
    let root = std::env::temp_dir().join(format!("rqx-stage-override-{}", std::process::id()));
    std::fs::create_dir_all(root.join("legacy")).unwrap();
    std::fs::create_dir_all(root.join("strict")).unwrap();
    std::fs::write(
        root.join("rqx.yaml"),
        "overrides:\n  - paths: [\"legacy/**\"]\n    rules:\n      disabled: [imprecise-type]\n  \
         - paths: [\"strict/**\"]\n    rules:\n      imprecise-type:\n        severity: error\n",
    )
    .unwrap();
    let settings = crate::config::resolve(&root, None, None).unwrap();
    let mut collector = Collector::new(0, false, false, false, None);
    collector.root = root.clone();
    let mut staged = Staged::default();
    for file in ["a.ts", "legacy/b.ts", "strict/c.ts"] {
        staged.propose(ranked(file, 1).0);
    }

    collector.absorb(staged, &settings);
    std::fs::remove_dir_all(&root).unwrap();

    let mut kept: Vec<(String, rqx_core::finding::Severity)> = collector
        .retained
        .iter()
        .map(|Reverse(Ranked(finding))| (finding.location.file.to_string(), finding.severity))
        .collect();
    kept.sort();
    assert_eq!(
        kept,
        [
            ("a.ts".to_owned(), rqx_core::finding::Severity::Warning),
            ("strict/c.ts".to_owned(), rqx_core::finding::Severity::Error),
        ]
    );
}

#[test]
fn a_stage_finding_of_a_rule_that_is_off_is_not_kept() {
    let mut config = rqx_core::config::RqxConfig::default();
    config.rules.disabled.insert("imprecise-type".to_owned());
    let mut collector = Collector::new(0, false, false, false, None);
    let mut staged = Staged::default();
    staged.propose(ranked("a.ts", 1).0);

    collector.absorb(staged, &Resolved::of(config));

    assert!(collector.retained.is_empty());
}

#[test]
fn what_the_stages_found_is_counted_whichever_collector_takes_it() {
    let mut collector = Collector::new(10, false, false, false, None);
    let mut staged = Staged::default();
    staged.findings.push(ranked("a.ts", 1).0);
    staged.importers.push((Arc::from("a.ts"), 3));
    staged.blames = 2;

    collector.absorb(staged, &Resolved::default());

    assert_eq!(collector.counts.total, 1);
    assert_eq!(collector.impact.get("a.ts").importers, 3);
    assert_eq!(
        (collector.stats.blames, collector.stats.git_processes),
        (2, 2)
    );
}

/// The base revision of a review is a second checkout of the same files. It reads
/// the working tree's cache so its unchanged files are not analysed again, and it
/// must leave that cache exactly as it found it.
#[test]
fn a_scan_that_borrows_a_cache_is_served_by_it_and_writes_nothing() {
    let lender = Project::copied_from_fixtures();
    lender.scan();
    let cache_file = lender.root.join(".rqx").join("cache.bin");
    let kept = std::fs::read(&cache_file).unwrap();

    let borrower = Project::copied_from_fixtures();
    let outcome = scan_against(
        &borrower.root,
        &borrower.settings,
        None,
        CachePolicy::Forced,
        Comparison::default(),
        Some(&lender.root),
    )
    .unwrap();

    assert!(outcome.stats.cache_hits > 0);
    assert_eq!(outcome.stats.cache_misses, 0, "every file is the lender's");
    assert!(
        !borrower.root.join(".rqx").exists(),
        "the borrower wrote a cache"
    );
    assert_eq!(std::fs::read(&cache_file).unwrap(), kept);
}
