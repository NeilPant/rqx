//! Who owns what in a scan: the CODEOWNERS file read from disk, and the health of each
//! owner's files. The rules themselves live in `rqx_core::ownership`; this is the part
//! that touches the file system and the scan.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rqx_core::Cleanliness;
use rqx_core::ownership::{CodeOwners, Dialect};

use crate::scan::ScanOutcome;

/// Where each forge looks for the file, in the order it prefers them. GitLab reads one
/// more place than GitHub does.
pub const LOCATIONS: [&str; 4] = [
    ".github/CODEOWNERS",
    ".gitlab/CODEOWNERS",
    "CODEOWNERS",
    "docs/CODEOWNERS",
];

/// What a file that no rule names is listed under, since it still belongs in the totals.
pub const UNOWNED: &str = "(unowned)";

/// The CODEOWNERS file under `root`, or `None` when the project has none or it holds no
/// rule, so a project without ownership sees no change anywhere.
pub fn load(root: &Path) -> Option<CodeOwners> {
    LOCATIONS.iter().find_map(|location| {
        let text = std::fs::read_to_string(root.join(location)).ok()?;
        let owners = CodeOwners::parse(&text, Dialect::detect(location, &text));
        (!owners.is_empty()).then_some(owners)
    })
}

/// One owner's files, as the report shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerRow {
    pub owner: String,
    pub files: u32,
    pub findings: u32,
    pub health: u32,
    pub grade: Cleanliness,
}

/// Every owner of `files`, worst health first. A file with several owners counts for
/// each of them, which is what makes a shared file visible to everyone who answers for
/// it.
pub fn table(outcome: &ScanOutcome, files: &BTreeSet<String>) -> Vec<OwnerRow> {
    let Some(codeowners) = &outcome.owners else {
        return Vec::new();
    };
    let mut owned: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for file in files {
        let owners = codeowners.owners_of(file);
        if owners.is_empty() {
            owned.entry(UNOWNED).or_default().push(file);
        }
        for owner in owners {
            owned.entry(owner).or_default().push(file);
        }
    }
    let mut rows: Vec<OwnerRow> = owned
        .into_iter()
        .map(|(owner, files)| row(outcome, owner, &files))
        .collect();
    rows.sort_by(|left, right| {
        left.health
            .cmp(&right.health)
            .then(right.findings.cmp(&left.findings))
            .then_with(|| left.owner.cmp(&right.owner))
    });
    rows
}

/// The scanned files an owner answers for. Owners are matched without regard to case,
/// as the forges do.
pub fn files_of<'a>(outcome: &'a ScanOutcome, owner: &str) -> Vec<&'a str> {
    let Some(codeowners) = &outcome.owners else {
        return Vec::new();
    };
    outcome
        .analyzed
        .iter()
        .map(String::as_str)
        .filter(|file| {
            codeowners
                .owners_of(file)
                .iter()
                .any(|named| named.eq_ignore_ascii_case(owner))
        })
        .collect()
}

/// The same curve as the project's, applied to these files alone, so an owner's number
/// is comparable with the project's and a gate on it means what the project gate means.
pub fn row(outcome: &ScanOutcome, owner: &str, files: &[&str]) -> OwnerRow {
    let lines: u64 = files
        .iter()
        .map(|file| u64::from(outcome.lines_by_file.get(*file).copied().unwrap_or(0)))
        .sum();
    let findings: u32 = files
        .iter()
        .map(|file| outcome.findings_by_file.get(*file).copied().unwrap_or(0))
        .sum();
    let weighted = outcome
        .counts
        .weighted_in(files.iter().copied(), &outcome.impact);
    let health = rqx_core::score::health_of(weighted, lines);
    OwnerRow {
        owner: owner.to_owned(),
        files: files.len() as u32,
        findings,
        health,
        grade: Cleanliness::of(health, findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::tests::outcome_with;
    use rqx_core::finding::Severity;

    fn owned_scan(codeowners: &str, findings: &[(&str, Severity)], files: &[&str]) -> ScanOutcome {
        let mut outcome = outcome_with(findings, files.len() as u32);
        outcome.owners = Some(CodeOwners::parse(codeowners, Dialect::GitHub));
        outcome.analyzed = files.iter().map(|file| (*file).to_owned()).collect();
        for file in files {
            outcome.lines_by_file.insert((*file).into(), 100);
        }
        outcome.code_lines = 100 * files.len() as u64;
        outcome
    }

    #[test]
    fn a_project_without_a_codeowners_file_has_no_owner_rows() {
        let outcome = outcome_with(&[("a.ts", Severity::Error)], 1);
        assert!(table(&outcome, &outcome.analyzed).is_empty());
    }

    #[test]
    fn each_owner_is_scored_on_their_own_files() {
        let findings = [
            ("web/a.ts", Severity::Error),
            ("web/a.ts", Severity::Error),
            ("web/a.ts", Severity::Error),
        ];
        let outcome = owned_scan(
            "web/ @web\napi/ @api\n",
            &findings,
            &["web/a.ts", "api/b.ts"],
        );
        let rows = table(&outcome, &outcome.analyzed);

        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].owner.as_str(), rows[0].findings), ("@web", 3));
        assert_eq!((rows[1].owner.as_str(), rows[1].health), ("@api", 100));
        assert!(rows[0].health < rows[1].health);
        assert_eq!(rows[1].grade, Cleanliness::Pristine);
    }

    #[test]
    fn a_file_with_two_owners_counts_for_both() {
        let outcome = owned_scan("shared/ @a @b\n", &[], &["shared/x.ts"]);
        let rows = table(&outcome, &outcome.analyzed);
        let names: Vec<&str> = rows.iter().map(|row| row.owner.as_str()).collect();
        assert_eq!(names, ["@a", "@b"]);
        assert!(rows.iter().all(|row| row.files == 1));
    }

    #[test]
    fn a_file_no_rule_names_is_listed_as_unowned() {
        let outcome = owned_scan("web/ @web\n", &[], &["web/a.ts", "tools/t.ts"]);
        let rows = table(&outcome, &outcome.analyzed);
        assert!(
            rows.iter()
                .any(|row| row.owner == UNOWNED && row.files == 1)
        );
    }

    #[test]
    fn the_first_location_that_holds_a_rule_wins() {
        let root = std::env::temp_dir().join(format!("rqx-owners-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".github")).expect("a temp project");
        std::fs::write(root.join(".github/CODEOWNERS"), "* @github\n").expect("a file");
        std::fs::write(root.join("CODEOWNERS"), "* @root\n").expect("a file");

        let loaded = load(&root);
        let _ = std::fs::remove_dir_all(&root);
        let owners = loaded.expect("a file with a rule");
        assert_eq!(owners.owners_of("a.ts"), ["@github"]);
    }

    #[test]
    fn a_file_with_only_comments_is_no_ownership_at_all() {
        let root = std::env::temp_dir().join(format!("rqx-owners-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temp project");
        std::fs::write(root.join("CODEOWNERS"), "# nobody yet\n").expect("a file");

        let loaded = load(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(loaded.is_none());
    }
}
