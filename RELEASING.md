# Releasing

One script writes the release, one push publishes it. Nothing goes out that a tag
did not ask for.

```sh
tools/release.sh 0.2.0 --dry-run    # says what it would change, writes nothing
tools/release.sh 0.2.0              # bumps, cuts the changelog, tests, commits, tags
git show v0.2.0                     # read it
git push origin main --follow-tags  # this is the publish
```

## What the script does

The version lives twice in `Cargo.toml`: on the package, and beside the path on each
internal dependency. It moves both, because leaving the second behind makes the
workspace stop resolving. `every_manifest_carries_the_same_version` fails the build
if they drift apart.

It then turns `## Unreleased` in `CHANGELOG.md` into `## 0.2.0 — <today>` and leaves
an empty `## Unreleased` above it for the next change. **That section becomes the
release notes**, so it has to be written before the release, in the pull requests
that earned it — the pipeline reads it back out and posts it verbatim.

Before committing it runs `cargo fmt --check`, `clippy -D warnings` and the whole
test suite. It refuses to run at all on a dirty tree, off `main`, on a tag that
already exists, or with an empty Unreleased section.

It does not push. The tag sitting locally is the last moment to change your mind.

## What the tag triggers

`.github/workflows/release.yml`, in five stages:

| Stage | Does |
|---|---|
| `check` | tag matches `Cargo.toml`, the changelog has that section, fmt, clippy, tests, and `cargo package --workspace` |
| `build` | six binaries, one per platform npm has a name for, each built and run on its own native runner |
| `publish` | a GitHub release whose body is the changelog section, carrying the six tarballs and `SHA256SUMS` |
| `crates` | crates.io, the three crates in dependency order |
| `npm` | the six `@rqx/<platform>` packages, then `@rqx/cli` on top of them |

`publish` waits on `build`, because a release object that exists before its assets do
is a download link that 404s for whoever is quickest. `npm` publishes the platform
packages first and `@rqx/cli` last: the main package resolves its binaries as optional
dependencies at install time, so publishing it before they exist would put a version
on the registry that installs into nothing.

The npm packages carry no version in the repository. The publish job writes
`Cargo.toml`'s into each generated manifest, which is how the tag-matches-the-manifest
check covers npm too, with nothing new that can drift.

A `workflow_dispatch` run is a rehearsal: everything above happens except publishing —
the six binaries are built and smoke-tested, the tarballs and checksums are assembled,
and the npm packages go through `npm publish --dry-run`.

`check` runs before anything is built, because a tag is hard to take back once a
release and a crates.io version hang off it. Both the check and the publish go
through `--workspace`, which resolves the three crates against each other rather than
against the registry — packaging `rqx-lang` alone looks for an `rqx-core` crates.io
has not been given yet, so every release would fail its own check. A version already
on crates.io is treated as a re-run, so the job is safe to retry when it fails
halfway.

A tag with a hyphen in it — `v0.3.0-rc.1` — is marked a prerelease, so an installer
looking for the latest version skips it.

## Secrets

| Secret | For | Missing |
|---|---|---|
| `CARGO_REGISTRY_TOKEN` | crates.io | the publish is skipped with a notice |
| `NPM_TOKEN` | npm | the publish is skipped with a notice |

`GITHUB_TOKEN` is the one the runner already provides, so the binaries and the
release itself need no configuration at all. Both registries are skipped rather than
required, so a fork cutting a tag gets binaries instead of a red pipeline about a
credential it was never going to have.

The `crates` and `npm` jobs each name a GitHub environment. Adding a required reviewer
to one turns that publish into something a person approves.

npm publishes with `--provenance`, which needs no key: `id-token: write` and a public
repository are enough for npm to attest that the tarball came from this workflow at
this commit. The release tarballs get `SHA256SUMS` rather than a signature, for the
same reason — nothing to hold, nothing to rotate, nothing to lose.

## If it goes wrong

**Before the push.** `git tag -d v0.2.0 && git reset --hard HEAD~1`.

**After the push, before the registries.** Delete the tag and the release, fix, tag
again. Nothing outside GitHub has seen it.

**After crates.io or npm.** A published version cannot be replaced. `cargo yank
--version 0.2.0 rqx` and `npm deprecate @rqx/cli@0.2.0 "<why>"` both stop new projects
resolving to it and leave existing lock files alone; then release the fix as `0.2.1`.
Neither is deleting: the code stays readable and installable for anyone pinned to it.

## Version numbers

Semantic versioning, read from the point of view of somebody whose build depends on
this:

- **Patch** — a fixed rule, a faster scan, a corrected message.
- **Minor** — a new rule, a new command, a new flag, a new reporter. A new rule can
  make a passing gate fail, which is why a project pins a version in CI.
- **Major** — a removed or renamed command, a changed exit code, a changed JSON or
  SARIF shape, a config key that no longer means what it did.

Before 1.0 the minor is where the breaking changes go, and the changelog says so.
