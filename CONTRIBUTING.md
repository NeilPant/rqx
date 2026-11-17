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

## Getting it running

Requirements: Rust stable, at least the `rust-version` in `Cargo.toml`. No other
toolchain is needed.

```sh
git clone https://github.com/jadhamwi21/rqx
cd rqx
cargo test --workspace          # about 800 tests; about twenty seconds once built
./rqx scan ../some-project      # builds, then runs the binary; no install
```

`./rqx` builds the binary and runs it, instead of using `cargo run`. Cargo's progress
output therefore stays out of the interactive session, and paths resolve against your
own working directory.

If the build fails at link time because the default C compiler or linker is missing or
wrong, point `CC` at a working one, for example `CC=/usr/bin/cc`. Cargo also reads
`CARGO_TARGET_<TRIPLE>_LINKER` for the linker.

### Developer commands

`tools/dev.sh` wraps the common tasks and exits `0` when everything passed, `1` when a
check failed and `2` for a usage error or a missing tool.

| Command | Does |
|---|---|
| `tools/dev.sh check` | The CI gates: `cargo fmt --check`, clippy, the tests and the generated-docs check |
| `tools/dev.sh smoke` | Builds a release binary and runs every command against `fixtures/` |
| `tools/dev.sh ci` | The smoke run as CI performs it: `CI=1`, `NO_COLOR=1`, no terminal |
| `tools/dev.sh try PATH [ARGS]` | Runs `rqx ARGS` (default `scan --no-tui`) inside `PATH` |
| `tools/dev.sh session [PATH]` | Opens the interactive session on `PATH` (default `fixtures/typescript`) |
| `tools/dev.sh bench [ARGS]` | Runs `tools/bench/run.sh ARGS` |

`tools/dev.sh help` lists the rest.

### Before opening a pull request

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs those three and fails on any of them. It also regenerates the documentation
under `docs/` and fails if the checked-in copy differs. `cargo run --quiet -- docs`
writes it. Run it after changing a rule, a flag or a configuration key.

### Fixtures

`fixtures/` holds files that are violations on purpose, so that each rule has something
to fire on. The repository's `rqx.yaml` excludes it from scans. Do not run `rqx fix`
over it, and do not correct code in a fixture. When a fixture changes, update the
expected counts in `crates/rqx-cli/tests/fixtures.rs`. The generated rule reference
often changes as well.

## Where things live

| Path | Holds |
|---|---|
| `crates/rqx-core` | The catalogue, findings, scoring and configuration |
| `crates/rqx-lang` | The tree-sitter collectors: one rule set for six languages |
| `crates/rqx-cli` | The command line, the interactive session, the language server, the MCP server and the reporters |
| `fixtures/` | The deliberately broken files, per language |
| `docs/` | The reference. `docs/rules/`, `docs/patterns/`, `docs/cli.md`, `docs/man/` and `docs/rqx.schema.json` are generated, and so is part of `docs/configuration.md` |

[docs/architecture.md](docs/architecture.md) describes the scan pipeline, the crates, the
cache and determinism, and has step-by-step guides for adding a rule, a language and a
slash command.

## Commits

Each commit is one logical change. The message gives the reason for the change, since
the diff already shows what changed. Use a short subject, and add a body when the
reason is not clear from the subject.

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
evidence.

## Style

Code is written to be read without comments. A comment records a reason, never a
description of what the code does; when a comment describes the code, rename the code
instead. Tests are named as sentences that state what must hold.
