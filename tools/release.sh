#!/usr/bin/env bash
# Cut a release: bump every manifest that carries the version, turn the changelog's
# Unreleased section into the new one, commit, tag.
#
# Nothing is pushed. The tag is what triggers the pipeline, so the last step stays a
# deliberate one you take after reading `git show`.
#
#   tools/release.sh 0.2.0
#   tools/release.sh 0.2.0 --dry-run

set -euo pipefail

version="${1:-}"
dry_run=""
[ "${2:-}" = "--dry-run" ] && dry_run="yes"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

die() { echo "error: $*" >&2; exit 1; }
step() { echo "  $*"; }

[ -n "$version" ] || die "usage: tools/release.sh <version> [--dry-run]"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
  || die "\`$version\` is not a semantic version"

current="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
# The tag is the guard against cutting the same release twice. Refusing the version
# already in the manifest as well would mean the first release could never be cut,
# because nothing has bumped it yet.
git rev-parse "v$version" >/dev/null 2>&1 && die "tag v$version already exists"

branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" = "main" ] || die "on \`$branch\`; releases are cut from main"
[ -z "$(git status --porcelain)" ] || die "working tree is not clean"

# The changelog is the release notes: the pipeline reads this section back out and
# posts it. An empty one means a release nobody can read.
grep -q '^## Unreleased$' CHANGELOG.md || die "CHANGELOG.md has no \`## Unreleased\` section"
unreleased="$(awk '/^## Unreleased$/{f=1;next} /^## /{f=0} f' CHANGELOG.md | grep -v '^$' || true)"
[ -n "$unreleased" ] || die "the Unreleased section is empty; a release needs notes"

echo "rqx $current -> $version"

bump() {
  local file="$1" pattern="$2"
  [ -f "$file" ] || die "$file is missing"
  grep -qE "$pattern" "$file" || die "$file does not hold the version where expected"
  if [ -z "$dry_run" ]; then
    perl -pi -e "s/\Q$current\E/$version/ if /$pattern/" "$file"
  fi
  step "$file"
}

# Twice in this one: the package version, and the version beside the path on each
# internal dependency in `[workspace.dependencies]`. Leaving the second behind makes
# the workspace stop resolving, which is a strange error to meet mid-release.
bump Cargo.toml                        '^version = "'
bump Cargo.toml                        '^rqx-(core|lang) = \{ version = "'

if [ -z "$dry_run" ]; then
  today="$(date -u +%Y-%m-%d)"
  # `## Unreleased` stays, empty, above the new section: the next change to land has
  # somewhere to go without anyone having to remember to add the heading back.
  #
  # awk rather than perl because it passes bytes through untouched. Perl in slurp
  # mode decodes the em dash in the heading it inserts and then re-encodes every
  # other one in the file, which turns each of them into mojibake.
  awk -v heading="## $version — $today" '
    { print }
    /^## Unreleased$/ && !cut { print ""; print heading; cut = 1 }
  ' CHANGELOG.md > CHANGELOG.next && mv CHANGELOG.next CHANGELOG.md
  step "CHANGELOG.md"

  # Rewrites the workspace members' versions in the lock file without touching a
  # dependency, which is what `cargo update -w` does and `cargo build` also would.
  cargo update --workspace --quiet
  step "Cargo.lock"
fi

echo
echo "checks"
if [ -z "$dry_run" ]; then
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
else
  step "skipped by --dry-run"
fi

if [ -n "$dry_run" ]; then
  echo
  echo "dry run; nothing written. Release notes would be:"
  echo
  printf '%s\n' "$unreleased"
  exit 0
fi

git add -A
git commit -q -m "release: $version"
git tag -a "v$version" -m "rqx $version"

echo
echo "committed and tagged v$version. Nothing is pushed yet:"
echo
echo "  git show v$version"
echo "  git push origin main --follow-tags"
