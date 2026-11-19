//! A rule reports the same findings whatever else is switched on. The engine builds the
//! facts the enabled rules read, so a rule that read facts it never declared went
//! silent as soon as the rules that did declare them were switched off.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use rqx_core::config::RqxConfig;
use rqx_core::pattern::PatternFamily;

use super::*;

/// Rules whose definition reads other rules' findings, so they report differently
/// alone by design, not because a fact was missing.
const READS_OTHER_RULES: [RuleId; 3] = [
    // A file is a hotspot when it changes often and still has findings.
    RuleId::Hotspot,
    // Both leave a line to the per-file rule that already reports work in that loop.
    RuleId::QuadraticViaCall,
    RuleId::RepeatedComputationInLoop,
];

/// One scan root per language, as the fixture tests use them, and the corpus the
/// complexity tests read.
const ROOTS: [&str; 7] = [
    "typescript",
    "javascript",
    "python",
    "go",
    "java",
    "cpp",
    "complexity/real",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The fixture configuration, with every rule and advisory on. History is left out:
/// the churn rules read git, which these trees share with the repository around them.
fn every_rule_on(root: &Path) -> Resolved {
    let mut settings =
        crate::config::resolve(root, Some(&fixtures().join("all-rules.yaml")), None).unwrap();
    let config = &mut settings.config;
    config.rules.disabled.clear();
    config.rules.categories.clear();
    config.rules.enabled = RuleId::ALL
        .iter()
        .map(|rule| rule.slug().to_owned())
        .collect();
    config.scan.churn_days = 0;
    config.scan.max_findings = 0;
    config.scan.threads = 1;
    settings
}

fn only(rules: &[RuleId], every: &Resolved) -> Resolved {
    let mut settings = every.clone();
    let config: &mut RqxConfig = &mut settings.config;
    config.rules.enabled = rules.iter().map(|rule| rule.slug().to_owned()).collect();
    config.rules.disabled = RuleId::ALL
        .iter()
        .filter(|other| !rules.contains(other))
        .map(|other| other.slug().to_owned())
        .collect();
    config.patterns.enabled.clear();
    config.patterns.disabled = PatternFamily::ALL
        .iter()
        .map(|family| family.slug().to_owned())
        .collect();
    settings
}

fn by_rule(outcome: &ScanOutcome) -> BTreeMap<RuleId, Vec<String>> {
    let mut found: BTreeMap<RuleId, Vec<String>> = BTreeMap::new();
    for finding in &outcome.findings {
        found
            .entry(finding.rule)
            .or_default()
            .push(format!("{finding:?}"));
    }
    found
}

type Reported = BTreeMap<RuleId, Vec<String>>;

/// One scan of `root` for each set of rules in `enabled`, spread over the machine's
/// cores, with only that set switched on. The results come back in the same order.
fn scan_each(root: &Path, every: &Resolved, enabled: &[Vec<RuleId>]) -> Vec<Reported> {
    let workers = std::thread::available_parallelism().map_or(4, usize::from);
    let next = AtomicUsize::new(0);
    let mut scanned: Vec<(usize, Reported)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Relaxed);
                        let Some(rules) = enabled.get(index) else {
                            break done;
                        };
                        let outcome =
                            scan_with(root, &only(rules, every), None, CachePolicy::Off).unwrap();
                        done.push((index, by_rule(&outcome)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect()
    });
    scanned.sort_by_key(|(index, _)| *index);
    scanned.into_iter().map(|(_, found)| found).collect()
}

fn differences(
    name: &str,
    rules: &[RuleId],
    scanned: &Reported,
    with_every_rule_on: &Reported,
    scanned_with: &str,
) -> Vec<String> {
    rules
        .iter()
        .filter(|rule| !READS_OTHER_RULES.contains(rule))
        .filter(|rule| scanned.get(rule) != with_every_rule_on.get(rule))
        .map(|rule| {
            format!(
                "{name}: {} reports {} {scanned_with} and {} with every rule on",
                rule.slug(),
                scanned.get(rule).map_or(0, Vec::len),
                with_every_rule_on.get(rule).map_or(0, Vec::len)
            )
        })
        .collect()
}

/// A scan reaches its rules only through the facts it builds for them, so rules that
/// read the same facts can be switched on together: if none of them reads a fact its
/// declaration leaves out, each reports what it reports with every rule on. Four scans
/// per root stand in for one per rule, and every root is covered.
#[test]
fn the_rules_of_each_fact_plan_report_what_they_report_with_every_rule_on() {
    let mut by_plan: BTreeMap<Vec<rqx_core::Fact>, Vec<RuleId>> = BTreeMap::new();
    for rule in RuleId::ALL {
        by_plan
            .entry(rule.facts().to_vec())
            .or_default()
            .push(*rule);
    }
    let plans: Vec<Vec<RuleId>> = by_plan.into_values().collect();

    let mut found = Vec::new();
    for name in ROOTS {
        let root = fixtures().join(name);
        let every = every_rule_on(&root);
        let all_on = by_rule(&scan_with(&root, &every, None, CachePolicy::Off).unwrap());
        for (rules, scanned) in plans.iter().zip(scan_each(&root, &every, &plans)) {
            found.extend(differences(name, rules, &scanned, &all_on, "with its plan"));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// A rule is not only the facts it reads: one can also depend on another rule being on,
/// which the fact-plan test cannot see. Each rule is therefore also scanned by itself,
/// on the root where it reports most. A rule that reports nowhere is left out; with
/// nothing to lose it can only differ by reporting something, and the fact-plan test
/// would show that.
#[test]
fn each_rule_alone_reports_what_it_reports_with_every_rule_on() {
    let roots: Vec<(&str, PathBuf, Resolved, Reported)> = ROOTS
        .iter()
        .map(|name| {
            let root = fixtures().join(name);
            let every = every_rule_on(&root);
            let all_on = by_rule(&scan_with(&root, &every, None, CachePolicy::Off).unwrap());
            (*name, root, every, all_on)
        })
        .collect();

    let busiest_root = |rule: &RuleId| {
        let reported =
            |root: &(&str, PathBuf, Resolved, Reported)| root.3.get(rule).map_or(0, Vec::len);
        let most = roots.iter().map(reported).max().unwrap_or(0);
        if most == 0 {
            return None;
        }
        roots.iter().position(|root| reported(root) == most)
    };

    let mut found = Vec::new();
    for (index, (name, root, every, all_on)) in roots.iter().enumerate() {
        let alone: Vec<Vec<RuleId>> = RuleId::ALL
            .iter()
            .filter(|rule| busiest_root(rule) == Some(index))
            .map(|rule| vec![*rule])
            .collect();
        for (rules, scanned) in alone.iter().zip(scan_each(root, every, &alone)) {
            found.extend(differences(name, rules, &scanned, all_on, "alone"));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// The import graph reads every file's imports. A subtree whose own rules read only
/// metrics still has to give the graph its imports, or the file it imports from looks
/// unused to a rule switched on at the root.
#[test]
fn a_subtree_whose_rules_read_only_metrics_still_feeds_the_import_graph() {
    let root = std::env::temp_dir().join(format!("rqx-graph-facts-{}", std::process::id()));
    std::fs::create_dir_all(root.join("legacy")).unwrap();
    std::fs::write(
        root.join("a.ts"),
        "export function used(): number {\n  return 1;\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("legacy/b.ts"),
        "import { used } from \"../a\";\n\nconsole.log(used());\n",
    )
    .unwrap();
    let collected: Vec<String> = RuleId::ALL
        .iter()
        .filter(|rule| rule.facts().contains(&rqx_core::Fact::Collected))
        .map(|rule| format!("        - {}\n", rule.slug()))
        .collect();
    std::fs::write(
        root.join("rqx.yaml"),
        format!(
            "entrypoints:\n  paths: [legacy/b.ts]\noverrides:\n  - paths: [\"legacy/**\"]\n    \
             rules:\n      disabled:\n{}    patterns:\n      disabled: [creational, structural, \
             behavioural]\n",
            collected.concat()
        ),
    )
    .unwrap();
    let settings = crate::config::resolve(&root, None, None).unwrap();

    let (_, facts) = settings.for_file_with_facts(&root.join("legacy/b.ts"));
    let outcome = scan_with(&root, &settings, None, CachePolicy::Off).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    let found: Vec<&str> = outcome.findings.iter().map(Finding::slug).collect();
    assert!(found.is_empty(), "{found:?}");
    assert!(facts.builds(rqx_core::Fact::Collected));
}
