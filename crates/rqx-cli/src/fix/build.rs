//! `rqx fix --verify-build`: the Go toolchain's own answer. Each Go module with an
//! edit is copied to a temporary directory, `go vet ./...` runs on the copy before
//! and after the round's edits are applied, and a module that passed before and fails
//! after has all of its edits withheld. The project itself is never touched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use rqx_core::Language;
use rqx_lang::rewrite::Withheld;

use super::{FileFix, Withholding};

/// Lines of `go vet` output carried into the reason. The first errors say what broke.
const REPORTED_LINES: usize = 3;

pub(super) fn guard(
    root: &Path,
    fixes: Vec<FileFix>,
    withheld: &mut Vec<Withholding>,
) -> Vec<FileFix> {
    let (go, mut kept): (Vec<FileFix>, Vec<FileFix>) = fixes
        .into_iter()
        .partition(|fix| Language::of_path(Path::new(&fix.file)) == Some(Language::Go));
    if go.is_empty() {
        return kept;
    }

    let mut modules: BTreeMap<PathBuf, Vec<FileFix>> = BTreeMap::new();
    for fix in go {
        match module_of(&root.join(&fix.file)) {
            Some(module) => modules.entry(module).or_default().push(fix),
            None => refuse(withheld, fix, "no go.mod encloses the file"),
        }
    }
    for (module, fixes) in modules {
        match vet_with(root, &module, &fixes) {
            Ok(()) => kept.extend(fixes),
            Err(reason) => {
                for fix in fixes {
                    refuse(withheld, fix, &reason);
                }
            }
        }
    }
    kept.sort_by(|left, right| left.file.cmp(&right.file));
    kept
}

fn refuse(withheld: &mut Vec<Withholding>, fix: FileFix, reason: &str) {
    withheld.push(Withholding {
        file: fix.file,
        edits: fix.applied.len(),
        reason: Withheld::Build(reason.to_owned()),
    });
}

fn module_of(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|directory| directory.join("go.mod").is_file())
        .map(Path::to_path_buf)
}

fn vet_with(root: &Path, module: &Path, fixes: &[FileFix]) -> Result<(), String> {
    if Command::new("go").arg("version").output().is_err() {
        return Err("`go` is not on PATH".to_owned());
    }
    let copy = std::env::temp_dir().join(format!(
        "rqx-verify-build-{}-{}",
        std::process::id(),
        module
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    let _ = std::fs::remove_dir_all(&copy);
    let result = copy_tree(module, &copy)
        .map_err(|error| format!("copying the module failed: {error}"))
        .and_then(|()| {
            vet(&copy).map_err(|output| {
                format!("go vet fails before the edits, so it cannot judge them: {output}")
            })
        })
        .and_then(|()| {
            for fix in fixes {
                let inside = root
                    .join(&fix.file)
                    .strip_prefix(module)
                    .map(Path::to_path_buf)
                    .map_err(|_| "the file is outside its module".to_owned())?;
                std::fs::write(copy.join(inside), &fix.after)
                    .map_err(|error| format!("writing the copy failed: {error}"))?;
            }
            vet(&copy).map_err(|output| format!("go vet: {output}"))
        });
    let _ = std::fs::remove_dir_all(&copy);
    result
}

fn vet(directory: &Path) -> Result<(), String> {
    let output = Command::new("go")
        .args(["vet", "./..."])
        .current_dir(directory)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(said
        .lines()
        .filter(|line| !line.starts_with('#'))
        .take(REPORTED_LINES)
        .collect::<Vec<_>>()
        .join("; "))
}

/// Everything but version control and rqx's own cache. A symbolic link to a directory
/// is skipped rather than followed, so a link cycle cannot run the copy forever.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || name == ".rqx" {
            continue;
        }
        let kind = entry.file_type()?;
        let target = to.join(&name);
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() || entry.path().is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
