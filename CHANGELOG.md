# Changelog

Notable changes, newest first, in the shape
[keep a changelog](https://keepachangelog.com/en/1.1.0/) suggests: Added, Changed,
Removed, Fixed. Dates are when the release was tagged.

Anything user-visible goes under `## Unreleased` in the same pull request that
changes it — `tools/release.sh` turns that section into the release, and the pipeline
posts it as the notes, so an entry written later is an entry nobody reads.

## Unreleased

### Added

- **`/waive`, `/watch` and `/open` in the session.** `/waive src/a.ts:42 rule` is
  `rqx waive`, and asks on the prompt for the reason when `--reason` is missing.
  `/watch on` re-analyses files as they are saved and prints what moved, such as
  `src/a.ts  −1 finding · health 82 → 83`, with `watching` in the status line.
  `/open 2` or `/open src/a.ts:42` opens a finding in `$VISUAL` or `$EDITOR` at its
  line, and the session comes back when the editor exits.
- **`rqx waive FILE:LINE RULE --reason TEXT`.** Writes the `rqx-ignore` comment above
  the line for you, with the file's indentation and comment style. `--until
  YYYY-MM-DD` dates it, `--whole-file` waives the rule for the file, and `--dry-run`
  prints the diff and changes nothing. An unknown rule is refused with the closest
  real one, and so is a blank reason, since the scan would not honour either.
- **`rqx update --version X.Y.Z` and `rqx update --dry-run`.** `--version` installs that
  release instead of the newest, which also lets you step back to an older one.
  `--dry-run` prints how the update would be done, the download URLs and the file it
  would replace, and changes nothing.
- **Man pages.** `man rqx` and one page per command, such as `man rqx-scan`, are
  generated from the same parser as `docs/cli.md` and checked in under `docs/man`.
  Release archives carry them in a `man/` directory beside the binary, which stays at
  the archive root, so the installers and the action extract it as before.
- **`--reporter html=PATH` writes a report you can open in a browser.** One file with
  its own styles and no scripts or network requests: the grade distribution, findings
  by family and by rule, then every file worst grade first with each finding's code
  frame, why it matters and the technique that fixes it, waived findings listed
  separately, and the gate's verdict when one was asked for. It switches between a
  dark and a light theme with the system setting. Like `json` and `sarif` it is a
  document, so on stdout it cannot share the stream with another report.
  `docs/ci.md` has the artifact upload for GitHub Actions and GitLab.
- **JavaScript is analysed.** `.js`, `.jsx`, `.mjs` and `.cjs` files are now read, and
  176 of the 185 rules apply to them. This changes what a scan reports: a repository
  that contains JavaScript will show more findings after upgrading, and `rqx check` can
  fail on code it never looked at before. JavaScript is read as TypeScript without its
  types, so the eight rules that read type syntax (`imprecise-type`, `flag-argument`,
  `boolean-not-a-question`, `catch-all-handler`, `primitive-obsession`,
  `abstract-nothing`, `speculative-generality` and `inappropriate-static`) and the
  pattern advisories that decide from a declared type do not apply. TypeScript and
  JavaScript share one module graph. An import written `./util.js` now finds `util.ts`,
  which also fixes TypeScript projects that use that convention being told the module
  is unused. To keep the old behaviour, name the languages you want with
  `[languages] enabled = ["typescript", "python", "go", "java", "cpp"]`, or skip the
  files with `[ignore] patterns = ["**/*.js"]`.
- **`--timings` says what the scan did.** Beneath the stage times it now prints
  counts: files read, cache hits and misses, walks of the tree, `git` processes and
  blames. A count is the same on every machine, so a warm run that stopped using the
  cache shows up as a number rather than as noise in a timing. The stages gain cache
  load, cache save and render.
### Changed

- **`rqx scan` always prints the report.** In a terminal it used to open the
  interactive interface, which silently dropped `--severity`, `--not-*`,
  `--confidence`, `--fixable`, `--search`, `--since`, `--changed`, `--group-by`,
  `--sort`, `--top`, `--detail` and `--context`. They now apply there as they do
  everywhere else. `--no-tui`, `--no-mouse` and `--motion` are still accepted.
- **Behaviour change: bare `rqx` outside a terminal scans the working directory and
  exits with the scan's code.** It used to print help and exit 0, which made a
  misconfigured CI job look green. Piped, redirected, under CI, on a dumb terminal or
  with `--no-tui`, `rqx` now means `rqx scan`, including the defaults `RQX_LEVEL` and
  `RQX_THREADS` set. In a terminal it still opens the interface.
- **`rqx update` downloads the release instead of building it.** A binary installed with
  `install.sh`, `install.ps1` or from a release page is now replaced by the new
  release's binary, which takes seconds rather than the minute a build did. The archive
  is checked against the release's `SHA256SUMS` first; if the checksum is missing or
  does not match, nothing is installed and the exit code is 2. Installs under
  `~/.cargo/bin` (or `$CARGO_HOME/bin`) are still rebuilt with `cargo install`, and
  installs marked `RQX_MANAGED` are still handed back to their package manager.
  Downloading needs `curl` and `tar`, which Windows 10 and later include.

### Changed

- **Import resolution is faster.** Resolving an import now checks the files the scan
  already walked instead of asking the filesystem about each candidate, and a
  specifier that many files in one directory write is resolved once. The graph and
  every finding are unchanged.
- **Per-file analysis reads the syntax tree once for its hygiene checks.** Twelve checks
  each walked the whole tree and built their own list of its nodes; they now share one
  list per file. Findings are unchanged.
- **rqx uses the mimalloc allocator.** A scan allocates many small short-lived values
  on every worker thread, and mimalloc handles that with about 4% less processor time
  and about 3% more peak memory on a synthetic 4,400-file corpus. It is the `mimalloc`
  cargo feature, on by default; `cargo install rqx --no-default-features` builds with
  the system allocator.
- **Bare `rqx` opens an inline session instead of the full-screen interface.** It
  scans the working directory, prints a header and the worst findings into your
  terminal's own scrollback, and keeps a prompt at the bottom. Nothing takes over the
  screen or captures the mouse, so selecting and scrolling work as usual and what it
  printed is still there after you exit. It needs a window of at least 50x14; a
  smaller one, a pipe, CI or `--no-tui` still gets the plain scan. `--no-mouse` is
  still accepted and does nothing.

### Fixed

- **`scan --watch` no longer writes a clear-screen sequence into a file or a pipe.**
  It cleared the screen before every run whatever stdout was, so a watch redirected to
  a log filled it with escape characters. It now clears only on a terminal, and
  anywhere else prints a `── rescanned after HH:MM:SS ──` line between reports.

## 0.2.1 — 2026-11-17

### Added

- **One line installs rqx.** `curl -fsSL …/install.sh | sh` on macOS and Linux, and
  `irm …/install.ps1 | iex` on Windows, download the release binary for the machine,
  check it against `SHA256SUMS` and install it into `~/.local/bin` or
  `%LOCALAPPDATA%\rqx\bin`. Neither uses sudo or edits shell files. `--version`
  pins a release and `--dry-run` shows every step without changing anything.
- **`--no-progress` turns the progress bar off, and `--quiet` does too.** The bar
  already stayed away from anything but a terminal; on a terminal there was no way to
  silence it short of redirecting stderr. `--no-progress` (or `RQX_NO_PROGRESS=1`) is a
  global flag, and `-q` on `scan` and `check` now implies it.
- **JUnit and GitHub annotations carry the gate.** A gated run adds a
  `<testsuite name="rqx gate">` to the JUnit report with one test that fails when the
  gate does, and ends the GitHub annotations with `::error title=rqx gate::` and the
  failures, or `::notice title=rqx gate::gate passed`. A provider that shows only
  tests or only annotations no longer has to infer a failed gate from a red exit
  code. A run with no gate writes exactly what it wrote before.
- **The SARIF run records its verdict.** `runs[0].invocations[0]` carries the exit
  code the run ends with and one error notification per gate failure, and
  `runs[0].properties.rqx` carries `schema_version` and, for a gated run, the gate. A
  viewer that only has the document can now tell a failed gate from a clean run. Two
  more things are available and off by default, because they change how GitHub
  matches the alerts it already holds: `RQX_SARIF_FINGERPRINTS=1` adds
  `partialFingerprints`, and `RQX_SARIF_CATEGORY=<id>` adds `automationDetails`.
  `docs/ci.md` says what each one does to existing alerts.
- **A gated scan says whether it passed.** The JSON summary line gains
  `gate: { "passed": bool, "failures": [...] }` whenever there is a verdict: a scan
  with a gate flag such as `--fail-on` or `--min-health`, `check`, and `comment`. Before, a pipeline reading the JSON had to rebuild the gate from the
  counts, or trust the exit code alone. `review` is unchanged.
- **Every JSON line says what it is.** Finding lines in `--reporter json` now carry
  `"kind": "finding"`, so a consumer switches on `kind` for every line instead of
  telling findings apart by the keys they lack. The summary line gains
  `schema_version` (`1`) and `rqx_version`, and the summaries of `fix --json`,
  `verify --json` and `plan --json` gain `schema_version`. `docs/json.md` is new and
  states the promise: keys are never removed or renamed, and a consumer ignores keys
  it does not know. The camelCase keys in `verify` and `plan` stay as they are.

### Fixed

- **The action's `findings` output counts what was reported.** It counted every
  `"ruleId"` in the SARIF file, and SARIF lists a finding a waiver silenced too, marked
  suppressed, so a project that waived findings saw a number larger than the report
  showed. The count now leaves suppressed results out and needs `jq`, which the
  GitHub-hosted runners have.
- **`check` counts every finding, not the 500 kept for display.** The review gate
  counted the findings retained under `max_findings` (500 by default), so a branch
  with more findings than that was counted as if it had 500, and a limit above 500
  could never fail. `[gate] max_findings`, `--allow-findings` and `fail_on` now
  count from a per-file tally taken before the cap, the way `rqx scan` already did.
  A config such as `[gate] max_findings = 600` that used to pass `rqx check` on a large
  change now fails when the change holds more than 600.
- **`fail_on = "never"` no longer switches off the health and grade floors.** A
  project that set `fail_on = "never"` together with `min_health` or `min_grade` got a
  gate that enforced neither in `rqx check` and `rqx comment`, and not the health floor
  in `rqx scan`: the setting meant "no finding fails the run by severity" and was read
  as "no gate". `min_health` and `min_grade` now fail a run regardless of `fail_on`. A
  config with `fail_on = "never"` and a floor that used to pass because of this now
  fails when the score or a file's grade is below it.
- **`check` and `comment` honour `ratchet` and `fail_on_incomplete`.** Both settings
  were read only by `rqx scan`, so a pipeline that gated merge requests with
  `rqx check` or `rqx comment` passed branches that grew the baseline's total or left
  a design pattern half applied, however the gate was configured. A project with
  `[gate] ratchet = true` or `fail_on_incomplete = true` that used to pass `check` now
  fails when the total outgrows the baseline, or when a half applied pattern sits in a
  file the review covers. The ratchet compares the project's total, because the
  baseline records one number; the pattern check looks only at the reviewed files.
- **Every `--group-by` ends with the summary and the gate.** Grouping by rule, family
  or severity stopped after the last group, so a gated run printed no summary and no
  `gate failed` line and the exit code was the only sign. `--stats` printed no gate
  either. All of them now end the way the per-file listing does: cycles,
  architecture, summary, hotspots, skipped files, notes, gate.
- **A document on stdout is the only thing on stdout.** `--timings`, `scan --advice`
  and the fix report of `check --fix` wrote their lines to stdout beside the JSON,
  SARIF, JUnit, GitLab or Markdown document, so a pipeline that parsed the output
  failed on the first extra line. They now go to stderr whenever a document is
  printed to stdout; the terminal and GitHub reports keep them where they were.
- **A cache hit is kept.** After one file changed, the cache was rewritten holding
  only that file, so the next run analysed everything else again, and the run after
  paid the same price for the next edit. The cache now keeps every file the run saw,
  hit or miss, and a run that hit every file no longer rewrites it.
- **A blame that crashed no longer shifts the dates of every file after it.** The
  answers were collected without the crashed one, so the files after it picked up the
  neighbouring file's blame: `TODO` ages and waiver ages could land on the wrong
  file. A file whose blame failed now reports no dates and the rest stay in place.
- **A comma or colon no longer cuts a GitHub annotation short.** A path or title with
  a `,` or `:` in it ended the annotation's properties early, so GitHub dropped the
  line number or pinned the warning to the wrong file. Properties now escape both, and
  a rule the project wrote is titled with its own name instead of `custom`.

## 0.2.0 — 2026-11-17

### Added

- **Prebuilt binaries on every release.** Each GitHub release now carries a binary
  for macOS, Linux and Windows on x64 and arm64, with a `SHA256SUMS` file beside them.
  Installing needs no compiler and no Rust toolchain, so a CI job that spent minutes
  building rqx now downloads it in seconds. The Linux build is static musl: one binary
  runs on Alpine, Debian and RHEL with no glibc floor. An npm package wrapping the
  binaries is built and dry-run by every release but not published yet; setting the
  repository variable `PUBLISH_NPM` to `true` turns publishing on.
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

- **`rqx config explain KEY`.** Prints a setting's description, type, default and
  current value, and the file or preset that set it: `rqx config explain
  rules.long-function.severity`. It reads the same schema an editor does, so the
  words match the hover text, and a misspelt key is answered with the closest real one.
- **Per-rule options.** A rule's slug is a key under `rules`, with `severity`,
  `enabled`, `paths`, `confidence` and the thresholds that rule reads:
  `rules: {long-parameter-list: {severity: error, max_params: 6, paths: ["src/domain/**"]}}`.
  The same keys work inside an entry of `overrides`. A threshold written under a rule
  is moved into `thresholds` as the file is read, so layering, policy locks and the
  `from` column treat it as the setting it is. A key under `rules` that is not a rule,
  a list name or an option of that rule is refused with the closest name; the JSON
  Schema lists every rule and its own thresholds. The `perf` preset now reports every
  performance rule as an error.
- **Presets and `extends`.** `extends: [legacy-adoption, ci-gate]` builds a config on
  named sets of settings: `recommended`, `strict`, `legacy-adoption` (relaxed
  thresholds, a gate that fails only on errors, the ratchet on), `ci-gate` (no errors,
  health of at least 70, no critical file) and `perf`. An entry may also be a path to
  another YAML file, relative to the one naming it. Everything extended goes under
  every file rqx reads, so a preset never overrides a policy or a project's own key,
  and `rqx config` shows `preset:ci-gate` in the `from` column for a value it set.
  See `docs/presets.md`.
- **A JSON Schema for the config.** `rqx config schema` prints it and
  `docs/rqx.schema.json` is the checked-in copy, regenerated by `rqx docs`. It is
  derived from the config types, so it lists every key, its default and its
  description, and refuses the keys rqx would. `rqx init` writes a
  `# yaml-language-server: $schema=` line at the top of `rqx.yaml`, which is enough
  for VS Code's YAML extension and others to complete and validate it.
- **`rqx config migrate [PATH] [--dry-run]`.** Converts every `RQX.toml` under a
  directory, the policy files they point at, and the user config to YAML. Comments
  above keys and sections come across, `[[override]]` becomes `overrides:`, and a
  policy pointer is rewritten to the new file. The original is kept as `RQX.toml.bak`,
  and the YAML is parsed back and compared with the TOML before anything is written,
  so a conversion that would change a value writes nothing.

### Removed

- **TOML configuration.** rqx no longer reads `RQX.toml` or `rqx.toml`, in a
  directory, as the user config, as a policy, or through `--config` and `RQX_CONFIG`.
  Finding one stops the run with exit 2 and a message saying so, because ignoring it
  could quietly loosen a CI gate. `rqx config migrate` converts it.

### Changed

- **The action downloads rqx rather than building it.** `jadhamwi21/rqx@v0.2.0` reads the
- **`rqx config set` refuses a value the config would refuse.** It edits the YAML in
  place, keeping comments and layout, and checks the result before writing, so
  `rqx config set thresholds.max_params many` leaves the file as it was and says why.
  Per-rule keys such as `rules.long-function.severity` work like any other.
- **The action downloads rqx rather than building it.** `jadhamwi21/rqx@v1` reads the
  version out of the checkout, fetches that release's binary for the runner and
  verifies it against `SHA256SUMS`. A ref with no published binary — a fork, or
  `@main` ahead of the last release — still compiles from the checkout as before.
- **`rqx update` hands a managed install back to its manager.** An install marked
  with `RQX_MANAGED` gets the release notes and a pointer to the package manager that
  owns it. Building over that file would leave a second binary on the path.
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
