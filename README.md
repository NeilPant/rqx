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
![Rules](https://img.shields.io/badge/rules-201-0a84ff)
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
rqx check --staged              # review what the next commit would record
rqx comment --fail-on error     # post that review onto the merge request
rqx mcp                         # serve the analysis to an agent over MCP
```

## Contents

1. [Install](#install)
2. [Network access](#network-access)
3. [The commands](#the-commands)
4. [What rqx does not do](#what-rqx-does-not-do)
5. [The report](#the-report)
6. [Anatomy of a finding](#anatomy-of-a-finding)
7. [The catalogue](#the-catalogue)
8. [Design patterns](#design-patterns)
9. [Architectural constraints](#architectural-constraints)
10. [Language support](#language-support)
11. [Automatic fixes](#automatic-fixes)
12. [Verifying a refactoring](#verifying-a-refactoring)
13. [Prioritising the work](#prioritising-the-work)
14. [Adopting it on an existing codebase](#adopting-it-on-an-existing-codebase)
15. [Surveying an unfamiliar codebase](#surveying-an-unfamiliar-codebase)
16. [Editing configuration](#editing-configuration)
17. [Levels](#levels)
18. [Configuration](#configuration)
19. [Enabling and disabling rules](#enabling-and-disabling-rules)
20. [Cross-language duplication](#cross-language-duplication)
21. [Performance rules](#performance-rules)
22. [Coverage as evidence](#coverage-as-evidence)
23. [Confidence levels](#confidence-levels)
24. [Editors](#editors)
25. [Agents](#agents)
26. [The interface](#the-interface)
27. [CI](#ci)
28. [Contributing](#contributing)
29. [Performance](#performance)
30. [References](#references)
31. [License](#license)

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
`--dry-run` prints every step without changing anything. To inspect the script
before running it, read [install.sh](install.sh).

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

A release is one tag. It builds six binaries, one per platform, and publishes them
beside the crates from the same commit, so one version number identifies the same
code in every channel. Every asset is checksummed in `SHA256SUMS` on the release.

Editors talk to `rqx lsp` over the Language Server Protocol; `rqx setup <editor>`
prints the configuration each one needs. See [Editors](#editors).

## Network access

A scan reads your code on your machine and sends none of it anywhere. rqx reaches the
network in three cases.

**The update check.** When you run `rqx scan` or `rqx check` in a terminal, rqx asks
GitHub whether a newer release has been tagged, at most once a day. It runs `curl`
against `https://api.github.com/repos/jadhamwi21/rqx/tags` with the headers
`Accept: application/vnd.github+json` and `User-Agent: rqx/<version>`. For the newest
tag it also fetches that tag's `CHANGELOG.md` from `raw.githubusercontent.com`, so the
notice can say what changed. Nothing from your project is in either request. The answer
is cached in `update.json` under `$XDG_CACHE_HOME/rqx`, or `~/.cache/rqx` when that is
unset, and a newer version is shown as one dim line after the report.

The check starts when the scan does, and the report waits at most a quarter of a second
for it at the end. It runs only when both stdout and stderr are terminals, so it is
skipped when output is piped or redirected. It is also skipped when `CI`,
`JENKINS_URL`, `TEAMCITY_VERSION`, `TF_BUILD` or one of the other pipeline variables
rqx recognises is set, and for an install that a package manager owns (`RQX_MANAGED`).
Exit codes, the report and reporter files are the same whether or not it runs.

To turn it off, set `RQX_NO_UPDATE_CHECK` in the environment, or add this to
`rqx.yaml`:

```yaml
update:
  check: false
```

**`rqx update`** downloads a release from GitHub, but only when you run it.

**`rqx comment`** posts to the GitLab or GitHub API of the merge request it is run for,
but only when you run it.

## The commands

Thirty-three of them, under five headings. `rqx --help` prints this list; each
command's own `--help` prints its flags, and [docs/cli.md](docs/cli.md) is the whole
reference.

| | |
|---|---|
| **The loop** | `scan` · `check` · `fix` · `verify` · `comment` |
| **Look closer** | `complexity` · `perf` · `health` · `owners` · `trend` · `slop` · `hotspots` · `graph` · `plan` · `patterns` · `recommend` |
| **Reference** | `rules` · `explain` · `techniques` · `levels` |
| **Project** | `init` · `config` · `baseline` · `suppressions` · `waive` · `cache` |
| **Integrations** | `lsp` · `mcp` · `setup` · `completions` · `badge` · `query` · `update` |

The four most frequently used have one-letter aliases: `rqx s`, `rqx c`, `rqx f`,
`rqx v`. Adding a thirty-fourth command fails the build until it is placed under a heading.

Inside the interface the same list is `/help`.

## What rqx does not do

- **It does not type-check.** rqx parses source, resolves scopes and imports, and reads
  git history. It does not resolve types, so a finding that needs a type checker to be
  certain is reported at lower confidence, or not reported, and says which.
- **Rule coverage differs by language.** Of 201 rules, 195 apply to TypeScript, 187
  to JavaScript, 185 to Python, 184 to Java, 183 to C++ and 151 to Go. A rule that cannot
  exist in a language says so in `rqx explain`, with a reason. `rqx rules --language
  <name>` lists the ones that apply.
- **JavaScript leaves out the type-syntax rules.** JavaScript is read as TypeScript
  without its types, so the eight rules that read type syntax do not apply to it:
  `abstract-nothing`, `boolean-not-a-question`, `catch-all-handler`, `flag-argument`,
  `imprecise-type`, `inappropriate-static`, `primitive-obsession` and
  `speculative-generality`.
- **It does not do security or taint analysis.** rqx does not follow data from a source
  to a sink, so it will not find injection or unsafe deserialisation. Use a tool built
  for that, such as [Semgrep](https://semgrep.dev) or [CodeQL](https://codeql.github.com).
- **It does not judge meaning.** Whether a comment still matches the code beneath it,
  whether an algorithm is the right one, or whether behaviour a reader expected is
  missing cannot be settled by a metric, so rqx has no rules for them.

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
  Rules     9 of 201 fired
  Health    66/100
  Duration  0.02s
```

Every finding shows the code it is about, with the offending columns marked in the
severity's colour and a caret ruler underneath for terminals that drop backgrounds.

A rule's `Why:` is the same on every finding the rule produces, and a file's churn is
the same on every finding in it, so each is printed once: the reason on the rule's
first finding and the cost in the file's header. `--detail full` repeats both on every
finding and shows two lines of context either side; `--detail terse` is one line per
finding. `--context <n>` sets the window at any level.

Import cycles, and the contracts a file breaks where an `architecture` is declared,
get sections of their own above the summary. Both are also ordinary findings,
anchored on one member of the ring.

Each file carries a grade (`pristine`, `clean`, `fair`, `poor` or `critical`) from the
same curve as the health score, applied to that file's own lines. `Start here` ranks
by it, and `--min-grade` gates on it, which catches one file degrading where a
project-wide average would not.

Clean files are counted, not listed; `--show-passed` lists them. Narrow the view
with `--severity`, `--rule`, `--category` and `--path`, or by subtraction with
`--not-rule`, `--not-category` and `--not-path`. `--confidence` keeps only what rqx is
certain of or only what it is guessing at, `--fixable` keeps what `rqx fix` could act
on, and `--search` matches the rule, the file or the evidence. None of them changes the
health score or the gate.

## Anatomy of a finding

Every finding carries four things:

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

The finding states the measured fact, the reason it matters, what it costs given how
often the file changes and how much depends on it, and a named refactoring. The fix
is a named refactoring, never a generic instruction such as "simplify this".

Each rule is traceable to an established catalogue. `RuleId::spec()` is an exhaustive
match, so a rule that does not declare its metric and fix does not build.

## The catalogue

<!-- rqx:catalogue -->
201 rules across 11 families. Every rule declares the metric that decides it and the refactoring that answers it.

| Family | Rules | What it measures |
|---|---|---|
| **Size** | 16 | functions, types and modules whose size or complexity exceeds a threshold |
| **Abstraction** | 17 | types whose structure does not match the problem |
| **Change** | 7 | code shapes that raise the cost of a change |
| **Waste** | 34 | code and comments that nothing reaches |
| **Coupling** | 18 | dependencies between modules that are too broad or point the wrong way |
| **Flow** | 17 | control flow that is redundant or misleading |
| **Naming** | 12 | names that mislead or carry no information |
| **Safety** | 28 | silent failure and swallowed errors |
| **Tests** | 18 | tests that pass without checking behaviour |
| **Noise** | 9 | text that adds reading cost without adding information |
| **Performance** | 25 | work done more often, or more expensively, than needed |
<!-- /rqx:catalogue -->

The table above and the pages in `docs/rules/` are generated from the catalogue by
`rqx docs`, and a test fails if they differ from it.

`rqx techniques` prints every refactoring the rules prescribe, grouped by the kind of
move it is, with the rules that ask for it. A technique that no rule prescribes
fails the test suite.

### What is deliberately absent

Some properties of code cannot be decided from source: whether a comment still
matches the code beneath it, whether an algorithm is the right one, and whether
expected behaviour is missing. No metric settles these, so no rule reports them.

The other limits, including what needs types, are listed under
[What rqx does not do](#what-rqx-does-not-do).

## Design patterns

A third catalogue covers all twenty-three patterns from *Design Patterns*. Each
pattern has the static shape that decides it and a stated reason where rqx declines
to recognise it. `rqx patterns` reads a project, `rqx patterns --coverage` prints the
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

rqx reports three kinds of result, and they do not rank. A pattern in use is
inventory. A pattern half applied is a defect: a decorator that cannot substitute for
what it wraps, or a subscribe with no unsubscribe. A pattern that would fit is a
suggestion about a shape already in the code.

Pattern results do not change the health score. Advice is not a `Finding` and has no
route to `FindingCounts`; a test scans the fixtures with every advisory on and off
and compares the summary byte for byte. The only route to an exit code is
`gate.fail_on_incomplete`, which considers only the half-applied patterns.

Confidence is set per advisory rather than per pattern. Recognising a Decorator from
what it holds is `verified`. Reading a Bridge out of a cross-product of subclass
names is a `measured threshold`. Pairing a save with a restore by name is
`low confidence` and ships off. [`docs/patterns/`](docs/patterns/README.md) is the
reference.

## Architectural constraints

ArchUnit covers Java, dependency-cruiser covers JavaScript and import-linter covers
Python. rqx checks layering across all six languages from one configuration, because
it reads one module graph.

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
before it, so the domain cannot import an adapter. The finding points at the import
line, in any of the six languages, because it reads the same module graph as cycle
detection.

The check does nothing until an `architecture` is declared, because rqx does not
infer one. A layer named in `paths` but absent from `layers` is a configuration
error.

## Language support

```yaml
# rqx.yaml: all six are enabled by default
languages:
  enabled: [typescript, javascript, python, go, java, cpp]
```

Every rule is written once, against measurements a language backend fills in: how
many statements a body holds, which names it binds, what it takes from other files.
A language contributes a table of node kinds, a reader for its module surface and an
import resolver. Nothing else about a language reaches the rules.

| | TypeScript | JavaScript | Python | Go | Java | C++ |
|---|---|---|---|---|---|---|
| **Extensions** | `.ts .tsx .mts .cts` | `.js .jsx .mjs .cjs` | `.py .pyi` | `.go` | `.java` | `.cpp .cc .cxx .hpp .hh .h .ipp` |
| **Export is** | `export` | `export` | a module-level name without `_`, or `__all__` | a capital initial | `public` | a declaration in a header |
| **Imports resolve via** | extensions, `index`, `tsconfig` paths, `package.json` | the same, and one graph with TypeScript: `./x.js` finds `x.ts` | packages and `__init__.py`, relative dots | `go.mod` module path, package directories | package to directory under a source root | `#include "…"` against the file and include roots |
| **Entry points** | `package.json` `main`/`exports`/`bin` | `package.json` `main`/`exports`/`bin` | `__main__.py`, `main.py`, `conftest.py`, `test_*`, `[project.scripts]` | `package main`, `_test.go` | `static void main`, `@Test` | every translation unit, `TEST(…)` |
| **Tests** | Jest/Mocha/Vitest | Jest/Mocha/Vitest | pytest, unittest | `go test` | JUnit | GoogleTest |
| **Rules that apply** | 195 | 187 | 185 | 151 | 184 | 183 |

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

`rqx rules --language go` lists what does apply. Two tests cover this. Every
applicable rule must fire on that language's fixtures, and every exempted rule must
stay silent on them. A rule that stops working in one language fails the build.

## Automatic fixes

Most findings name a refactoring whose shape is a design decision. Some fixes involve
no decision: deleting an unused local needs only its exact bytes, which rqx has
already located, and a check that nothing references them.

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

`rqx fix` writes nothing without `--write`, and with it writes only verified edits.
Each file's edits are applied in memory, re-parsed for the error nodes tree-sitter
inserts when it recovers, and re-analysed. The targeted findings must be gone and no
new finding may appear. A file that fails is named with the reason. Writes are
atomic. The pass repeats, because one fix can enable the next: deleting the branch
that held a binding's only reference makes that binding unused.

Thirteen rules qualify. `fix.disabled: [commented-out-code]` in `rqx.yaml` opts one
out, and `rqx explain <rule>` says whether a rule is fixable. For the other 172,
`rqx fix --plan` emits each finding with its span, the source text, the metric, the
reasoning and the named refactoring, as a brief for a person or an agent. `rqx check
--base main --fix` does the same for a branch and judges
the gate afterwards, so a branch whose only failures were mechanical passes.

Editors offer the same edits, verified the same way and tagged `source.fixAll`, so
they can be applied on save.

## Verifying a refactoring

rqx names a refactoring for every finding, and `rqx verify` grades a diff against the
refactoring it claimed to perform. Naming a refactoring does not show that it was
carried out, which matters most where an agent wrote the diff.

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

`relocated` marks a body moved into a new helper that takes its complexity with it,
and it is reported separately from `resolved`. Findings are matched by what they are
about (rule, file and subject) and never by line, so moving code around does not
count as fixing it.

`--expect cognitive-complexity@orders/reconcile.ts` fails the run unless that
particular finding was resolved. `verify` is also an MCP tool, so an agent can propose
a change, be graded, and try again.

## Prioritising the work

`rqx plan` ranks findings by what each item buys against what it costs, and projects
the health score after each one:

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
change: its churn and how many modules import it. Effort comes from the refactoring the
rule prescribes: deleting is one point, moving behaviour up or down a type tree is
five. The selection is exact rather than greedy: the cheapest two items can be worth
more together than the most expensive one.

## Adopting it on an existing codebase

A gate enabled on a codebase that already has findings fails the first pull request.
Four features make adoption incremental.

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

All eight warnings are still reported and still appear in the editor. Seven are debt
the baseline recorded and one arrived with this change, and only that one fails the
build. The health score counts all eight, because it describes the code and not which
findings have been accepted. [More](docs/baseline.md).

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

The query is a tree-sitter query, and every grammar is built into the binary. A custom
rule counts towards the score, shows in the editor, gets its own SARIF descriptor, and
can be disabled or waived by name. `rqx query typescript '<query>' --captures` tries
one before you commit it, and a query that does not compile is a configuration error
naming the rule and the row it failed at. [More](docs/custom-rules.md).

**One standard across many repositories.**

```yaml
policy: ../platform/rqx-policy.yaml
```

The policy is merged under the project, so a service can still tighten settings.
`locked.keys` lists the settings a service may not change, whether in its own config,
in an override or with `--level`. [More](docs/policy.md).

## Surveying an unfamiliar codebase

```sh
rqx recommend      # which thresholds fit, which refactoring pays, what to read first
rqx complexity     # the measurements, as a distribution rather than a tail
rqx health --by rule
rqx graph --cycles --format mermaid
rqx slop           # which files read as written rather than read
rqx hotspots       # which files are changed often and have findings
```

`rqx complexity` shows the distribution of function complexity in a tree:

```
  Complexity  39873 functions measured

                        cplx  stmts  nest args  where
    (anonymous)           61    102     5    1  …/chart/layers/CandleLayer.tsx:82
    TradingChartCanvas    60    221     4    1  …/chart/TradingChart.tsx:230
    submit                53    102     5    3  …/challenges.service.ts:658
    projectMarkdown       52     84     4    1  …/markdown-projection.ts:30

    p50 0 · p90 2 · p99 11 · 180 of 39873 over the threshold of 15
```

The final line shows whether the threshold describes the exceptions or the norm.
`rqx recommend` proposes moving it in the first case and makes no proposal in the
second.

`rqx graph` prints the import graph as text, Graphviz, Mermaid or JSON, with the edges
that close a cycle marked in each. `circular-dependency` reports the ring as a
walk that can be followed rather than a count, anchored on the import that opens it.

`rqx slop` scores how much of a file is text that costs a reader and buys nothing: a
comment above every line restating it, a divider in a fifty-line file, `example.com`
in a constant, a leftover *Here's the implementation you asked for!*. Each is weak
evidence alone. Together and at density they suggest code that was generated and not
reviewed. It is labelled a heuristic wherever it appears and gates nothing unless
configured to.

`rqx hotspots` ranks files by the commits they received in the churn window multiplied
by the weight of their findings and of their cognitive complexity, with a column that
states each factor. See [docs/hotspots.md](docs/hotspots.md).

`rqx badge --output badge.svg` writes the health score as a small SVG for a README. A
GitHub Actions workflow that keeps it current is in [docs/ci.md](docs/ci.md#a-health-badge).

## Editing configuration

```sh
rqx config                                   # the settings, and which file set each
rqx config set thresholds.max_params 3       # write it, comments and layout intact
rqx config set gate.fail_on warning --pick   # choose which file in the chain
rqx config unset thresholds.max_params
rqx config files                             # the chain, nearest last
```

A monorepo can read a user config, a policy and one `rqx.yaml` per directory. `set`
writes to the file that already holds the key, and `--pick` prompts for a file when
you want a different one. Edits go through a format-preserving writer, because the
template is mostly prose explaining what each key is for and re-serialising would
discard it. A value the config would refuse is refused before the file is written,
and a key the policy locked is refused before the file is touched.

## Levels

A project's overall strictness is one setting:

```yaml
level: strict
```

| Level | Rules | Gate | For |
|---|---|---|---|
| `relaxed` | 168 of 201 | error | legacy code that has to stay shippable while it is cleaned up |
| `standard` | 177 of 201 | error | the catalogue as it stands |
| `strict` | 181 of 201 | warning, min grade `fair` | held to what a reader can follow without effort |
| `pristine` | 201 of 201 | info, min grade `clean`, health 90 | every rule, including the ones that ship off |

A level is merged under everything the project states, so it moves the defaults and
does not override a stated value. `rqx config` shows `level:strict` in the `from`
column for a value the level moved, and names the project's file for one it set.
`--level strict` overrides the file, because CI usually needs a stricter standard on the default branch
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
code 2, because skipping it could change what a pipeline gates on. `rqx config
migrate` converts it, its policy files and the user config to YAML, keeping comments
and the originals as `.bak`.

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

No other table may be scoped. The file walker is built once and the gate reaches one
verdict, so an override naming `scan` or `gate` is a configuration error.
[docs/configuration.md](docs/configuration.md) has the whole schema, all 44 thresholds
and the rules each one tunes.

## Enabling and disabling rules

```yaml
rules:
  categories: [waste, coupling]           # only these families
  disabled: [magic-number]                # or a single rule
  enabled: [short-name-wide-scope]        # opt into an off-by-default rule
```

Twenty-four rules ship off by default: the ones whose signal is real but noisy, or
which encode a convention a given project may not share. They are listed with
`(off by default)` in `rqx rules`.

## Cross-language duplication

Clone detection is per language: the hash reads node ids that each grammar assigns,
and grammars do not share ids. A second hash reads a vocabulary no grammar owns
(branch, loop, call, return, assign and the canonical operator), so a validation
rule ported from TypeScript to Python still matches.

```
● cross-language-duplicate  api/validate.py:4
  9 statements, 51 nodes, 2 occurrences
  the same shape is in TypeScript and Python; give the rule one home and call it
  from both, or write down why they are allowed to differ
```

The rule is off by default and reports at low confidence, which keeps it out of the
health score. Two blocks that branch and return in the same order are not
necessarily the same rule, so a group has to span languages, clear eight statements
and forty nodes, and share a constant before it is reported. Ported code usually
keeps its constants when its syntax changes.

## Performance rules

Several costs that a profiler would find are visible in source: a query inside a
loop, a linear search inside a loop, an await per item, a pattern recompiled on every
pass. Twenty-five rules read those shapes, three of them off by default, and `rqx perf`
ranks the functions carrying them by what they cost against how much of the project
leans on the file they are in.

The orders in the table are estimates. Big-O cannot be decided from source, so rqx
reads loops, the types of the receivers and the calls it can follow, and each row
carries a confidence and, with `--explain`, the evidence behind it. On the precision
corpus the time and space orders were exact for 0.91 of 437 functions written for the
corpus and for 0.48 of 31 functions copied from five open source projects. The sample
of real code is small. [docs/perf-precision.md](docs/perf-precision.md) has the figures
per language and per rule.

```
$ rqx perf --top 2

  Bottlenecks  4 functions worth a look

                   order   time space   io alloc      conf loops trips reach  where
    reconcileBatch  O(n) O(n·m)  O(n) O(n)     2  verified     1     2  ×1.0  reconcile.ts:3
                   `db.order.findMany` runs on every pass of the loop.  · Fix: Batch the Work
    fibonacci        exp    exp  O(n) O(1)     –  verified     0     0  ×1.0  reconcile.ts:12
                   Calls itself 2 times per invocation with no memo in sight.  · Fix: Memoize

    p50 1 · p90 1 · p99 1 loop depth · 0 over 2 · 2 with a round trip inside a loop
```

`order` is the syntactic loop order. `time`, `space` and `io` are composed across calls
and files, and `conf` says how sure the analysis is. `rqx perf --explain reconcileBatch`
prints how a row's cost was derived. None of this is proof of cost: three nested loops
over three items are cheap. A CPU profile can settle it. `--profile cpu.pprof` reads a V8
`.cpuprofile`, a Go pprof, speedscope JSON, `jfr print` text or collapsed stacks by
content. A performance finding in a function the run never put on the stack keeps its
finding and stops counting, in the same way a covered symbol stops counting as dead
code.

Some rules rely on names, which are weak evidence, so those rules are `heuristic` and
never `error`. A receiver that is already keyed (a `Set`, a `Map`, anything named
`index` or `cache`) is a lookup rather than a round trip and is not reported.
[docs/perf.md](docs/perf.md) has the columns, the formats and the vocabulary a
project can extend.

## Coverage as evidence

Dead code is decided from the import graph, and a graph cannot see a dynamic import,
a container wiring things up by name, or reflection. A coverage file settles it:

```bash
rqx scan . --coverage coverage/lcov.info
```

A file or export that the graph called dead and a test run reached keeps its finding,
because the missing import may still be worth adding, but drops to `low` confidence.
It is shown and no longer counts against the health score. One that nothing reached
stays `verified` and records the coverage as evidence.

lcov, Go cover profiles and coverage.py JSON are recognised by content, several files
merge, and a coverage file that cannot be read is skipped with a warning rather than
failing the scan.

## Confidence levels

A finding says how sure it is. `unused-parameter` claims **verified** only for a
trailing parameter of a function whose signature its author controls. A parameter
in the middle cannot be deleted without renumbering every call site, and a callback's
arity is fixed by whatever calls it, so those are reported as **heuristic**, or not
at all.

Findings that cannot be verified are omitted rather than asserted. Ignored files are
never parsed, which is why tests are not ignored by default: an unparsed import
produces false "unused export" findings.

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

There is no extension to install. Every client above is stock LSP configuration
pointed at the `rqx lsp` binary, so a capability added to the server is available to
all of them. Any editor with an LSP client works the same way. For a sidebar view of
findings, files and rules, run `rqx` in a terminal split.

Quick fixes are the same edits `rqx fix` applies, verified the same way and tagged
`source.fixAll` so an editor can apply them on save. For rules whose fix is a
decision, the quick fix offers to explain the rule or turn it off.

The server scans the workspace on its own and republishes every file, so the rules
that need the whole import graph produce findings even when the editor has one file
open.

[docs/editor.md](docs/editor.md) has the commands, the custom notifications and a
paste-in block per editor.

## Agents

`rqx mcp` runs an MCP server, so a coding agent can query a project's findings
instead of inferring them by reading files. Each answer carries the same four things
as a finding: the measured fact, why it matters, what it costs and the named
refactoring.

```sh
claude mcp add rqx -- rqx mcp
```

Answers are summaries with drill-down. `scan` returns health, the grade distribution
and what to open first; the findings are paged out of `findings` and can be narrowed
by rule, severity or path. A full report of a 12,000-file repository does not fit in
a context window.

| Tool | Answers |
|---|---|
| `scan` · `findings` | The project's health, then its findings a page at a time |
| `check` | The diff against a base revision, and whether the gate passes |
| `plan` | The findings `rqx fix` will not touch, each with its source, metric and named refactoring |
| `fix_preview` | The mechanical fixes `rqx fix` would make to one file, as a unified diff with the findings they resolve |
| `verify` | What a diff did to every finding: resolved, improved, relocated, introduced |
| `analyze` | An unsaved buffer, without touching the disk |
| `perf` | Where the work grows faster than the data, ranked by cost against reach |
| `complexity` · `health` | The distributions the score comes from, and what each part costs |
| `recommend` · `graph` | What to do about the project, and what imports what |
| `patterns` · `explain` · `rules` · `config` | The catalogues, and the settings in force |

All sixteen are read-only: nothing rqx exposes to an agent writes a file. `plan` returns
the brief and `fix_preview` the diff, and the agent does the writing. `--http` serves
the same tools over Streamable HTTP for clients that cannot spawn a process, and
refuses a public address without `RQX_MCP_TOKEN`.
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

It scans as it opens. While it works, the owl animates beside the phase and the files
done. When the scan finishes, a header and the worst findings go into scrollback,
with the owl in the header in the colour of the verdict. Under the
prompt, one line says `? for shortcuts` and, on the right, the health, the grade, the
findings and `watching` when it is on.

Slash commands use the command line's definitions. `/plan --budget 20` is parsed by
the same definition as `rqx plan --budget 20`, runs on the scan the session already
holds, and prints through the same functions, so the two produce the same output. Text with no
slash searches the findings.

```
> /plan --budget 20
> /findings --severity error --group-by rule
> dead export
```

A line that starts with `/` lists the matching commands under the prompt: command
names, then after a space that command's flags, then the values a flag takes, which
are rule slugs, families, files from the scan or the choices the flag lists. `tab`
accepts, `esc` closes. `?` on an empty line prints the keys. The line edits with the
arrow keys, `home`, `end`, `ctrl-u` and `ctrl-w`; `↑` and `↓` walk a history that is
kept per user outside the repository. `/rescan` scans again, `/watch on` re-analyses
files as they are saved, `/open 2` opens a finding in `$EDITOR`, `/waive` writes a
waiver, `/clear` clears the screen, `/help` lists the commands, and `/exit`,
`ctrl-d` or `ctrl-c` twice leaves.

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

A closed pipe is not a failure: `rqx scan . \| head` exits `0`.

One analysis pass feeds every consumer. `--reporter <name>` writes to stdout,
`--reporter <name>=<path>` writes to a file, and both repeat:

```sh
rqx scan . \
  --reporter terminal \
  --reporter junit=rqx.xml \
  --reporter sarif=rqx.sarif \
  --fail-on error
```

The reporters are `terminal`, `json`, `sarif`, `junit`, `gitlab`, `github`,
`markdown` and `html` (deprecated, removed in 0.6.0). JUnit XML and GitLab Code Quality together cover GitHub,
GitLab, Jenkins, CircleCI, Azure Pipelines, Bitbucket and Buildkite natively.
Snippets for each are in [docs/ci.md](docs/ci.md). `action.yml` is the GitHub
Action: `uses: jadhamwi21/rqx@v0.2.1`. A moving major tag such as `@v1` is planned
and is not published yet.

`rqx comment` posts the review onto the merge request it is reviewing, on GitLab and
GitHub. It reads the project, the merge request and the token from the CI
environment, writes one comment (a verdict line, the counts, and the worst findings
folded behind a `<details>`), and edits that same comment on every later run instead
of adding another. It also grades the commit, so the merge request shows `rqx: B · 12
findings` beside the other checks without anyone
opening a job log.

```sh
rqx comment --fail-on error
```

`rqx comment` takes no `--base` by default because the CI environment supplies it.
GitLab provides `CI_MERGE_REQUEST_DIFF_BASE_SHA` and GitHub provides the pull
request's base. Both are commits that exist in a CI clone, which `origin/HEAD` may
not be. Pass `--base` to override it.

`--dry-run` prints what it would post and posts nothing. `--marker` keeps two jobs'
comments apart on one merge request. `--no-status` comments without grading. The exit
code is the gate's, so the job fails the same way
`check` does. GitLab needs a token with `api` scope (`CI_JOB_TOKEN` cannot write
merge-request notes), and GitHub needs `pull-requests: write`.

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

`./rqx` builds the binary and then execs it instead of using `cargo run`, so cargo's
progress output does not appear inside the interface, and paths resolve against the
current working directory. Build output goes to stderr, so
`./rqx scan . --reporter json > out.json` stays valid JSON. To install it:
`cargo install --path crates/rqx-cli`.

Rules are a declarative table. [CONTRIBUTING.md](CONTRIBUTING.md) walks through adding
one. The compiler reports most of the steps, and a rule that no fixture can trigger
does not compile. Precision numbers from a real repository are part of the review.

[Code of conduct](CODE_OF_CONDUCT.md) ·
[Security](SECURITY.md) ·
[Releasing](RELEASING.md) ·
[Changelog](CHANGELOG.md) ·
[Discussions](https://github.com/jadhamwi21/rqx/discussions)

## Performance

Measured on an M3 Pro (five performance cores, six efficiency cores) with the
cache off for the cold numbers:

| | Files | Code lines | Cold | Warm |
|---|---:|---:|---:|---:|
| A TypeScript codebase | 3,456 | 461,090 | **3.2s** | **0.41s** |
| Five languages in one tree | 4,440 | 178,344 | **1.6s** | **0.67s** |

Warm is the same answer from `.rqx/cache.bin` for every file whose contents have not
moved. The cross-file stage (the import graph, the clone index and the churn rules)
runs again either way, because a deleted import changes what the graph says about
files nobody touched.

The walk scales with the cores it is given. On the TypeScript codebase: one thread
24.3s, two 12.3s, four 6.4s, eight 3.8s, all eleven 3.3s, which is **7.3× on 5+6**.

Output is byte-identical for any thread count, and a test asserts it, so the thread
count does not affect a CI gate. Memory use depends on the thread count rather than
the file count.

Roughly 90% of a cold scan is the per-file parse and walk. The stages after it
(duplicates, the import graph, git history and co-change) together take about 200ms of
the 3.2s above, so the cache skips the larger part and keeps the smaller one.

## References

Every rule names a metric and a refactoring that come from an established source. The
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
something the books predate: `Extract Shared Module`, `Invert Dependency`,
`Batch the Work` and `Memoize`. Those are rqx's own names, and `rqx techniques` does
not distinguish them.

The rules that read the import graph, the module surface and the syntax tree are
built on [tree-sitter](https://tree-sitter.github.io/tree-sitter/), one grammar per
language, which is why a rule written once works in all six languages.

## License

MIT. See [LICENSE](LICENSE).

Contributions are accepted under the same licence; there is no contributor licence
agreement to sign.
