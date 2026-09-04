# rqx

Finds dead code, duplication and architectural drift in TypeScript, Python, Go, Java
and C++ — and tells you *why* each finding matters and *what refactoring* fixes it.

One Rust binary. No Node, no npm, no runtime dependencies.

```
rqx                             # the interface: scan, then browse everything
rqx scan ./src                  # report everything
rqx scan --severity error       # only the findings that are never false
rqx check --base main           # review only what changed, fail CI on the gate
rqx patterns ./src              # which design patterns are used, broken, or would fit
rqx explain unused-export       # what a rule maps to and why it exists
rqx mcp                         # serve the analysis to an AI agent over MCP
```

## The report

```
 FAIL  src/orders/reconcile.ts  3 findings

   ● cognitive-complexity  88:3  warning
    88 │   private async reconcileBatch(batch: Batch): Promise<Result> {
       │                 ^^^^^^^^^^^^^^
     Cognitive complexity 25 (threshold 15), max nesting depth 6.
     Why:  every nested decision point multiplies the paths a reader has to trace
     Cost: changed 23 times in the last 90 days, 14 modules import it
     Fix:  Decompose Conditional / Extract Method — lift the conditions into
           named predicates, then extract the deepest block

  Files     7 with findings · 35 clean · 42 scanned
  Findings  3 errors · 31 warnings · 8 info
  Rules     6 of 42 fired
  Health    73/100
  Duration  0.84s
```

Every finding shows the code it is about: the whole span, context either side, and
the offending columns marked in the severity's colour with a caret ruler underneath
for terminals that drop backgrounds. `--context <n>` changes the window.

Each file carries a grade — `pristine`, `clean`, `fair`, `poor`, `critical` — from
the same curve as the health score applied to that file's own lines. It is what
`Start here` ranks by, and `--min-grade` gates on it, which catches one file rotting
where a project-wide average would not.

Clean files are counted, not listed — `--show-passed` lists them. Narrow the view
with `--severity`, `--rule`, `--category` and `--path`; none of them move the health
score or the gate, because a CI result that depended on how it was printed would be
worthless.

## What makes a finding

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

Sixty rules across eight families. Every rule names the metric that decides it and
the refactoring that answers it, and neither is optional.

That traceability is enforced by the compiler. The catalogue is one declarative
table (`crates/rqx-core/src/catalogue.rs`) in which every field is mandatory, so a
rule that omits its metric or its fix does not build. A second test asserts that
**every rule in the catalogue actually fires on a fixture** — a rule nothing can
trigger is a documentation entry pretending to be a check.

`rqx rules` prints all 60 grouped by family, and `rqx explain <rule>` prints one in
full. [`docs/rules/`](docs/rules/README.md) is the same catalogue as a reference: what
each rule measures, the threshold that tunes it, the languages it cannot apply to, and
a worked example lifted from the fixture that fires it.

| Family | Rules | What it measures |
|---|---|---|
| **Size** | 8 | units that outgrew what a reader can hold |
| **Abstraction** | 7 | types that model the problem wrong |
| **Change** | 3 | shapes that make an edit expensive |
| **Waste** | 18 | code and comments nothing reaches |
| **Coupling** | 5 | modules that know too much about each other |
| **Naming** | 7 | names that mislead or say nothing |
| **Safety** | 8 | silent failure and swallowed errors |
| **Tests** | 4 | suites that pass without checking |

Those pages are generated from the catalogue by `rqx docs`, and a test fails if they
drift from it — a hand-written table of sixty rules is a table that goes stale, which
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

## Five languages, one rule set

```toml
# RQX.toml — all five are on unless you say otherwise
[languages]
enabled = ["typescript", "python", "go", "java", "cpp"]
```

Every rule is written once, against measurements a language backend fills in: how
many statements a body holds, which names it binds, what it takes from other files.
A language contributes a table of node kinds, a reader for its module surface and an
import resolver; nothing else about it reaches the rules.

| | TypeScript | Python | Go | Java | C++ |
|---|---|---|---|---|---|
| **Extensions** | `.ts .tsx .mts .cts` | `.py .pyi` | `.go` | `.java` | `.cpp .cc .cxx .hpp .hh .h .ipp` |
| **Export is** | `export` | a module-level name without `_`, or `__all__` | a capital initial | `public` | a declaration in a header |
| **Imports resolve via** | extensions, `index`, `tsconfig` paths, `package.json` | packages and `__init__.py`, relative dots | `go.mod` module path, package directories | package to directory under a source root | `#include "…"` against the file and include roots |
| **Entry points** | `package.json` `main`/`exports`/`bin` | `__main__.py`, `main.py`, `conftest.py`, `test_*`, `[project.scripts]` | `package main`, `_test.go` | `static void main`, `@Test` | every translation unit, `TEST(…)` |
| **Tests** | Jest/Mocha/Vitest | pytest, unittest | `go test` | JUnit | GoogleTest |
| **Rules that apply** | 60 | 59 | 52 | 58 | 58 |

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

## Watch it while you work

```
rqx serve --open
```

A dashboard on `127.0.0.1`, updated as you save. It re-analyses the file you touched
and nothing else — a save costs about 6 ms on a small project, and the page shows the
change within one poll interval.

| Page | Shows |
|---|---|
| Overview | health, the trend for the session, the grade distribution, what to open first, and what the project would score under each level |
| Map | every file as a tile, sized by lines and coloured by grade — the shape of the codebase in one screen |
| Findings | every finding, filtered by severity, family, rule, path or whether `rqx fix` can answer it; click one for the code, the metric, the reason and the named refactoring |
| Files | one row per file, worst grade first, with blast radius |
| Plan | the ranked plan with a budget slider, re-ranked as you drag it |
| Rules | the catalogue, with how often each rule fired here |
| Activity | every save as it lands, with what it resolved and introduced |

`⌘K` jumps to a page or a file. The page is served from the binary — no Node, no
network, no build step — and binds loopback only, refusing any other address: it
renders your source and has no way to authenticate a browser.

## Which five do I fix on Monday

A list of three hundred findings ranked by severity is what every tool prints, and it
is why debt lists go unread. `rqx plan` ranks the work by what it buys against what it
costs, and says what the score would be after each item:

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

## Levels

How much a project asks of itself is one word:

```toml
level = "strict"
```

| Level | Rules | Gate | For |
|---|---|---|---|
| `relaxed` | 41 of 60 | error | legacy code that has to stay shippable while it is cleaned up |
| `standard` | 50 of 60 | error | the catalogue as it stands |
| `strict` | 54 of 60 | warning, min grade `fair` | held to what a reader can follow without effort |
| `pristine` | 60 of 60 | info, min grade `clean`, health 90 | every rule, including the ones that ship off |

A level is merged **under** everything you wrote, so it moves the defaults and never
overrides a value you stated — `rqx config` says `level:strict` in the `from` column
for a value the level moved, and names your file for one you set. `--level strict`
overrides the file, because CI usually wants a stricter standard on the default branch
than a working copy does, and an `[[override]]` may name its own level, so a project can
be strict in its domain and relaxed in its scripts. `rqx levels` prints each one as the
settings it would take to write by hand.

## Configuration

`rqx init` writes an `RQX.toml` with every setting at its default and a line of prose
per table; `rqx config` prints what a run will actually use and names the file each
value came from.

Layers merge, lowest first: the built-in defaults, then your own
`~/.config/rqx/RQX.toml`, then every `RQX.toml` from the scanned directory upward
(outermost first, stopping at the repository root), then `RQX_CONFIG`, then
`--config`, then the command's own flags. So `rqx scan src/` in a monorepo uses the
project's config rather than silently running on defaults.

A subtree with different standards can relax the two tables that are decided per file:

```toml
[[override]]
paths = ["src/generated/**"]

[override.thresholds]
long_function_statements = 200
```

Nothing else may be scoped. The file walker is built once and the gate reaches one
verdict, so an override naming `[scan]` or `[gate]` is an error rather than a setting
that quietly does nothing. [docs/configuration.md](docs/configuration.md) has the
whole schema, all 22 thresholds and the rules each one tunes.

## Families can be switched on and off

```toml
[rules]
categories = ["dispensables", "couplers"]   # only these families
disabled   = ["magic-number"]                # or a single rule
enabled    = ["naming"]                      # opt into an off-by-default family
```

Ten rules ship off by default — the ones whose signal is real but noisy, or which
encode a convention a given project may not share. They are listed with
`(off by default)` in `rqx rules`.

## Confidence is never overstated

A finding says how sure it is. `unused-parameter` claims **verified** only for a
trailing parameter of a function whose signature its author controls. A parameter
in the middle cannot be deleted without renumbering every call site, and a
callback's arity is fixed by whatever calls it — so those are reported as
**heuristic**, or not at all.

Findings that cannot be verified are omitted rather than asserted. Ignored files
are never parsed, which is why tests are *not* ignored by default: an invisible
import manufactures false "unused export" findings.

## Editors

`rqx lsp` is a language server, and everything an editor shows comes through it — so a
capability lands in every editor at once rather than once per client.

It scans the whole project after startup and after each save, publishing diagnostics for
files you never opened. That matters because eight of the sixty rules — unused exports,
unused files, import cycles, duplication and the two co-change rules — cannot be answered
from a single open file, and an editor that only saw the other 52 would be quietly wrong
about dead code.

Hover gives the full explanation. Quick fixes switch a rule or a family off in `RQX.toml`
and never touch your source, because every fix rqx names is a refactoring rather than an
edit it could apply for you. A code lens puts each file's grade, blast radius and churn
above its first line.

Nine commands are plain `workspace/executeCommand`, so any client can bind them —
`rqx.scan`, `rqx.check`, `rqx.explain`, `rqx.startHere` and the rest — behind one leader
chord with the same letters everywhere: `s` scan, `c` check, `e` explain, `h` start here.

| | Client |
|---|---|
| VS Code, Cursor, Windsurf, VSCodium | [`editors/vscode`](editors/vscode/README.md) — panel, status bar, ships the server inside it |
| Zed | [`editors/zed`](editors/zed/README.md) |
| Neovim, Helix, Sublime, Emacs, Vim, Kate | configuration only — [docs/editor.md](docs/editor.md) |

[`editors/README.md`](editors/README.md) has the full matrix of what each one gets.

## Agents

`rqx mcp` is an MCP server, so a coding agent can ask what a project's problems are
instead of reading every file to guess. It answers with the same four things every
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
| `analyze` | An unsaved buffer, without touching the disk |
| `patterns` · `explain` · `rules` · `config` | The catalogues, and the settings in force |

All nine are read-only: nothing rqx exposes to an agent writes a file. `plan` returns
the brief and the agent does the writing. `--http` serves
the same tools over Streamable HTTP for clients that cannot spawn a process, and refuses
a public address without `RQX_MCP_TOKEN` rather than handing your source to the network.
[docs/mcp.md](docs/mcp.md) has the payload shapes and a config snippet per client.

## The interface

`rqx` with no arguments opens a full-screen interface. Every screen is reachable
from the menu and every screen returns to it, so nothing is a dead end:

| Tab | Shows |
|---|---|
| Overview | health, severity and grade distributions, what to open first |
| Findings | every finding with the offending code marked, filter, group, severity floor |
| Files | one row per file, worst grade first, with churn and blast radius |
| Patterns | which design patterns are in use, which are half applied, and where one would fit |
| Rules | the catalogue, searchable, with each rule's family, metric, fix and why |
| Config | the effective settings and where each one came from |

`tab` cycles all six, `1`–`6` jump, `r` runs a scan, `/` filters, `?` lists the keys
of the screen you are on, `esc` steps back, `q` leaves and prints what the session
found. A tab opened before a scan says what it will hold rather than refusing to
open, and nothing but `q` returns you to the shell. It animates only while something is moving — a
still screen blocks on input and costs nothing to leave open. `--motion off` turns
the movement off without losing anything the screen says.

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
JUnit XML and GitLab Code Quality that covers GitHub, GitLab, Jenkins, CircleCI,
Azure Pipelines, Bitbucket and Buildkite natively. Copy-paste snippets for each
are in [docs/ci.md](docs/ci.md); `action.yml` wires up GitHub Actions in five
lines.

The gate lives in `RQX.toml` and every part of it can be overridden per run with
`--fail-on`, `--allow-findings` and `--min-health`, so a strict gate on the
default branch and a lenient one on feature branches do not need two config files.

```toml
# RQX.toml
[gate]
fail_on = "warning"
max_findings = 0
```

## Working on it

```
./rqx scan ../some-project      # runs from the source tree, no install
RQX_RELEASE=1 ./rqx scan .      # optimised, for timing runs
cargo rqx rules                 # same thing through cargo
cargo test --workspace
```

`./rqx` builds and then execs the binary rather than using `cargo run`, so cargo's
progress output never lands in the middle of the interface, and paths still resolve
against your own working directory. Build output goes to stderr, so
`./rqx scan . --reporter json > out.json` stays valid JSON.

To install it properly: `cargo install --path crates/rqx-cli`.

## Performance

A 12,330-file, 265k-line mixed-language repository: **~1.7s**, with memory a function
of thread count rather than file count. Output is byte-identical regardless of how
many threads it ran on, so a CI gate is never flaky.

Narrowing to one language narrows the work: the same tree with only TypeScript
enabled is 2,070 files in **~0.37s**, because a file no enabled language claims is
never opened.
