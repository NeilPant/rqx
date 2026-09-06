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

The version lives in five files — the Cargo workspace, the VS Code manifest, the Zed
extension's two, and the JetBrains properties. It bumps all five, so a plugin cannot
ship claiming last version's number. `every_manifest_carries_the_same_version` fails
the build if one drifts anyway.

It then turns `## Unreleased` in `CHANGELOG.md` into `## 0.2.0 — <today>` and leaves
an empty `## Unreleased` above it for the next change. **That section becomes the
release notes**, so it has to be written before the release, in the pull requests
that earned it — the pipeline reads it back out and posts it verbatim.

Before committing it runs `cargo fmt --check`, `clippy -D warnings` and the whole
test suite. It refuses to run at all on a dirty tree, off `main`, on a tag that
already exists, or with an empty Unreleased section.

It does not push. The tag sitting locally is the last moment to change your mind.

## What the tag triggers

`.github/workflows/release.yml`, in four stages:

| Stage | Does |
|---|---|
| `check` | tag matches `Cargo.toml`, the changelog has that section, fmt, clippy, tests, and `cargo publish --dry-run` for all three crates |
| `build` | five targets, each tarball holding the binary, README, LICENSE and changelog, each binary run once to prove it starts |
| `publish` | `SHA256SUMS`, then a GitHub release whose body is the changelog section |
| `crates` / `extensions` | crates.io in dependency order, then the VS Code Marketplace, Open VSX and the `.vsix` on the release |

`check` runs before anything is built, because a tag is hard to take back once a
release and a crates.io version hang off it. crates.io publishing tolerates a version
that is already there, so the job is safe to re-run when one of the three fails
halfway.

A tag with a hyphen in it — `v0.3.0-rc.1` — is marked a prerelease, so an installer
looking for the latest version skips it.

## Secrets

| Secret | For | Missing |
|---|---|---|
| `CARGO_REGISTRY_TOKEN` | crates.io | the job fails |
| `VSCE_PAT` | VS Code Marketplace | skipped with a notice |
| `OVSX_PAT` | Open VSX | skipped with a notice |

The two marketplace tokens are skipped rather than required so that a fork cutting a
tag gets binaries instead of a red pipeline about credentials it was never going to
have. `GITHUB_TOKEN` is the one the runner already provides.

The `crates` and `extensions` jobs each name a GitHub environment — `crates.io` and
`marketplaces`. Adding a required reviewer to either turns that publish into
something a person approves, which is worth doing before the first release goes out.

## If it goes wrong

**Before the push.** `git tag -d v0.2.0 && git reset --hard HEAD~1`.

**After the push, before crates.io.** Delete the tag and the draft release, fix, tag
again. Nothing outside GitHub has seen it.

**After crates.io.** A published version cannot be replaced. `cargo yank --version
0.2.0 rqx` stops new projects resolving to it and leaves existing lock files alone,
then release the fix as `0.2.1`. Yanking is not deleting: the code stays readable.

## Version numbers

Semantic versioning, read from the point of view of somebody whose build depends on
this:

- **Patch** — a fixed rule, a faster scan, a corrected message.
- **Minor** — a new rule, a new command, a new flag, a new reporter. A new rule can
  make a passing gate fail, which is why a project pins a version in CI.
- **Major** — a removed or renamed command, a changed exit code, a changed JSON or
  SARIF shape, a config key that no longer means what it did.

Before 1.0 the minor is where the breaking changes go, and the changelog says so.
