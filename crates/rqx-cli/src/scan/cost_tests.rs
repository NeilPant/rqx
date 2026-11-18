//! The cost stage of a scan: the summaries are composed across files on every scan, from
//! facts the cache serves, so an edit in one file reaches a caller in another.

use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use super::*;

const HOUR: u64 = 3600;

/// A project in a temporary directory, removed when it goes out of scope.
struct Tree {
    root: PathBuf,
    settings: Resolved,
}

impl Tree {
    fn with(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "rqx-cost-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let tree = Self {
            root,
            settings: Resolved::of(rqx_core::config::RqxConfig::default()),
        };
        for (name, text) in files {
            tree.write(name, text, HOUR);
        }
        tree
    }

    fn write(&self, name: &str, text: &str, seconds_ago: u64) {
        let path = self.root.join(name);
        std::fs::write(&path, text).unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds_ago);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    fn with_threads(mut self, threads: usize) -> Self {
        let mut config = self.settings.config.clone();
        config.scan.threads = threads;
        self.settings = Resolved::of(config);
        self
    }

    fn scan(&self) -> ScanOutcome {
        scan_with(&self.root, &self.settings, None, CachePolicy::Forced).unwrap()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The time of the only summarised function of `file`.
fn time_in(outcome: &ScanOutcome, file: &str) -> String {
    let times: Vec<String> = outcome
        .summaries
        .iter()
        .filter(|(name, _, _)| *name == file)
        .map(|(_, _, summary)| summary.time.to_string())
        .collect();
    assert_eq!(times.len(), 1, "{file}: {times:?}");
    times[0].clone()
}

const CALLER: &str = "import { find } from \"./finder\";\n\
    export function lookupAll(items: string[], wanted: string[]) {\n  \
      for (const w of wanted) { find(items, w); }\n}\n";

const FINDER_LINEAR: &str = "export function find(list: string[], x: string) {\n  \
      for (const v of list) { if (v === x) { return v; } }\n  return null;\n}\n";

const FINDER_QUADRATIC: &str = "export function find(list: string[], x: string) {\n  \
      for (const v of list) { for (const w of list) { if (v === x && w === x) { return v; } } }\n  \
      return null;\n}\n";

#[test]
fn a_call_across_files_is_costed_by_the_scan() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);

    let outcome = tree.scan();

    assert_eq!(time_in(&outcome, "caller.ts"), "O(n·m)");
    assert_eq!(time_in(&outcome, "finder.ts"), "O(n)");
    assert!(outcome.summaries.complete);
}

/// A callee edited in one file changes the summary of a caller in another that did not
/// change, and the caller is served from the cache while it does.
#[test]
fn an_edit_to_a_callee_reaches_a_caller_the_cache_serves() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);
    let cold = tree.scan();
    assert_eq!(time_in(&cold, "caller.ts"), "O(n·m)");

    tree.write("finder.ts", FINDER_QUADRATIC, HOUR / 2);
    let warm = tree.scan();

    assert_eq!(
        warm.stats.cache_hits, 1,
        "the caller is served from the cache"
    );
    assert_eq!(warm.stats.cache_misses, 1, "the finder is read again");
    assert_eq!(time_in(&warm, "caller.ts"), "O(n·m²)");
}

#[test]
fn the_summaries_are_the_same_on_one_thread_and_on_sixteen() {
    let files: Vec<(String, String)> = (0..40)
        .map(|index| {
            let next = format!(
                "import {{ step{} }} from \"./f{}\";\n",
                index + 1,
                index + 1
            );
            let body = if index == 39 {
                "  for (const x of items) { sink(x); }\n".to_owned()
            } else {
                format!("  for (const x of items) {{ step{}(items); }}\n", index + 1)
            };
            let source = format!(
                "{}export function step{index}(items: number[]) {{\n{body}}}\n",
                if index == 39 { "" } else { &next }
            );
            (format!("f{index}.ts"), source)
        })
        .collect();
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();

    let alone = Tree::with(&named).with_threads(1).scan();
    let many = Tree::with(&named).with_threads(16).scan();

    assert_eq!(alone.summaries, many.summaries);
    assert_eq!(alone.summaries.len(), 40);
    assert_eq!(time_in(&alone, "f0.ts"), "O(n^40)");
}

#[test]
fn a_scan_of_one_file_says_it_did_not_see_the_project() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);

    let alone = scan_with(
        &tree.root.join("caller.ts"),
        &tree.settings,
        None,
        CachePolicy::Off,
    )
    .unwrap();

    assert!(!alone.summaries.complete);
    let names: Vec<&str> = alone.summaries.iter().map(|(name, _, _)| name).collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert_eq!(time_in(&alone, names[0]), "O(n)");
}

#[test]
fn the_stage_is_timed() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);

    let outcome = tree.scan();

    assert!(
        outcome
            .timings
            .iter()
            .any(|(stage, _)| *stage == "composing costs"),
        "{:?}",
        outcome.timings
    );
}

const SQUARING_CALLER: &str = "import { find } from \"./finder\";\n\
    export function dedupe(items: string[]) {\n  \
      const seen: string[] = [];\n  \
      for (const item of items) { if (find(items, item) === null) { seen.push(item); } }\n  \
      return seen;\n}\n";

fn reported(outcome: &ScanOutcome, rule: RuleId) -> Vec<(String, u32)> {
    outcome
        .findings
        .iter()
        .filter(|finding| finding.rule == rule)
        .map(|finding| (finding.location.file.to_string(), finding.location.line))
        .collect()
}

#[test]
fn a_loop_around_a_call_into_another_file_that_scans_the_same_collection_is_quadratic() {
    let tree = Tree::with(&[("caller.ts", SQUARING_CALLER), ("finder.ts", FINDER_LINEAR)]);

    let outcome = tree.scan();

    assert_eq!(
        reported(&outcome, RuleId::QuadraticViaCall),
        [("caller.ts".to_owned(), 4)]
    );
    let finding = outcome
        .findings
        .iter()
        .find(|finding| finding.rule == RuleId::QuadraticViaCall)
        .unwrap();
    let text = finding.evidence.to_string();
    assert!(
        text.contains("O(n²): loop :4 \u{2192} find O(n) finder.ts:1"),
        "{text}"
    );
}

#[test]
fn a_loop_around_a_call_that_scans_another_collection_is_reported_as_a_product() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);

    let outcome = tree.scan();

    let finding = outcome
        .findings
        .iter()
        .find(|finding| finding.rule == RuleId::QuadraticViaCall)
        .expect("the product is reported");
    let text = finding.evidence.to_string();
    assert!(
        text.contains("over `wanted` and itself takes time in `items`"),
        "{text}"
    );
    assert!(text.contains("O(n·m): loop :3"), "{text}");
}

#[test]
fn a_call_that_costs_nothing_to_the_loop_is_left_alone() {
    let caller = "import { label } from \"./label\";\nexport function f(items: string[]) {\n  for (const item of items) { label(item); }\n}\n";
    let label = "export function label(text: string) { return text.length; }\n";
    let tree = Tree::with(&[("caller.ts", caller), ("label.ts", label)]);

    assert!(reported(&tree.scan(), RuleId::QuadraticViaCall).is_empty());
}

#[test]
fn a_scan_of_one_file_reports_no_rule_that_reads_costs_across_files() {
    let tree = Tree::with(&[("caller.ts", SQUARING_CALLER), ("finder.ts", FINDER_LINEAR)]);

    let alone = scan_with(
        &tree.root.join("caller.ts"),
        &tree.settings,
        None,
        CachePolicy::Off,
    )
    .unwrap();

    assert!(reported(&alone, RuleId::QuadraticViaCall).is_empty());
}

#[test]
fn an_edit_to_the_callee_changes_what_the_cached_caller_is_reported_for() {
    let tree = Tree::with(&[("caller.ts", SQUARING_CALLER), ("finder.ts", FINDER_LINEAR)]);
    assert_eq!(reported(&tree.scan(), RuleId::QuadraticViaCall).len(), 1);

    let constant = "export function find(list: string[], x: string) {\n  return list.length > 0 ? x : null;\n}\n";
    tree.write("finder.ts", constant, HOUR / 2);
    let warm = tree.scan();

    assert_eq!(
        warm.stats.cache_hits, 1,
        "the caller is served from the cache"
    );
    assert!(reported(&warm, RuleId::QuadraticViaCall).is_empty());
}

#[test]
fn a_project_that_turns_the_rule_off_is_not_told_about_it() {
    let mut config = rqx_core::config::RqxConfig::default();
    config
        .rules
        .disabled
        .insert("quadratic-via-call".to_owned());
    let mut tree = Tree::with(&[("caller.ts", SQUARING_CALLER), ("finder.ts", FINDER_LINEAR)]);
    tree.settings = Resolved::of(config);

    assert!(reported(&tree.scan(), RuleId::QuadraticViaCall).is_empty());
}

fn scan_keeping_costs(tree: &Tree) -> ScanOutcome {
    scan_against(
        &tree.root,
        &tree.settings,
        None,
        CachePolicy::Off,
        Comparison::costed(),
        None,
    )
    .unwrap()
}

#[test]
fn the_rules_that_read_costs_report_the_same_whether_or_not_the_facts_are_kept() {
    let tree = Tree::with(&[("caller.ts", SQUARING_CALLER), ("finder.ts", FINDER_LINEAR)]);

    let plain = tree.scan();
    let kept = scan_keeping_costs(&tree);

    assert!(plain.cost_facts.is_none() && kept.cost_facts.is_some());
    assert_eq!(
        reported(&plain, RuleId::QuadraticViaCall),
        reported(&kept, RuleId::QuadraticViaCall)
    );
    assert_eq!(reported(&kept, RuleId::QuadraticViaCall).len(), 1);
}

#[test]
fn the_kept_facts_say_which_functions_each_call_reached() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);

    let kept = scan_keeping_costs(&tree);

    let facts = kept.cost_facts.expect("the facts were asked for");
    let lookup = facts
        .of_file("caller.ts")
        .and_then(|file| {
            file.functions
                .iter()
                .find(|function| function.qual == "lookupAll")
        })
        .expect("the caller's facts");
    assert_eq!(facts.function("caller.ts", lookup.start), Some(lookup));
    let reached = facts.links.of_function("caller.ts", lookup.start);
    assert_eq!(reached.len(), 1);
    assert_eq!(reached[0].len(), 1, "{reached:?}");
    assert_eq!(reached[0][0].file, "finder.ts");
}

#[test]
fn the_cost_facts_are_kept_only_for_a_scan_that_asks_for_them() {
    let tree = Tree::with(&[("caller.ts", CALLER), ("finder.ts", FINDER_LINEAR)]);

    assert!(tree.scan().cost_facts.is_none());

    let kept = scan_keeping_costs(&tree);
    let inputs = kept.cost_facts.expect("the facts were asked for");
    let mut names: Vec<&str> = inputs.files().map(|file| file.name).collect();
    names.sort_unstable();
    assert_eq!(names, ["caller.ts", "finder.ts"]);
}

#[test]
fn a_configured_max_order_keeps_the_cost_facts_without_being_asked() {
    let mut tree = Tree::with(&[("finder.ts", FINDER_LINEAR)]);
    let mut config = tree.settings.config.clone();
    config.performance.max_order = Some("O(n)".to_owned());
    tree.settings = Resolved::of(config);

    assert!(tree.scan().cost_facts.is_some());
}
