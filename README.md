<div align="center">

<img src="assets/mascot-128.png" width="96" height="96" alt="" />

# rqx

**A static analyser for TypeScript, JavaScript, Python, Go, Java and C++.** It reports dead code,
duplication, complexity and architectural drift; states why each finding matters and
what it costs; names the refactoring that resolves it; applies the fixes that have a
single correct answer; and grades a diff against the base branch.

A single Rust binary with no runtime dependencies.

[![CI](https://github.com/jadhamwi21/rqx/actions/workflows/rqx.yml/badge.svg)](https://github.com/jadhamwi21/rqx/actions/workflows/rqx.yml)
[![License](https://img.shields.io/badge/license-MIT-black)](LICENSE)
![Rules](https://img.shields.io/badge/rules-185-0a84ff)
![Languages](https://img.shields.io/badge/languages-6-0a84ff)
![No runtime dependencies](https://img.shields.io/badge/runtime%20deps-none-1f9e55)

`static-analysis` · `code-quality` · `refactoring` · `dead-code` · `lsp` · `mcp` · `rust`

</div>

---

```
rqx                             # scan, then browse the result
rqx scan ./src                  # report everything
rqx fix --write                 # apply the fixes with one right answer, proven first
rqx plan --budget 20            # rank the work, and say what the score will be after
rqx perf --profile cpu.pprof    # what grows faster than its input, and what ran
rqx verify --base main          # was the named refactoring actually made
rqx check --base main           # review only what changed, fail CI on the gate
rqx comment --fail-on error     # post that review onto the merge request
rqx mcp                         # serve the analysis to an agent over MCP
```

## Contents

1. [Install](#install)
2. [The commands](#the-commands)
3. [The report](#the-report)
4. [Anatomy of a finding](#anatomy-of-a-finding)
5. [The catalogue](#the-catalogue)
6. [Design patterns](#design-patterns)
7. [Architectural constraints](#architectural-constraints)
8. [Language support](#language-support)
9. [Automatic fixes](#automatic-fixes)
10. [Verifying a refactoring](#verifying-a-refactoring)
11. [Prioritising the work](#prioritising-the-work)
12. [Adopting it on an existing codebase](#adopting-it-on-an-existing-codebase)
13. [Surveying an unfamiliar codebase](#surveying-an-unfamiliar-codebase)
14. [Editing configuration](#editing-configuration)
15. [Levels](#levels)
16. [Configuration](#configuration)
17. [Enabling and disabling rules](#enabling-and-disabling-rules)
18. [Cross-language duplication](#cross-language-duplication)
19. [Performance rules](#performance-rules)
20. [Coverage as evidence](#coverage-as-evidence)
21. [Confidence levels](#confidence-levels)
22. [Editors](#editors)
23. [Agents](#agents)
24. [The interface](#the-interface)
25. [CI](#ci)
26. [Contributing](#contributing)
27. [Performance](#performance)
28. [References](#references)
29. [License](#license)

Every page under [`docs/`](docs/README.md) goes further than its section here; the
rule reference in [`docs/rules/`](docs/rules) and the pattern reference in
[`docs/patterns/`](docs/patterns/README.md) are generated from the catalogue itself.

## Install

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.ps1 | iex
```

The installer downloads the prebuilt binary for your platform from the latest GitHub
release, checks it against the release's `SHA256SUMS`, and installs it into
`~/.local/bin` (`%LOCALAPPDATA%\rqx\bin` on Windows). It never uses sudo and never
edits your shell files or PATH; when the directory is not on your PATH it prints the
line to add. `--version X.Y.Z` pins a release, `--dir` picks the directory, and
`--dry-run` prints every step without changing anything. Read
[install.sh](install.sh) first if you would rather not pipe a script into a shell.

Each release archive also holds the manual pages under `man/`; copy that directory to
somewhere on your `MANPATH` and `man rqx` works.

To build from source instead, which takes about a minute:

```bash
cargo install --git https://github.com/jadhamwi21/rqx --locked rqx
```

```bash
rqx update                  # install the newest release
rqx update --check          # say whether one exists, install nothing
rqx update --notes          # read what changed
rqx update --version 0.2.1  # install that release instead, newer or older
rqx update --dry-run        # show the method, the URLs and the file it would replace
```

How it installs follows from how rqx was installed. A binary from the installer or a
release download is replaced by the new release's binary, after its SHA-256 matches
the release's `SHA256SUMS`; any mismatch installs nothing and exits 2. A copy under
`~/.cargo/bin` (or `$CARGO_HOME/bin`) is rebuilt with `cargo install`. An install a
package manager owns marks itself with `RQX_MANAGED`, and `rqx update` then reads the
notes and points at that manager rather than replacing a file it does not own.

A scan prints one dim line when a newer version has been tagged, and nothing when it
has not. The check starts when the scan does, on a thread the report waits a quarter
of a second for at the end; its answer is cached for a day.

**It cannot disturb a pipeline.** It is skipped when stdout or stderr is not a
terminal, which is the test that does not depend on knowing every CI system, and
again when any of `CI`, `JENKINS_URL`, `TEAMCITY_VERSION`, `TF_BUILD` or their
equivalents is set. `RQX_NO_UPDATE_CHECK` and `update.check: false` in `rqx.yaml`
turn it off outright, `RQX_MANAGED` marks an install a package manager owns, and no
exit code, report or reporter output changes either way.

**One tag, every channel.** A release builds six binaries, one per platform, and
publishes them beside the crates from the same commit, so a version means the same
thing wherever it came from. Every asset is checksummed in `SHA256SUMS` on the
release.

Editors talk to `rqx lsp` over the Language Server Protocol; `rqx setup <editor>`
prints the configuration each one needs — see [Editors](#editors).


## The commands

Twenty-eight of them, under five headings. `rqx --help` prints this list; each
command's own `--help` prints its flags, and [docs/cli.md](docs/cli.md) is the whole
reference.

| | |
|---|---|
| **The loop** | `scan` · `check` · `fix` · `verify` · `comment` |
| **Look closer** | `complexity` · `perf` · `health` · `slop` · `graph` · `plan` · `patterns` · `recommend` |
| **Reference** | `rules` · `explain` · `techniques` · `levels` |
| **Project** | `init` · `config` · `baseline` · `suppressions` · `cache` |
| **Integrations** | `lsp` · `mcp` · `setup` · `completions` · `query` · `update` |

The four most frequently used have one-letter aliases: `rqx s`, `rqx c`, `rqx f`,
`rqx v`. A twenty-ninth command fails the build until it is placed under a heading,
so the list cannot quietly go back to being flat.

Inside the interface the same list is `/help`.

## The report

Real output, narrowed to two rules so it fits here:

```
  .  2 files · 11 findings · health 66/100

  FAIL   src/orders/reconcile.ts  fair · 2 of 8 findings shown
        Cost: changed 1 time in the last 90 days, 1 module imports it

   ● deep-nesting  7:17  warning
     7 │   private async reconcileBatch(batch: Batch): Promise<number> {
       │                 ^^^^^^^^^^^^^^
     Nesting reaches 7 levels (threshold 3).
     Why:  at this depth the conditions that led here are off-screen, so the
           preconditions of the innermost code are invisible where it matters
     Fix:  Replace Nested Conditional with Guard Clauses — invert the outer
           conditions of `reconcileBatch` into early returns so the innermost
           code sits at depth 1

  Cycles
    src/orders/ledger.ts → src/orders/reconcile.ts → src/orders/ledger.ts

  Files     2 with findings · 0 clean · 2 scanned
  Findings  0 errors · 10 warnings · 1 info · 8 not shown
  Grades    1 clean · 1 fair
  Rules     9 of 185 fired
  Health    66/100
  Duration  0.02s
```

Every finding shows the code it is about, with the offending columns marked in the
severity's colour and a caret ruler underneath for terminals that drop backgrounds.

A rule's `Why:` reads the same on every finding it produces, and a file's churn reads
the same on every finding in it, so each is printed once — the reason on that rule's
first finding, the cost in the file's header. `--detail full` repeats both on every
finding and shows two lines of context either side; `--detail terse` is one line per
finding. `--context <n>` sets the window at any level.

Import cycles, and the contracts a file breaks where an `[architecture]` is declared,
get sections of their own above the summary. Both are ordinary findings as well —
anchored on one member of the ring — but a cycle presented as a cycle is what the
reader is looking for.

Each file carries a grade — `pristine`, `clean`, `fair`, `poor`, `critical` — from
the same curve as the health score applied to that file's own lines. It is what
`Start here` ranks by, and `--min-grade` gates on it, which catches one file rotting
where a project-wide average would not.

Clean files are counted, not listed — `--show-passed` lists them. Narrow the view
with `--severity`, `--rule`, `--category` and `--path`, or by subtraction with
`--not-rule`, `--not-category` and `--not-path`. `--confidence` keeps only what rqx is
certain of or only what it is guessing at, `--fixable` keeps what `rqx fix` could act
on, and `--search` matches the rule, the file or the evidence. None of them move the
health score or the gate, because a CI result that depended on how it was printed
would be worthless.

## Anatomy of a finding

Every finding carries four things, and none of them are optional:

```
  src/orders/reconcile.ts:88:3  cognitive-complexity  ⚠ warning
      88 │   private async reconcileBatch(batch: Batch): Promise<Result> {
         │                 ^^^^^^^^^^^^^^
      Cognitive complexity 25 (threshold 15), max nesting depth 6.
      Why: every nested decision point multiplies the paths a reader has to trace,
           and untraced paths are where regressions survive review
      Cost: changed 23 times in the last 90 days, 14 modules import it
      Fix: Decompose Conditional / Extract Method — lift the conditions in
           `reconcileBatch` into named predicates, then extract the deepest block
```

The **measured fact**, the **reason it matters**, what it **costs** given how often
that file changes and how much depends on it, and a **named refactoring** — never
"simplify this".

Each rule is traceable to an established catalogue, and that traceability is
enforced by the compiler: `RuleId::spec()` is an exhaustive match, so a rule that
does not declare its metric and fix does not build.

## The catalogue

<!-- rqx:catalogue -->
185 rules across 11 families. Every rule names the metric that decides it and the refactoring that answers it, and neither is optional.

| Family | Rules | What it measures |
|---|---|---|
| **Size** | 16 | units that outgrew what a reader can hold |
| **Abstraction** | 17 | types that model the problem wrong |
| **Change** | 7 | shapes that make an edit expensive |
| **Waste** | 33 | code and comments nothing reaches |
| **Coupling** | 18 | modules that know too much about each other, and dependencies pointing the wrong way |
| **Flow** | 17 | control flow that says less than it looks like |
| **Naming** | 12 | names that mislead or say nothing |
| **Safety** | 28 | silent failure and swallowed errors |
| **Tests** | 18 | suites that pass without checking |
| **Noise** | 9 | text that costs a reader and buys nothing |
| **Performance** | 10 | work done more often, or more expensively, than it needs to be |
<!-- /rqx:catalogue -->

Those pages are generated from the catalogue by `rqx docs`, and a test fails if they
drift from it — a hand-written table of sixty-odd rules is a table that goes stale, which
is what the one here had already done.

`rqx techniques` prints every refactoring the rules prescribe, grouped by the kind of
move it is, with the rules that ask for it. A technique nothing prescribes fails the
suite, because an unreachable menu entry is worse than no entry.

### What is deliberately absent

Some things are visible in source and still not decidable from it. Whether a comment
still matches the code beneath it, whether an algorithm is the right one, whether
behaviour a reader expected is missing — no metric settles these, and a tool that
claimed otherwise would be inventing findings. That costs more trust than the missing
rules do.

Rqx also parses without resolving types, so anything that needs a type checker to be
certain is either reported at lower confidence or not at all, and says which.

## Design patterns

A third catalogue, held to the same standard: all twenty-three patterns from *Design
Patterns*, each with the static shape that decides it and the reason Rqx declines
where it does. `rqx patterns` reads a project, `rqx patterns --coverage` prints the
whole table, and `rqx explain decorator` prints one.

```
  Structural

  Decorator                   add behaviour to one object without touching its class
    ✓  1 place
    ●  src/mail/Retrying.java:12:7  half applied
        `RetryingSender` wraps a `Sender` and implements nothing
        give `RetryingSender` the interface `Sender` answers to, or callers have to
        know which of the two they are holding
```

Three things are said, and they do not rank. A pattern **in use** is inventory. One
**half applied** is a defect: a decorator that cannot substitute for what it wraps, a
subscribe with no unsubscribe. One that **would fit** is a suggestion about a shape
already in the code.

**None of it moves the health score.** A pattern that would fit is not a finding, and
a score that fell because code declined to use Strategy would stop meaning anything.
Advice is not a `Finding` and has no route to `FindingCounts`; a test scans the
fixtures with every advisory on and off and compares the summary byte for byte. The
one way it reaches an exit code is `[gate] fail_on_incomplete`, which sees only the
half-applied half.

Confidence is per advisory, not per pattern: recognising a Decorator from what it
holds is `verified`, reading a Bridge out of a cross-product of subclass names is a
`measured threshold`, and pairing a save with a restore by name is `low confidence`
and ships off. [`docs/patterns/`](docs/patterns/README.md) is the reference.

## Architectural constraints

ArchUnit covers Java, dependency-cruiser JavaScript, import-linter Python. None of
them covers a polyglot repository from one configuration, which is where layering
drift tends to accumulate: no single tool observes the whole graph.

```yaml
architecture:
  layers: [domain, application, adapters]
  paths:
    "src/domain/**": { layer: domain }
    "src/application/**": { layer: application }
    "src/adapters/**": { layer: adapters }
    "src/scripts/**": { may_only_import: ["src/adapters/**"] }
```

`layers` is ordered inner to outer: a file may import its own layer and any layer
before it, so the domain cannot reach for an adapter. The finding points at the import
line, in whichever of the six languages wrote it, because it reads the same module
graph the cycle detection already uses.

The check is silent until declared: an architecture that has not been written down
is not one rqx infers. A layer named in `paths` but absent from `layers` is an error
rather than a rule that silently governs nothing.

## Language support

```yaml
# rqx.yaml — all six are enabled unless stated otherwise
languages:
  enabled: [typescript, javascript, python, go, java, cpp]
```

Every rule is written once, against measurements a language backend fills in: how
many statements a body holds, which names it binds, what it takes from other files.
A language contributes a table of node kinds, a reader for its module surface and an
import resolver; nothing else about it reaches the rules.

| | TypeScript | JavaScript | Python | Go | Java | C++ |
|---|---|---|---|---|---|---|
| **Extensions** | `.ts .tsx .mts .cts` | `.js .jsx .mjs .cjs` | `.py .pyi` | `.go` | `.java` | `.cpp .cc .cxx .hpp .hh .h .ipp` |
| **Export is** | `export` | `export` | a module-level name without `_`, or `__all__` | a capital initial | `public` | a declaration in a header |
| **Imports resolve via** | extensions, `index`, `tsconfig` paths, `package.json` | the same, and one graph with TypeScript: `./x.js` finds `x.ts` | packages and `__init__.py`, relative dots | `go.mod` module path, package directories | package to directory under a source root | `#include "…"` against the file and include roots |
| **Entry points** | `package.json` `main`/`exports`/`bin` | `package.json` `main`/`exports`/`bin` | `__main__.py`, `main.py`, `conftest.py`, `test_*`, `[project.scripts]` | `package main`, `_test.go` | `static void main`, `@Test` | every translation unit, `TEST(…)` |
| **Tests** | Jest/Mocha/Vitest | Jest/Mocha/Vitest | pytest, unittest | `go test` | JUnit | GoogleTest |
| **Rules that apply** | 184 | 176 | 174 | 139 | 171 | 169 |

JavaScript is read as TypeScript without its types. TypeScript and JavaScript files
in one project share a module graph, so an import of `./util.js` that is really
`util.ts` counts, and neither language reports the other's exports as dead. The eight
rules that read type syntax do not apply to JavaScript.

A rule that cannot exist in a language says so, in the catalogue, with a reason:

```
$ rqx explain refused-bequest

  Not applicable to
    Go: nothing is inherited in Go, so nothing can be refused: a type either
        has a method or does not
```

`rqx rules --language go` lists what does apply. Two tests hold this honest: every
applicable rule must fire on that language's fixtures, and every exempted rule must
stay silent on them — so a rule that quietly stops working in one language fails the
build rather than going unnoticed.

## Automatic fixes

Most findings name a refactoring, and choosing its shape is the work. Some do not:
nothing about deleting an unused local is a design decision, and rqx already located
the exact bytes and proved nothing references them.

```
$ rqx fix ./src

  FIX   orders/reconcile.ts  2 fixes

   ● unreachable-code  52:5  deletion
    52 │ -   return cached
    53 │ -   log.debug('unreachable')
     Why:  this code cannot run, so whatever it was meant to do is not happening

  Files     3 to fix · 42 scanned
  Fixes     11 ready · 2 withheld · 46 need a decision
  Health    73 → 81  (+8)
  Verified  every fix re-parsed and re-analysed clean

  Nothing written.  rqx fix --write
```

**Nothing is written unverified.** Each file's edits are applied in memory, re-parsed
for the error nodes tree-sitter invents when it recovers, and re-analysed: the targeted
findings must be gone and nothing new may appear. A file that fails is named with the
reason rather than passed over silently. Writes are atomic, and one fix making the next
possible is why it repeats — deleting the branch that held a binding's only reference
is what makes that binding unused.

Thirteen rules qualify today. `[fix] disabled = ["commented-out-code"]` opts one out;
`rqx explain <rule>` says whether a rule is fixable. The other 172 are
`rqx fix --plan`, which emits them with the span, the source text, the metric, the
reasoning and the named refactoring — the brief for a person or an agent, in the shape
they can act on. `rqx check --base main --fix` does the same for a branch and judges
the gate afterwards, so a branch whose only failures were mechanical passes.

Editors offer the same edits, verified the same way and tagged `source.fixAll`, so
they can be applied on save.

## Verifying a refactoring

rqx names a refactoring for every finding, but naming one does not establish that
it was carried out. `rqx verify` grades a diff against the refactoring it claimed to
perform, which matters most where the diff was written by an agent.

```
$ rqx verify --base main

   ● resolved     unreachable-code
       subject   console.log('dead')
       as        consistent with Remove Dead Code

   ● relocated    cognitive-complexity
       subject   reconcileBatch
       where     now at orders/helpers.ts:41

  Resolved    1
  Relocated   1
  Health      73 → 74  (whole project)

  ✖ the diff moved problems rather than removing them
```

`relocated` is the one nobody else reports: a body moved into a new helper takes its
complexity with it, and a tool that called that "resolved" would be endorsing the
wrong diff. Findings are matched by what they are about — rule, file and subject —
never by line, so moving code around does not read as fixing it.

`--expect cognitive-complexity@orders/reconcile.ts` fails the run unless that
particular finding was resolved. It is a `verify` MCP tool too, so an agent can
propose, be graded, and try again.

## Prioritising the work

A list of findings ranked by severity is the conventional output and is rarely
acted on. `rqx plan` ranks the work by what each item buys against what it costs, and
projects the health score after each one:

```
$ rqx plan ./src --budget 12 --simulate

  The plan  9 items in 12 points of effort

    1. ● empty-catch  orders/reconcile.ts:17:5
       Remove Dead Code · 1 point · health 76
    2. ● cognitive-complexity  orders/reconcile.ts:88:3
       Decompose Conditional / Extract Method · 2 points · health 81

  Effort    12 points
  Health    73 → 84  (+11)

  Grades
    critical  2 → 0
        fair  9 → 4
```

Value is the weight the finding carries in the score, scaled by what the file costs to
change — churn and how many modules import it. Effort comes from the refactoring the
rule prescribes: deleting is one point, moving behaviour up or down a type tree is
five. The selection is exact rather than greedy, because the cheapest two items are
often worth more together than the dearest one.

## Adopting it on an existing codebase

A codebase of any age arrives with a large existing debt. A gate switched on
against it fails the first pull request and is switched off by the second. Four
features exist to make adoption incremental.

**Record the existing debt.**

```sh
rqx baseline            # writes .rqx/baseline.json — commit it
```

```yaml
gate:
  fail_on: warning
  baseline: .rqx/baseline.json
  ratchet: true          # and it may not grow
```

```
  Findings  0 errors · 8 warnings · 2 info
  Baseline  7 findings known · 1 new
```

Eight warnings, still reported, still in the editor. Seven are debt the baseline
recorded; one arrived with this change, and only that one fails the build. The health
score still counts all of them — the score is a statement about the code, not about
which debt has been agreed to. [More](docs/baseline.md).

**Waive one finding, with a reason.**

```ts
// rqx-ignore long-function: the parser is generated, PLAT-812
```

Without the reason it waives nothing: the finding stands and
`unjustified-suppression` reports the comment. `until 2027-01-01` puts a date on it,
and past that date it lapses the same way. `rqx suppressions` lists every waiver with
its reason and who wrote it. [More](docs/suppressions.md).

**Write project-specific rules.**

```yaml
rule:
  - id: no-legacy-client
    languages: [typescript]
    query: |
      ((call_expression function: (member_expression object: (identifier) @o)) @site
        (#eq? @o "legacyClient"))
    message: legacyClient is retired; call ApiClient
    family: coupling
    severity: error
```

A tree-sitter query, and every grammar is already in the binary. It counts towards the
score, shows in the editor, gets its own SARIF descriptor, and can be disabled or
waived by name. `rqx query typescript '<query>' --captures` tries one before
commit to it, and a query that will not compile is a config error naming the rule and
the row it failed at. [More](docs/custom-rules.md).

**One standard across many repositories.**

```yaml
policy: ../platform/rqx-policy.yaml
```

The policy sits *under* the project, so a service may still tighten what it likes.
`locked.keys` names what it may not change at all — not in its own config, not in an
override, not with `--level`. [More](docs/policy.md).

## Surveying an unfamiliar codebase

```sh
rqx recommend      # which thresholds fit, which refactoring pays, what to read first
rqx complexity     # the measurements, as a distribution rather than a tail
rqx health --by rule
rqx graph --cycles --format mermaid
rqx slop           # which files read as written rather than read
```

`rqx complexity` is the most useful first command on an unfamiliar tree:

```
  Complexity  39873 functions measured

                        cplx  stmts  nest args  where
    (anonymous)           61    102     5    1  …/chart/layers/CandleLayer.tsx:82
    TradingChartCanvas    60    221     4    1  …/chart/TradingChart.tsx:230
    submit                53    102     5    3  …/challenges.service.ts:658
    projectMarkdown       52     84     4    1  …/markdown-projection.ts:30

    p50 0 · p90 2 · p99 11 · 180 of 39873 over the threshold of 15
```

The final line establishes whether the threshold describes the exceptions or the
norm. `rqx recommend` proposes moving it in the first case and stays silent in the
second.

`rqx graph` prints the import graph as text, Graphviz, Mermaid or JSON, with the edges
that close a cycle marked in each — and `circular-dependency` reports the ring as a
walk that can be followed rather than a count, anchored on the import that opens it.

`rqx slop` scores how much of a file is text that costs a reader and buys nothing: a
comment above every line restating it, a divider in a fifty-line file, `example.com`
in a constant, a leftover *Here's the implementation you asked for!* — each is nothing
alone; together and at density they describe code that was produced and never read
back. It is labelled a heuristic everywhere it appears and gates nothing unless asked.

## Editing configuration

```sh
rqx config                                   # the settings, and which file set each
rqx config set thresholds.max_params 3       # write it, comments and layout intact
rqx config set gate.fail_on warning --pick   # choose which file in the chain
rqx config unset thresholds.max_params
rqx config files                             # the chain, nearest last
```

A monorepo reads a user config, a policy and one `rqx.yaml` per directory, so `set`
writes to the file that already holds the key, and `--pick` asks for one when a
different one. Edits go through a format-preserving writer: the template is mostly
prose explaining what each key is for, and a reserialise would throw all of it away on
the first change. A value the config would refuse is refused before the file is written. A key the policy locked is refused before the file is touched.

## Levels

How much a project asks of itself is one word:

```yaml
level: strict
```

| Level | Rules | Gate | For |
|---|---|---|---|
| `relaxed` | 155 of 185 | error | legacy code that has to stay shippable while it is cleaned up |
| `standard` | 164 of 185 | error | the catalogue as it stands |
| `strict` | 168 of 185 | warning, min grade `fair` | held to what a reader can follow without effort |
| `pristine` | 185 of 185 | info, min grade `clean`, health 90 | every rule, including the ones that ship off |

A level is merged **under** everything the project states, so it moves the defaults
and never overrides a stated value — `rqx config` says `level:strict` in the `from` column
for a value the level moved, and names the project's file for one it set. `--level strict`
overrides the file, because CI usually wants a stricter standard on the default branch
than a working copy does, and an entry of `overrides` may name its own level, so a project can
be strict in its domain and relaxed in its scripts. `rqx levels` prints each one as the
settings it would take to write by hand.

## Configuration

`rqx init` writes an `rqx.yaml` with every setting at its default and a line of prose
per table, and a `# yaml-language-server: $schema=` line that gives an editor
completion and validation from [`docs/rqx.schema.json`](docs/rqx.schema.json).
`rqx config` prints what a run will actually use and names the file or preset each
value came from, and `rqx config explain thresholds.max_params` says what one setting
is, what it defaults to and why it has the value it has. `rqx.yml`, `.rqx.yml` and
`.rqx.yaml` are read too; two in one directory is an error.

rqx no longer reads TOML. A directory holding an `RQX.toml` stops the run with exit
code 2, because skipping it could change what a pipeline gates on, and
`rqx config migrate` converts it, its policy files and the user config to YAML,
keeping comments and the originals as `.bak`.

Layers merge, lowest first: the built-in defaults and the `level`, then every preset
and file named by `extends`, then the user's `~/.config/rqx/rqx.yaml`, then the policy
a project points at, then every `rqx.yaml` from the scanned directory upward
(outermost first, stopping at the repository root), then `RQX_CONFIG`, then
`--config`, then the command's own flags. So `rqx scan src/` in a monorepo uses the
project's config rather than silently running on defaults.

**Start from a preset.** `extends` builds on named sets of settings, or on other
files, and the file's own keys win:

```yaml
extends: [legacy-adoption, ci-gate]

thresholds:
  max_params: 5
```

`recommended`, `strict`, `legacy-adoption`, `ci-gate` and `perf` are built in
([docs/presets.md](docs/presets.md)).

**Settings for one rule.** A rule's slug is a key under `rules`:

```yaml
rules:
  long-parameter-list:
    severity: error
    max_params: 6
    paths: ["src/domain/**"]
```

A subtree with different standards can relax the tables that are decided per file:

```yaml
overrides:
  - paths: ["src/generated/**"]
    thresholds:
      long_function_statements: 200
```

Nothing else may be scoped. The file walker is built once and the gate reaches one
verdict, so an override naming `scan` or `gate` is an error rather than a setting
that quietly does nothing. [docs/configuration.md](docs/configuration.md) has the
whole schema, all 44 thresholds and the rules each one tunes.

## Enabling and disabling rules

```yaml
rules:
  categories: [waste, coupling]           # only these families
  disabled: [magic-number]                # or a single rule
  enabled: [short-name-wide-scope]        # opt into an off-by-default rule
```

Twenty-one rules ship off by default — the ones whose signal is real but noisy, or
which encode a convention a given project may not share. They are listed with
`(off by default)` in `rqx rules`.

## Cross-language duplication

Clone detection is per-language everywhere, including here: the hash reads node ids a
grammar assigns, and no two grammars agree on them. A second hash reads a vocabulary
none of them owns — branch, loop, call, return, assign, and the canonical operator —
so a validation rule ported from TypeScript to Python still matches.

```
● cross-language-duplicate  api/validate.py:4
  9 statements, 51 nodes, 2 occurrences
  the same shape is in TypeScript and Python; give the rule one home and call it
  from both, or write down why they are allowed to differ
```

Off by default and reported at low confidence, which keeps it out of the health
score. Two blocks that merely branch and return in the same order are not the same
rule, so a group has to span languages, clear eight statements and forty nodes, and
share a constant before it is reported — ported code keeps its constants even when
its syntax changes completely.

## Performance rules

Much of what a profiler eventually identifies is visible in the source: a query
inside a loop, a linear search inside a loop, an await per item, a pattern recompiled
on every pass. Ten rules read those shapes, and
`rqx perf` ranks the functions carrying them by what they cost against how much of
the project leans on the file they are in.

```
$ rqx perf --top 2

  Bottlenecks  8 functions worth a look

                   order  loops   io  reach   sampled  where
    reconcileBatch  O(n²)      2    1   ×3.4    41.2%  orders/reconcile.ts:88
                  `db.order.findMany` runs on every pass of the loop. On the stack in
                  41.2% of samples.  · Fix: Batch the Work
    fibonacci         exp      0    0   ×1.1        –  math/series.ts:12
                  Calls itself 2 times per invocation with no memo in sight. A profile
                  never sampled this function.  · Fix: Memoize

    p50 0 · p90 1 · p99 2 loop depth · 14 over 2 · 9 with a round trip inside a loop
```

`order` is loop order, not a proof — three nested loops over three items are three
passes of nothing. Which is why a CPU profile can settle it. `--profile cpu.pprof`
reads a V8 `.cpuprofile`, a Go pprof or collapsed stacks by content, and a
performance finding in a function the run never put on the stack keeps its finding
and stops counting, exactly as a covered symbol stops counting as dead code.

Names are the weakest evidence here, so the rules leaning on one are `heuristic` and
never `error`, and a receiver that is already keyed — a `Set`, a `Map`, anything
named `index` or `cache` — is a lookup rather than a round trip and is not reported.
[docs/perf.md](docs/perf.md) has the columns, the formats and the vocabulary a
project can extend.

## Coverage as evidence

Dead code is decided from the import graph, and a graph cannot see a dynamic import,
a container wiring things up by name, or reflection. A coverage file settles it:

```bash
rqx scan . --coverage coverage/lcov.info
```

A file or export the graph called dead that a test run reached keeps its finding —
the missing import may still be worth adding — but drops to `low` confidence, which
means it is shown and stops counting against the health score. One nothing reached
stays `verified`, and now says so with evidence rather than only from the graph.

lcov, Go cover profiles and coverage.py JSON are recognised by content, several files
merge, and a coverage file that cannot be read is skipped with a warning rather than
failing the scan.

## Confidence levels

A finding says how sure it is. `unused-parameter` claims **verified** only for a
trailing parameter of a function whose signature its author controls. A parameter
in the middle cannot be deleted without renumbering every call site, and a
callback's arity is fixed by whatever calls it — so those are reported as
**heuristic**, or not at all.

Findings that cannot be verified are omitted rather than asserted. Ignored files
are never parsed, which is why tests are *not* ignored by default: an invisible
import manufactures false "unused export" findings.

## Editors

```
rqx setup            # every editor rqx knows
rqx setup neovim     # the config it needs
rqx setup neovim -w  # append it, after showing what will be added
```

| | Diagnostics | Cross-file rules | Hover | Quick fixes | Code lens | Commands |
|---|---|---|---|---|---|---|
| **Neovim** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Helix** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Sublime Text** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Emacs** eglot | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Emacs** lsp-mode | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Vim** coc.nvim | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **Vim** vim-lsp | ✓ | ✓ | ✓ | ✓ | — | — |
| **Kakoune** kak-lsp | ✓ | ✓ | ✓ | ✓ | — | — |
| **Kate** | ✓ | ✓ | ✓ | ✓ | — | — |

There is no extension to install and none to keep up to date: every client above is
stock LSP configuration against the `rqx lsp` binary, so a capability added to the
server reaches all of them in the same commit. Any editor with an LSP client works
the same way. For a sidebar of findings, files and rules, `rqx` in a terminal split
is that view.

Quick fixes are the same edits `rqx fix` applies, verified the same way and tagged
`source.fixAll` so an editor can apply them on save. The rules whose fix is
a decision offer to explain themselves or turn the rule off instead.

Cross-file findings arrive without being asked for: the server scans the workspace on
its own and republishes every file, so the rules that need the whole import graph do
not go silently missing in an editor that has one file open.

[docs/editor.md](docs/editor.md) has the commands, the custom notifications and a
paste-in block per editor.

## Agents

`rqx mcp` runs an MCP server, so a coding agent can query a project's problems
rather than inferring them by reading files. It answers with the same four things every
finding carries — the measured fact, why it matters, what it costs and the named
refactoring — which is exactly what an agent asked to "clean this up" otherwise invents.

```sh
claude mcp add rqx -- rqx mcp
```

Answers are summaries with drill-down, never dumps. `scan` returns health, the grade
distribution and what to open first; the findings themselves are paged out of `findings`,
narrowed by rule, severity or path. A full report of a 12,000-file repository does not fit
in a context window, and a tool that returned one would leave nothing to fix it with.

| Tool | Answers |
|---|---|
| `scan` · `findings` | The project's health, then its findings a page at a time |
| `check` | The diff against a base revision, and whether the gate passes |
| `plan` | The findings `rqx fix` will not touch, each with its source, metric and named refactoring |
| `verify` | What a diff did to every finding: resolved, improved, relocated, introduced |
| `analyze` | An unsaved buffer, without touching the disk |
| `perf` | Where the work grows faster than the data, ranked by cost against reach |
| `complexity` · `health` | The distributions the score comes from, and what each part costs |
| `recommend` · `graph` | What to do about the project, and what imports what |
| `patterns` · `explain` · `rules` · `config` | The catalogues, and the settings in force |

All fifteen are read-only: nothing rqx exposes to an agent writes a file. `plan` returns
the brief and the agent does the writing. `--http` serves
the same tools over Streamable HTTP for clients that cannot spawn a process, and refuses
a public address without `RQX_MCP_TOKEN` rather than exposing the source to a network.
[docs/mcp.md](docs/mcp.md) has the payload shapes and a config snippet per client.

## The interface

`rqx` on its own opens an interactive session when it has a terminal to open it in.
Piped, redirected, run under CI, in a window smaller than 50 by 14 or given
`--no-tui`, it scans the working directory instead, exactly as `rqx scan` would.
`rqx scan` and every other subcommand always print their report.

The session lives at the bottom of your terminal. It uses no alternate screen and
does not capture the mouse, so what it prints lands in your scrollback and stays
there after you leave, and you select, search and scroll it with the terminal's own
tools.

It scans as it opens. While it works, the owl glances left and right beside the
phase and the files done; when the scan lands, a header and the worst findings go
into scrollback and the owl settles into the colour of the verdict and blinks. The
status line shows health, grade, findings, watch state and cache hits.

**A slash command is the command line's command.** `/plan --budget 20` is parsed by
the same definition as `rqx plan --budget 20`, runs on the scan the session already
holds, and prints through the same functions, so the two cannot differ. Text with no
slash searches the findings.

```
> /plan --budget 20
> /findings --severity error --group-by rule
> dead export
```

Typing `/` opens a completion menu: command names, then that command's flags, then
the values a flag takes, which are rule slugs, families, files from the scan or the
choices the flag lists. `tab` accepts, `esc` closes. The line edits with the arrow
keys, `home`, `end`, `ctrl-u` and `ctrl-w`; `↑` and `↓` walk a history that is kept
per user outside the repository. `/rescan` scans again, `/clear` clears the screen,
`/help` lists the commands, and `/exit`, `ctrl-d` or `ctrl-c` twice leaves.

`--motion off` draws the owl still. [docs/interface.md](docs/interface.md) has the
commands, the keys and the rules for when it opens.

## CI

`rqx check --base main` reviews the changed files plus everything that imports
them, and exits non-zero when the gate fails.

| Exit code | Meaning |
|---|---|
| `0` | ran, gate satisfied |
| `1` | ran, gate failed |
| `2` | rqx could not run |
| `101` | rqx hit a bug; please report it |

A reader that walks away is not a failure: `rqx scan . \| head` exits `0`, because the
scan did what it was asked and the pipe closing is the reader's decision.

One analysis pass feeds every consumer. `--reporter <name>` writes to stdout,
`--reporter <name>=<path>` writes to a file, and both repeat:

```sh
rqx scan . \
  --reporter terminal \
  --reporter junit=rqx.xml \
  --reporter sarif=rqx.sarif \
  --fail-on error
```

`terminal`, `json`, `sarif`, `junit`, `gitlab`, `github` and `markdown` — between
JUnit XML and GitLab Code Quality that covers GitHub, GitLab, Jenkins, CircleCI, Azure
Pipelines, Bitbucket and Buildkite natively. Copy-paste snippets for each
are in [docs/ci.md](docs/ci.md); `action.yml` wires up GitHub Actions in five
lines.

**`rqx comment` posts the review onto the merge request it is reviewing**, on
GitLab and GitHub alike. It reads the project, the merge request and the token
from the CI environment, writes one comment — a verdict line, the counts, and the
worst findings folded behind a `<details>` — and edits that same comment on every
later run instead of adding another. It also grades the commit, so the merge
request shows `rqx: B · 12 findings` beside the other checks without anyone
opening a job log.

```sh
rqx comment --fail-on error
```

There is no `--base` there because the pipeline already knows it: GitLab hands over
`CI_MERGE_REQUEST_DIFF_BASE_SHA` and GitHub the pull request's base, both of them
commits that exist in a CI clone — which `origin/HEAD` is not. Pass `--base` to
override it.

`--dry-run` prints what it would post and posts nothing, which is the right first
run. `--marker` keeps two jobs' comments apart on one merge request. `--no-status`
comments without grading. The exit code is the gate's, so the job fails the same way
`check` does. GitLab needs a token with `api` scope — `CI_JOB_TOKEN` cannot write
merge-request notes — and GitHub needs `pull-requests: write`.

The gate lives in `rqx.yaml` and every part of it can be overridden per run with
`--fail-on`, `--allow-findings` and `--min-health`, so a strict gate on the
default branch and a lenient one on feature branches do not need two config files.

```yaml
# rqx.yaml
gate:
  fail_on: warning
  max_findings: 0
```

## Contributing

One Rust workspace, no other toolchain:

```
./rqx scan ../some-project      # runs from the source tree, no install
RQX_RELEASE=1 ./rqx scan .      # optimised, for timing runs
cargo rqx rules                 # same thing through cargo
cargo test --workspace
```

`./rqx` builds and then execs the binary rather than using `cargo run`, so cargo's
progress output never lands in the middle of the interface, and paths still resolve
against the current working directory. Build output goes to stderr, so
`./rqx scan . --reporter json > out.json` stays valid JSON. To install it properly:
`cargo install --path crates/rqx-cli`.

Rules are a declarative table, and [CONTRIBUTING.md](CONTRIBUTING.md) walks through
adding one — the compiler names most of the steps, and a rule no fixture can trigger
does not compile. Precision numbers from a real repository are the review.

[Code of conduct](CODE_OF_CONDUCT.md) ·
[Security](SECURITY.md) ·
[Releasing](RELEASING.md) ·
[Changelog](CHANGELOG.md) ·
[Discussions](https://github.com/jadhamwi21/rqx/discussions)

## Performance

Measured on an M3 Pro — five performance cores, six efficiency cores — with the
cache off for the cold numbers:

| | Files | Code lines | Cold | Warm |
|---|---:|---:|---:|---:|
| A TypeScript codebase | 3,456 | 461,090 | **3.2s** | **0.41s** |
| Five languages in one tree | 4,440 | 178,344 | **1.6s** | **0.67s** |

Warm is the same answer from `.rqx/cache.bin` for every file whose contents have not
moved; the cross-file stage — the import graph, the clone index, the churn rules —
runs again either way, because a deleted import changes what the graph says about
files nobody touched.

The walk scales with the cores it is given. On the TypeScript codebase: one thread
24.3s, two 12.3s, four 6.4s, eight 3.8s, all eleven 3.3s — **7.3× on 5+6**, which is
close to what that mix of cores can give.

**Output is byte-identical whatever the thread count**, so a CI gate is never flaky;
a test asserts it. Memory is a function of thread count rather than file count, so a
large repository does not need a large machine.

Roughly 90% of a cold scan is the per-file parse and walk. The stages after it —
duplicates, the import graph, git history, co-change — are together about 200ms of
the 3.2s above, which is why the cache pays: it skips the first part and keeps the
second.

## References

Every rule names a metric and a refactoring, and neither is invented here. The
catalogues rqx is traceable to, and what it takes from each:

| Source | What rqx takes from it |
|---|---|
| Martin Fowler, *Refactoring: Improving the Design of Existing Code*, 2nd ed. (Addison-Wesley, 2018) | The refactoring names every finding prescribes — Extract Method, Move Method, Replace Nested Conditional with Guard Clauses — and most of the smells behind the Size, Abstraction, Coupling and Change families. `rqx techniques` lists them grouped by the kind of move each one is. |
| Gamma, Helm, Johnson & Vlissides, *Design Patterns* (Addison-Wesley, 1994) | All twenty-three patterns, cited by section: `rqx explain decorator` prints `Design Patterns §4.4`. `rqx patterns` reports which a project uses, which are half applied, and where one would fit. |
| Robert C. Martin, *Clean Code* (Prentice Hall, 2008) | The Naming and Flow families, and the argument that a long parameter list, a flag argument and an output parameter are defects rather than style. |
| Gerard Meszaros, *xUnit Test Patterns* (Addison-Wesley, 2007) | The Tests family — assertionless tests, eager tests, self-asserting tests, unused fixtures and the rest of the suite smells. |
| Michael Feathers, *Working Effectively with Legacy Code* (Prentice Hall, 2004) | The baseline and ratchet: judging what a change adds rather than what it inherited, so a gate can go on before the debt comes off. |
| Adam Tornhill, *Your Code as a Crime Scene* (Pragmatic Bookshelf, 2015) | Reading git history as a signal — churn, hotspots, co-change and single-author files, and weighting a finding by what its file costs to change. |

A handful of techniques have no entry in those catalogues because they answer
something the books predate — `Extract Shared Module`, `Invert Dependency`,
`Batch the Work`, `Memoize`. Those are rqx's own names, and `rqx techniques` marks
no distinction: what matters at the point of a finding is that the fix has a name
and the name means one thing.

The rules that read the import graph, the module surface and the syntax tree are
built on [tree-sitter](https://tree-sitter.github.io/tree-sitter/), one grammar per
language, which is why a rule written once works in five.

## License

MIT. See [LICENSE](LICENSE).

Contributions are accepted under the same licence; there is no contributor licence
agreement to sign.
