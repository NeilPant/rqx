# Contributing

Issues and pull requests are welcome. A question in
[Discussions](https://github.com/jadhamwi21/rqx/discussions) is a good first step: the
answer is sometimes a documentation fix, which is a smaller change than a code change.

By opening a pull request you agree that your contribution is licensed under the
[MIT licence](LICENSE), the same as the rest of the project. There is no contributor
licence agreement. The [code of conduct](CODE_OF_CONDUCT.md) is one page and asks that
discussion stay on the code and not on the person who wrote it.

## What to expect

| Event | Target |
|---|---|
| First reply to an issue or pull request | within 48 hours |
| Review of a pull request | within 5 days |
| A thread with no reply | ping the thread after 7 days of silence |

These are targets for the maintainers, and a ping on a quiet thread is welcome. A
contributor whose change ships is credited by name in the changelog entry for that
release.

GitHub does not start the checks on a first-time contributor's pull request until a
maintainer approves the run, so a new pull request shows no results for a while. The
checks also have no write access on a pull request from a fork, which is why the
code-scanning upload is skipped there and does not count against the change.

## Getting it running

You need:

- Rust stable, at least the `rust-version` in `Cargo.toml`.
- `git`, and a full clone. A `--depth` clone makes the generated documentation differ,
  because it embeds examples from the history.
- `python3`, which `tools/dev.sh check` uses for the benchmark summary test.
- About 3 GB of disk: roughly 2.3 GB in `target/` for the debug and release builds,
  and 700 MB in `~/.cargo/registry` on a first build.

```sh
git clone https://github.com/jadhamwi21/rqx
cd rqx
tools/dev.sh check              # the one command: see below
./rqx scan ../some-project      # builds, then runs the binary; no install
```

`./rqx` builds the binary and runs it, instead of using `cargo run`. Cargo's progress
output therefore stays out of the interactive session, and paths resolve against your
own working directory.

If the build fails at link time because the default C compiler or linker is missing or
wrong, point `CC` at a working one, for example `CC=/usr/bin/cc`. Cargo also reads
`CARGO_TARGET_<TRIPLE>_LINKER` for the linker.

### The one command

`tools/dev.sh check` runs the gates CI treats as the verdict on a pull request:
`cargo fmt --check`, clippy with warnings denied, the whole test suite and the
generated-documentation check. Run it before opening a pull request.

It takes a few minutes. With a warm build it ran in about two and a half minutes on an
11-core machine, and a cold clone takes several minutes more, mostly compiling. Fewer
cores take proportionally longer. The test suite has about 1,600 tests, and several of
them scan every fixture, so a quiet terminal during the test step is normal. Cargo
prints no progress while a long test runs.

CI adds jobs that `check` does not run: a build on the minimum supported Rust version,
`cargo deny`, `shellcheck` on `install.sh`, the PowerShell installer check, the tests
on macOS and Windows, and a scan of each language's fixtures. A pull request can pass
`check` and still fail one of those.

For the inner loop while editing, run a subset:

```sh
cargo test -p rqx-core -p rqx-lang                          # the catalogue and the collectors
cargo test -p rqx --test fixtures                           # every fixture against its .expect file
cargo test -p rqx --test fixtures every_fixture_finding_is_expected
```

The tests build their own throwaway git repositories and run `git` with your global
and system configuration switched off and signing disabled, so settings such as
`commit.gpgsign`, `core.hooksPath` or a commit template do not change the result. They
remove the directories they create when they finish.

### Developer commands

`tools/dev.sh` wraps the common tasks and exits `0` when everything passed, `1` when a
check failed and `2` for a usage error or a missing tool.

| Command | Does |
|---|---|
| `tools/dev.sh check` | The formatting, lint, test and generated-documentation gates of CI |
| `tools/dev.sh smoke` | Builds a release binary and runs the scan, report and read-only commands against `fixtures/` |
| `tools/dev.sh ci` | The smoke run as CI performs it: `CI=1`, `NO_COLOR=1`, no terminal |
| `tools/dev.sh try PATH [ARGS]` | Runs `rqx ARGS` (default `scan --no-tui`) inside `PATH` |
| `tools/dev.sh session [PATH]` | Opens the interactive session on `PATH` (default `fixtures/typescript`) |
| `tools/dev.sh bench [ARGS]` | Runs `tools/bench/run.sh ARGS` |
| `tools/dev.sh install-test` | Checks `install.sh`; the release lookup needs the network |

`tools/dev.sh help` lists the rest.

### Before opening a pull request

Branch from `main` and open the pull request against `main`. The commands are the
ones the [pull request template](.github/PULL_REQUEST_TEMPLATE.md) lists:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run --quiet -- docs
```

CI runs the first three and fails on any of them. It also regenerates the documentation
and fails if the checked-in copy differs. `cargo run --quiet -- docs` writes it: run it
after changing a rule, a flag or a configuration key, then `git add docs/ README.md`.
It rewrites `docs/rules/`, `docs/patterns/`, `docs/cli.md`, `docs/man/`,
`docs/rqx.schema.json`, `docs/perf-precision.md`, `docs/configuration.md` and the
catalogue table in `README.md`. Edit those through the code that generates them, which
is `crates/rqx-cli/src/report/docs.rs` for the prose, and not by hand.

### Fixtures

`fixtures/` holds files that are violations on purpose, so that each rule has something
to fire on. The repository's `rqx.yaml` excludes it from scans. Do not run `rqx fix`
over it, and do not correct code in a fixture.

`fixtures/<language>/.expect` lists every finding the default configuration reports in
that directory. Each line names a file, an expectation and the source line it applies
to, and the entry finds its line by text, so editing elsewhere in a fixture moves
nothing:

```text
bloat.go rule=no-base-case | func Loops(rows []int) int {
```

`rule=` entries are written by the test harness. The others are written by hand:
`none=` says a rule must stay silent on the line, which is how a false positive that
was fixed stays fixed, and `known-fp=` records a finding that is wrong for a stated
reason. The full table is in
[docs/architecture.md](docs/architecture.md#fixture-expectations).

After changing a rule or a fixture, run:

```sh
cargo test -p rqx --test fixtures every_fixture_finding_is_expected
```

A failure prints one line per difference, followed by the command that accepts them:

```text
fixtures/go/flow.go:154  missing discarded-error
fixtures/go/flow.go:166  unexpected discarded-error
```

If the change is intended, run the same command with `RQX_BLESS=1` and commit the
`.expect` diff with the change. The generated rule reference often changes as well.

The fixtures also feed the checks behind `docs/perf-precision.md` and the golden files
in `crates/rqx-cli/tests/contract/`, which change when a fixture gains or loses lines.
A worked example, from the report to the pull request, is in
[docs/architecture.md](docs/architecture.md#walkthrough-fix-a-false-positive).

## Where things live

| Path | Holds |
|---|---|
| `crates/rqx-core` | The catalogue, findings, scoring and configuration |
| `crates/rqx-lang` | The tree-sitter collectors: one rule set for six languages |
| `crates/rqx-cli` | The command line, the interactive session, the language server, the MCP server and the reporters; its tests are in `crates/rqx-cli/tests/` |
| `fixtures/` | The deliberately broken files, per language, with their `.expect` files |
| `docs/` | The reference. See above for which pages are generated |
| `rqx` | A launcher that builds the binary and runs it; used as `./rqx` |
| `action.yml` | The GitHub Action, which installs a release binary and runs `rqx check` |
| `install.sh`, `install.ps1` | The installers for macOS and Linux, and for Windows |
| `npm/` | The `@rqx/cli` launcher package and its tests |
| `tools/` | `dev.sh` (the commands above), `measure.sh`, `ci-local.sh`, `release.sh`, `labels.sh`, `install-test.sh`, `bench/`, and `render-mascot.py`, which needs Pillow |
| `.github/` | The workflows, the issue forms and the pull request template |
| `RELEASING.md` | How a release is cut and where it is published |

[docs/architecture.md](docs/architecture.md) describes the scan pipeline, the crates, the
cache and determinism, and has step-by-step guides for adding a rule, a language and a
slash command.

## Commits

Each commit is one logical change. The message gives the reason for the change, since
the diff already shows what changed. Use a short subject, and add a body when the
reason is not clear from the subject. A change that users can see adds an entry under
`## Unreleased` in [CHANGELOG.md](CHANGELOG.md), in the same commit.

## What a rule has to be

- **Decidable without types.** rqx parses and does not type-check. A rule may read
  syntax, scope resolution, the import graph and git history. A rule that needs more
  has `Confidence::Low` and is off by default, or is not added.
- **Accurate about what it knows.** `Verified` means the analysis is exact for the
  cases it reports. `Heuristic` means a threshold was compared. `Low` means the rule
  guesses, and these ship off.
- **Named by its fix.** Every rule declares the refactoring that answers it. A finding
  that names a problem without a fix is not accepted.

The checklist for adding a rule is in
[docs/architecture.md](docs/architecture.md#guide-add-a-rule). The compiler reports
most of the steps, and a test fails for a rule that no fixture triggers.

## Precision

Before a rule ships on by default, run it on real repositories and read every finding.
A rule below roughly 70% true positives is `Heuristic` and off until its collector
improves. Put the numbers in the pull request: the repository or files it was tried
on, and the count of true and false positives. The
[rule proposal form](.github/ISSUE_TEMPLATE/rule-proposal.yml) asks for the same
evidence. A change to an existing rule is held to the same standard: show which
findings it removes or adds on real code, and that the ones it removes were wrong.

To measure a rule on code that is not in this repository, build the binary without your
change and the one with it, then compare them:

```sh
git stash                                  # or check out main in a second worktree
cargo build --release && cp target/release/rqx /tmp/rqx-base
git stash pop
cargo build --release
tools/measure.sh /tmp/rqx-base target/release/rqx ~/code/some-project
```

`tools/measure.sh BASE NEW PATH [SCAN_ARGS...]` scans `PATH` with each binary, with the
cache and the update check off, and prints the rules whose finding count differs.
It writes nothing under `PATH` and reports it if anything there changed. Extra
arguments go to `rqx scan`, for example `--config` to switch the rule on when it ships
off. Use `rqx scan PATH --reporter json --max-findings 0` to list the findings
themselves, and read each one that the change adds or removes.

## Style

Code is written to be read without comments. A comment records a reason, never a
description of what the code does; when a comment describes the code, rename the code
instead. Tests are named as sentences that state what must hold.
