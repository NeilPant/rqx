# Releasing

A release is one local script followed by one push. The push of the tag starts the
publishing pipeline.

```sh
tools/release.sh 0.2.0 --dry-run    # prints the changes it would make, writes nothing
tools/release.sh 0.2.0              # bumps the version, cuts the changelog, tests, commits, tags
git show v0.2.0                     # inspect the release commit and tag
git push origin main --follow-tags  # publishes
```

## Where rqx is published

| Channel | Source |
|---|---|
| GitHub release | the six tarballs and `SHA256SUMS`, built by `release.yml` |
| Install scripts | `install.sh` and `install.ps1` download the release tarball for the platform |
| npm | `@rqx/cli` and six `@rqx/<platform>` packages, once `PUBLISH_NPM` is set |
| Source | `cargo install --git https://github.com/jadhamwi21/rqx --locked rqx` |

rqx is not on crates.io. The name `rqx` there belongs to another project, so
`cargo install rqx` installs different software. The workspace crates keep their names
and are not published.

## What the script does

1. Sets the version in `Cargo.toml`. The version appears in two places: on the
   package, and beside the `path` on each internal dependency. The script updates
   both. If the dependency entries are left behind, the workspace stops resolving.
   The test `every_manifest_carries_the_same_version` fails the build if the two
   differ.
2. Renames `## Unreleased` in `CHANGELOG.md` to `## 0.2.0 — <today>` and adds an
   empty `## Unreleased` above it. The renamed section is used verbatim as the
   release notes, so it must be written before the release, in the pull requests
   that make each change.
3. Runs `cargo fmt --check`, `clippy -D warnings` and the full test suite.
4. Commits the result and creates the tag.

The script refuses to run on a dirty working tree, on a branch other than `main`,
when the tag already exists, or when the Unreleased section is empty.

The script does not push. Until the push, the release exists only as a local commit
and tag, and both can be removed (see "If it goes wrong").

## What the tag triggers

The tag starts `.github/workflows/release.yml`, which runs four stages:

| Stage | Does |
|---|---|
| `check` | tag matches `Cargo.toml`, the changelog has that section, fmt, clippy, tests and the npm launcher tests |
| `build` | six binaries, one per platform that has an npm platform package, each built and run on its own native runner |
| `publish` | a GitHub release whose body is the changelog section, carrying the six tarballs and `SHA256SUMS` |
| `npm` | the six `@rqx/<platform>` packages, then `@rqx/cli` |

Ordering constraints:

- `check` runs before anything is built, because a tag with a release and npm
  versions attached is hard to withdraw.
- `publish` waits for `build`. A release created before its assets exist has download
  links that return 404.
- `npm` publishes the platform packages first and `@rqx/cli` last. `@rqx/cli`
  resolves its binaries as optional dependencies at install time, so a version
  published before them would install without a binary.

Packaging behavior:

- The npm packages carry no version in the repository. The publish job writes the
  version from `Cargo.toml` into each generated manifest, so the check that the tag
  matches `Cargo.toml` also covers npm.

A tag containing a hyphen, such as `v0.3.0-rc.1`, is marked as a prerelease. An
installer that looks up the latest version skips prereleases.

A `workflow_dispatch` run is a rehearsal. It runs every stage except the actual
publishing: the six binaries are built and smoke-tested, the tarballs and checksums
are assembled, and the npm packages go through `npm publish --dry-run`.

## Secrets

| Secret | For | Missing |
|---|---|---|
| `NPM_TOKEN` | npm | the publish is skipped with a notice |

- npm also requires the repository variable `PUBLISH_NPM` set to `true` before a tag
  publishes there. Without it, the `npm` job runs on rehearsals only.
- `GITHUB_TOKEN` is provided by the runner. The binaries and the GitHub release
  need no configuration.
- A missing npm token skips the publish instead of failing it, so a fork that pushes
  a tag gets the binaries and a passing pipeline.
- The `npm` job names a GitHub environment. Adding a required reviewer to the
  environment makes that publish wait for manual approval.
- npm publishes with `--provenance`. This needs no key: `id-token: write` and a
  public repository are enough for npm to attest that the tarball was built by this
  workflow at this commit.
- The release tarballs are accompanied by `SHA256SUMS` and are not signed, so there
  is no signing key to store or rotate.

## If it goes wrong

Before the push:

```sh
git tag -d v0.2.0 && git reset --hard HEAD~1
```

After the push, before npm: delete the tag and the GitHub release, fix the problem,
and tag again. Nothing outside GitHub has received the release.

After npm: a published version cannot be replaced. Run:

```sh
npm deprecate @rqx/cli@0.2.0 "<reason>"
```

The command stops new projects from resolving to the version and leaves existing lock
files unchanged. Then release the fix as `0.2.1`. It does not delete the version, so
the code stays readable and installable for projects pinned to it.

## Version numbers

Versions follow semantic versioning, applied from the viewpoint of a project whose
build depends on rqx:

- Patch: a fixed rule, a faster scan, a corrected message.
- Minor: a new rule, a new command, a new flag, a new reporter. A new rule can make a
  passing gate fail, so projects should pin a version in CI.
- Major: a removed or renamed command, a changed exit code, a changed JSON or SARIF
  shape, a config key whose meaning changed.

Before 1.0, breaking changes are released in minor versions, and the changelog marks
them.

## Repository labels

The issue forms assign `bug`, `false-positive` and `rule`, and GitHub drops a label
that does not exist. `tools/labels.sh` creates every label the forms and the
maintainers use, and is safe to run again. It needs an authenticated `gh` and writes to
the repository `gh` resolves, so run it once on a new repository and again after adding
a rule family or a language:

```sh
tools/labels.sh
```
