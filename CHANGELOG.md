# Changelog

Notable changes, newest first, in the shape
[keep a changelog](https://keepachangelog.com/en/1.1.0/) suggests: Added, Changed,
Removed, Fixed. Dates are when the release was tagged.

Anything user-visible goes under `## Unreleased` in the same pull request that
changes it — `tools/release.sh` turns that section into the release, and the pipeline
posts it as the notes, so an entry written later is an entry nobody reads.

## Unreleased

### Added

- **Configuration is YAML.** `rqx init` writes `rqx.yaml`, and rqx also reads
  `.rqx.yml`, `rqx.yml` and `.rqx.yaml`; two in one directory is an error that names
  both. The keys and structure are the ones `RQX.toml` had, with each table now a
  mapping, `[[override]]` written as an `overrides:` list, and the user config, policy
  files and `--config` all YAML. A mistake in a file is reported as `rqx.yaml:12:3`,
  and a misspelt key lists the keys it could have been.
- **`npm install --save-dev @rqx/cli`.** Every release now carries a prebuilt binary
  for macOS, Linux and Windows on x64 and arm64, behind an npm package that resolves
  the right one at install time. Nothing to compile and no Rust toolchain: a CI job
  that spent minutes building rqx now spends seconds downloading it. The Linux build
  is static musl, so one binary covers Alpine, Debian and RHEL with no glibc floor.
  The tarballs are on the GitHub release with a `SHA256SUMS` beside them, and the npm
  packages publish with Sigstore provenance.

### Removed

- **TOML configuration.** rqx no longer reads `RQX.toml` or `rqx.toml`, in a
  directory, as the user config, as a policy, or through `--config` and `RQX_CONFIG`.
  Finding one stops the run with exit 2 and a message saying so, because ignoring it
  could quietly loosen a CI gate.

### Changed

- **The action downloads rqx rather than building it.** `jadhamwi21/rqx@v1` reads the
  version out of the checkout, fetches that release's binary for the runner and
  verifies it against `SHA256SUMS`. A ref with no published binary — a fork, or
  `@main` ahead of the last release — still compiles from the checkout as before.
- **`rqx update` hands a managed install back to its manager.** Under npm it reads
  the notes and points at `npm` instead of building over a file npm owns, which would
  have left a second binary on the path for npm to overwrite on the next install.
- The published binary is built with fat LTO, one codegen unit and no symbols.
  Nobody waits for a release build, so it takes the opposite side of the trade
  `Cargo.toml` makes for source installs — smaller to download and faster to run.
- **A clean install takes 68 seconds rather than 228.** `codegen-units = 1` compiled
  the binary crate as one unit no core could share — 183 of those 228 seconds, all
  of it serialised at the end. Sixteen units cost 2.4% of scan time and 4MB of
  binary, measured on a 3,456-file corpus, with findings byte-identical.

## 0.1.1 — 2026-09-06

### Added

- **`rqx update`.** rqx publishes no binaries; a tag is the release, and updating
  builds it the same way `cargo install --git` installed it. A scan prints one dim
  line when a newer version has been tagged, with the changelog behind
  `rqx update --notes`. The check runs on a thread nothing waits for, caches for a
  day, and is skipped under CI, off a terminal, with `RQX_NO_UPDATE_CHECK`, or with
  `[update] check = false`.

### Fixed

- **The update check could print into a build log, and never completed.** Its gate
  asked whether the reporter was a coloured kind — true for the default reporter
  piped into a log — and leaned on `CI`, which Jenkins, TeamCity and Azure Pipelines
  do not reliably set. It now requires stdout and stderr to be terminals and names
  those systems too. Separately the worker was spawned after the report and
  abandoned, but `process::exit` does not wait for a thread, so `curl` was killed
  before answering; the check now starts with the scan and is collected at the end.
- crates.io publishing skips with a notice when no token is configured, rather than
  failing the last job of a release that had already published.

### Removed

- **The published binaries.** No tarballs, no `SHA256SUMS`, no five-target build
  matrix — installing and updating both build from source, which is one thing that
  can go wrong instead of five platform artifacts and the pipeline that uploads
  them. The GitHub Action builds from the checked-out commit and caches it.
- **The editor extensions.** The VS Code, Zed and JetBrains clients are gone, along
  with the marketplace publishing that went with them. Every editor rqx supports now
  talks to `rqx lsp` through stock LSP configuration, which `rqx setup <editor>`
  prints — nine of them, none needing anything installed beyond the binary.

## 0.1.0 — 2026-09-06

### Added

- **Baseline and ratchet.** `rqx baseline` records the debt a project already has;
  `[gate] baseline` counts only findings it does not hold, and `[gate] ratchet`
  refuses to let the total grow. [docs](docs/baseline.md)
- **Waivers.** `// rqx-ignore <rule>: <reason>` waives a finding, and the reason is
  not optional — without one the finding stands and `unjustified-suppression`
  reports the comment. `rqx suppressions` audits them. [docs](docs/suppressions.md)
- **Rules you write.** `[[rule]]` blocks holding tree-sitter queries, with
  `rqx query` to try one first. [docs](docs/custom-rules.md)
- **Policy.** `policy = "…"` reads a config from outside the repository, layered
  under the project so a service may still tighten it, with `[locked] keys` naming
  what it may not change. [docs](docs/policy.md)
- **`rqx config set/unset/files`** — change a setting from the command line, in the
  right file, without losing the comments.
- **`rqx complexity`**, **`rqx health`**, **`rqx slop`**, **`rqx recommend`**,
  **`rqx graph`**, **`rqx cache`**, **`rqx completions`**.
- **The `noise` family** — five rules for text that costs a reader and buys nothing.
- `--stats` for counts without findings, `--timings` for where a scan spent itself.
- Four more MCP tools: `complexity`, `health`, `graph`, `recommend`.
- **The interface takes a mouse.** The wheel scrolls, a click selects, a second
  click opens, and the tab bar and key hints are buttons. `--no-mouse` turns
  capture off where drag-select matters more. [docs](docs/interface.md)
- **A command palette** on `ctrl-p`: every tab, command, file and rule, narrowing
  as you type.
- **A menu that holds every command**, in five groups — Analyse, Browse, Report,
  Set up and Session — rather than the five flat entries it had. `RQX.toml`, a
  baseline and clearing the cache can be run from it.
- **Three new screens** — Complexity (and bottlenecks), Graph (the cycle rings and
  the imports) and Health (and slop) — from data the scan already produced.
- **`rqx scan` opens the interface** when it has a terminal and nothing about the
  invocation says otherwise. `--no-tui` prints the report instead.
- **Filters that subtract**: `--not-rule`, `--not-category`, `--not-path`, plus
  `--confidence`, `--fixable` and `--search`.
- **`--detail terse|normal|full`** and a Cycles section in the report.
- `rqx rules --slugs`, for a script or a shell completion.
- **`rqx comment` posts the review onto the merge request.** GitLab and GitHub, one
  comment edited on every re-run rather than repeated, and a commit status carrying
  the grade so the branch shows it without anyone opening a job log. `--dry-run`
  prints what it would post. [docs](docs/ci.md)
- **A scan screen that says what is happening**: the phase it is in, the ones behind
  it and what each took, files against total, lines a second, an estimate, a bar per
  language filling as files land, and the paths going past.
- **Grouped help.** `rqx --help` puts twenty-eight commands under five headings, and
  a test fails the build until a new one is placed. `s`, `c`, `f` and `v` are
  aliases for the four you type all day.
- One dim line each when a run has no `RQX.toml`, and when it is not in a git
  repository and the churn rules therefore found nothing.

### Removed

- **The browser dashboard.** `rqx serve`, the `html` reporter, the `rqx.dashboard`
  LSP command and the editor buttons that opened it are all gone. A second surface
  cost more to keep honest than it returned, and every table it drew is a table the
  terminal interface draws. `rqx` on its own is the interface now.
- `rqx tui` — the interface opens on `rqx` and on `rqx scan` in a terminal; nothing
  is left for the subcommand to name.

### Changed

- **The cache is on by default.** It was off because a warm run was no faster than a
  cold one; that was measured while the collector waited for `git log` before
  draining its channel. With that fixed, a warm scan of 3,440 files costs 298ms
  against 2,023ms cold.
- **A cycle is reported as a ring** — `a.ts → b.ts → c.ts → a.ts` — anchored on the
  import that opens it rather than on byte zero of whichever file sorted first.
- `verify` recognises an Extract Method that stayed in the file it came from.
- **`rqx` scans on launch and opens on your own code**, rather than on a menu. The
  menu is gone; the palette shows the same five groups when nothing is typed and
  ranks matches when something is, and `esc` steps back through where you have been.
- **One key, one meaning.** `s` was severity, sort and slop on three screens; it is
  sort everywhere, `m` flips a screen's other mode, `g` groups, `>` raises the
  severity floor. A test holds every screen's key table to one vocabulary.
- `esc` on the scan screen stops the scan instead of leaving it running behind you.
- The parser gives up on a file after five seconds, so one pathological minified
  file cannot hold a worker forever.

### Fixed

- **Findings could differ between runs** on any project with more than
  `max_findings` of them. The heap that chose which to keep ranked only on severity,
  confidence and rule, so ties were broken by whichever worker finished first.
- **`Identity` is FNV-1a rather than `DefaultHasher`.** Its values are persisted in
  GitLab reports and now in baselines, and the standard library promises nothing
  across versions. GitLab fingerprints change once.
- Release URLs, the VS Code extension's download base and the Action's binary base
  all named a repository that does not exist.
- The VS Code extension registered six commands the language client already
  registers from the server, so it failed to start at all.
- **A monochrome palette printed escape codes.** `--reporter terminal=<path>` wrote
  29 of them into the file and the reading pane showed `[1m` as text: bold is an
  effect rather than a colour, so an empty style still rendered one. Every reporter
  is now asserted to emit no `\x1b` at all when colour is off.
- **`rqx scan .` on this repository took 5.05s; it takes 0.36s.** Resolving imports
  walked the filesystem five levels deep ignoring `prune_dirs`, which on a build
  directory is 209,803 `stat` calls. A cold scan of a 3,456-file TypeScript project
  went from 6.05s to 2.60s: text is sliced rather than re-validated, node kinds are
  compared as integers, the walker carries its own ancestry instead of asking
  tree-sitter to re-descend for a parent, git blames run in parallel, and the cache
  loads beside the walk rather than before it. Findings are byte-identical.
- **The unused-fixture check was cubic** in the size of a test file. A 6,899-line
  pytest file took 36s to scan and now takes 0.47s.
- **The commit status named the wrong commit on GitHub.** `GITHUB_SHA` on a
  pull_request event is the merge commit, which is on no branch. The token also went
  to `curl` as an argument, where `ps` showed it to every other job on the runner.
- `--config` said it read a file "instead of discovering `RQX.toml`" while layering
  it over what it discovered, which is what `docs/configuration.md` describes and
  what it still does. The flag now says so.
