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

fn only(rule: RuleId, every: &Resolved) -> Resolved {
    let mut settings = every.clone();
    let config: &mut RqxConfig = &mut settings.config;
    config.rules.enabled = [rule.slug().to_owned()].into();
    config.rules.disabled = RuleId::ALL
        .iter()
        .filter(|other| **other != rule)
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

#[test]
fn each_rule_alone_reports_what_it_reports_with_every_rule_on() {
    let workers = std::thread::available_parallelism().map_or(4, usize::from);
    let mut differences = Vec::new();
    for name in ROOTS {
        let root = fixtures().join(name);
        let every = every_rule_on(&root);
        let all_on = by_rule(&scan_with(&root, &every, None, CachePolicy::Off).unwrap());
        let next = AtomicUsize::new(0);
        let found: Vec<(RuleId, Vec<String>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    scope.spawn(|| {
                        let mut alone = Vec::new();
                        while let Some(rule) = RuleId::ALL.get(next.fetch_add(1, Relaxed)) {
                            let settings = only(*rule, &every);
                            let outcome =
                                scan_with(&root, &settings, None, CachePolicy::Off).unwrap();
                            alone.push((*rule, by_rule(&outcome).remove(rule).unwrap_or_default()));
                        }
                        alone
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect()
        });
        for (rule, alone) in found {
            if READS_OTHER_RULES.contains(&rule) {
                continue;
            }
            let with_every = all_on.get(&rule).cloned().unwrap_or_default();
            if alone != with_every {
                differences.push(format!(
                    "{name}: {} reports {} alone and {} with every rule on",
                    rule.slug(),
                    alone.len(),
                    with_every.len()
                ));
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
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
