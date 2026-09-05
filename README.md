<div align="center">

<img src="assets/mascot-128.png" width="96" height="96" alt="" />

# rqx

**Finds dead code, duplication and architectural drift in TypeScript, Python, Go, Java
and C++ — says why each finding matters, names the refactoring that fixes it, applies
the ones with a single right answer, and grades the diff afterwards.**

One Rust binary. No Node, no npm, no runtime dependencies.

[![CI](https://github.com/jadhamwi21/rqx/actions/workflows/rqx.yml/badge.svg)](https://github.com/jadhamwi21/rqx/actions/workflows/rqx.yml)
[![Release](https://img.shields.io/github/v/release/jadhamwi21/rqx?color=0a84ff)](https://github.com/jadhamwi21/rqx/releases)
[![VS Code](https://img.shields.io/visual-studio-marketplace/v/jadhamwi.rqx?label=VS%20Code&color=0a84ff)](https://marketplace.visualstudio.com/items?itemName=jadhamwi.rqx)
[![Open VSX](https://img.shields.io/open-vsx/v/jadhamwi/rqx?label=Open%20VSX&color=0a84ff)](https://open-vsx.org/extension/jadhamwi/rqx)
[![License](https://img.shields.io/badge/license-MIT-black)](LICENSE)
![No runtime dependencies](https://img.shields.io/badge/runtime%20deps-none-1f9e55)

`static-analysis` · `code-quality` · `refactoring` · `dead-code` · `lsp` · `mcp` · `rust`

</div>

---

```
rqx                             # the interface: scan, then browse everything
rqx scan ./src                  # report everything
rqx fix --write                 # apply the fixes with one right answer, proven first
rqx plan --budget 20            # rank the work, and say what the score will be after
rqx verify --base main          # did the refactoring you named actually happen
rqx serve --open                # a live dashboard, updated as you save
rqx check --base main           # review only what changed, fail CI on the gate
rqx mcp                         # serve the analysis to an agent over MCP
```

## Install

A release ships one tarball per target, with `SHA256SUMS` beside them:

```bash
# macOS (Apple silicon); swap the target for your platform
VERSION=0.1.0 TARGET=aarch64-apple-darwin
curl -fsSLO "https://github.com/jadhamwi21/rqx/releases/download/v$VERSION/rqx-$VERSION-$TARGET.tar.gz"
tar -xzf "rqx-$VERSION-$TARGET.tar.gz"
install "rqx-$VERSION-$TARGET/rqx" /usr/local/bin/

# or build it
cargo install --git https://github.com/jadhamwi21/rqx rqx
```

Targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.

Editors: **VS Code** (also Cursor, Windsurf, VSCodium) and **Zed** ship extensions that
fetch the matching binary themselves. Everything else speaks to `rqx lsp` — see
[Editors](#editors).

<details>
<summary><b>Contents</b></summary>

- [Install](#install)
- [The report](#the-report)
- [What makes a finding](#what-makes-a-finding)
- [The catalogue](#the-catalogue)
- [Design patterns](#design-patterns)
- [The direction dependencies may point](#the-direction-dependencies-may-point)
- [Five languages, one rule set](#five-languages-one-rule-set)
- [It applies the fixes that have one right answer](#it-applies-the-fixes-that-have-one-right-answer)
- [Did the refactoring actually happen](#did-the-refactoring-actually-happen)
- [Watch it while you work](#watch-it-while-you-work)
- [Which five do I fix on Monday](#which-five-do-i-fix-on-monday)
- [Levels](#levels)
- [Configuration](#configuration)
- [Families can be switched on and off](#families-can-be-switched-on-and-off)
- [The same rule, written twice, in two languages](#the-same-rule-written-twice-in-two-languages)
- [What the tests actually ran](#what-the-tests-actually-ran)
- [Confidence is never overstated](#confidence-is-never-overstated)
- [Editors](#editors)
- [Agents](#agents)
- [The interface](#the-interface)
- [CI](#ci)
- [Working on it](#working-on-it)
- [Performance](#performance)

</details>

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

Eighty-one rules across nine families. Every rule names the metric that decides it and
the refactoring that answers it, and neither is optional.

That traceability is enforced by the compiler. The catalogue is one declarative
table (`crates/rqx-core/src/catalogue.rs`) in which every field is mandatory, so a
rule that omits its metric or its fix does not build. A second test asserts that
**every rule in the catalogue actually fires on a fixture** — a rule nothing can
trigger is a documentation entry pretending to be a check.

`rqx rules` prints all 75 grouped by family, and `rqx explain <rule>` prints one in
full. [`docs/rules/`](docs/rules/README.md) is the same catalogue as a reference: what
each rule measures, the threshold that tunes it, the languages it cannot apply to, and
a worked example lifted from the fixture that fires it.

| Family | Rules | What it measures |
|---|---|---|
| **Size** | 11 | units that outgrew what a reader can hold |
| **Abstraction** | 8 | types that model the problem wrong |
| **Change** | 3 | shapes that make an edit expensive |
| **Waste** | 19 | code and comments nothing reaches |
| **Coupling** | 7 | modules that know too much about each other, and dependencies pointing the wrong way |
| **Flow** | 7 | control flow that says less than it looks like |
| **Naming** | 7 | names that mislead or say nothing |
| **Safety** | 9 | silent failure and swallowed errors |
| **Tests** | 4 | suites that pass without checking |

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

## The direction dependencies may point

ArchUnit does Java. dependency-cruiser does JavaScript. import-linter does Python.
Nothing checks a polyglot monorepo in one config — which is exactly where the drift
happens, because no single tool sees the whole thing.

```toml
[architecture]
layers = ["domain", "application", "adapters"]

[architecture.paths]
"src/domain/**"      = { layer = "domain" }
"src/application/**" = { layer = "application" }
"src/adapters/**"    = { layer = "adapters" }
"src/scripts/**"     = { may_only_import = ["src/adapters/**"] }
```

`layers` is ordered inner to outer: a file may import its own layer and any layer
before it, so the domain cannot reach for an adapter. The finding points at the import
line, in whichever of the five languages wrote it, because it reads the same module
graph the cycle detection already uses.

Silent until you declare it — an architecture nobody wrote down is not one rqx should
infer — and a layer named in `paths` that `layers` never declares is an error rather
than a rule that quietly governs nothing.

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

## It applies the fixes that have one right answer

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

Ten rules qualify today. `[fix] disabled = ["commented-out-code"]` opts one out;
`rqx explain <rule>` says whether a rule is fixable. The other fifty-three are
`rqx fix --plan`, which emits them with the span, the source text, the metric, the
reasoning and the named refactoring — the brief for a person or an agent, in the shape
they can act on. `rqx check --base main --fix` does the same for a branch and judges
the gate afterwards, so a branch whose only failures were mechanical passes.

Your editor offers the same edits, verified the same way, tagged `source.fixAll` so
they can run on save.

## Did the refactoring actually happen

rqx names a refactoring for every finding. Nothing checked whether it was made —
and now that agents write most refactors, that gap is where wrong diffs get merged.

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

## Watch it while you work

```
rqx serve --open
```

A dashboard on `127.0.0.1`, updated as you save. It re-analyses the file you touched
and nothing else — a save costs about 6 ms on a small project, and the page shows the
change within one poll interval.

| Page | Shows |
|---|---|
| Overview | health, the trend, the grade distribution, what to open first, and what the project would score under each level |
| Map | every file as a tile, sized by lines and coloured by grade — the shape of the codebase in one screen |
| Graph | what imports what, coloured by grade, with every cycle drawn in red |
| Findings | every finding, sorted and grouped how you like, filtered by severity, family, rule, path glob, grade, confidence, fixability or whether the baseline already knew |
| Files | one row per file, searchable and sortable, worst grade first |
| Plan | the ranked plan with a budget slider, re-ranked as you drag it |
| Patterns | which design patterns are half applied, and where |
| Check | review the diff against any base and see the gate's verdict |
| Rules | the catalogue, with how often each rule fired here |
| Waivers | every `rqx-ignore`, its reason and its date |
| Config | the effective settings and which file set each one |
| Activity | every save as it lands, with what it resolved and introduced |

**It can act, not only look.** Open a finding and the drawer offers the proven fix —
shown as the edits it would make before it makes any of them — a waiver with a reason,
and switching the rule off. Every write goes back through the same functions `rqx fix`
and `rqx suppress` use, so nothing happens here that the command line would refuse.
`--read-only` serves the page without them.

It is the terminal's own theme: the palette comes from `brand.rs` rather than being
restated, so retuning a colour moves both, and the owl carries the verdict the way it
does in `rqx` — a failing repository is a red owl, not a brown one with a red mark on
it. Dark, warm and readable from a phone to a wide monitor.

`⌘K` jumps to a page, a file or a rule, and the whole view — page, filters, sort and
grouping — lives in the address, so a filtered list can be reloaded or sent to somebody.
The page is served from the binary — no Node, no network, no build step — and binds
loopback only, refusing any other address: it renders your source and has no way to
authenticate a browser. A write additionally has to carry the nonce the page was served
with, which a form on another site cannot read.

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
categories = ["waste", "coupling"]           # only these families
disabled   = ["magic-number"]                # or a single rule
enabled    = ["short-name-wide-scope"]       # opt into an off-by-default rule
```

Ten rules ship off by default — the ones whose signal is real but noisy, or which
encode a convention a given project may not share. They are listed with
`(off by default)` in `rqx rules`.

## The same rule, written twice, in two languages

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

## What the tests actually ran

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

```
rqx setup            # every editor rqx knows
rqx setup neovim     # the config it needs
rqx setup neovim -w  # appended for you, after showing what will be added
```

| | Diagnostics | Cross-file rules | Hover | Quick fixes | Code lens | Commands | Views | Dashboard |
|---|---|---|---|---|---|---|---|---|
| **VS Code**, Cursor, Windsurf, VSCodium | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | editor tab |
| **Zed** | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | browser |
| **JetBrains** IDEA, WebStorm, PyCharm, GoLand, CLion | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | tool window |
| **Neovim** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | browser |
| **Helix** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | browser |
| **Sublime Text** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | browser |
| **Emacs** eglot, lsp-mode | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | browser |
| **Vim** coc.nvim, vim-lsp | ✓ | ✓ | ✓ | ✓ | — | coc only | — | browser |
| **Kate** | ✓ | ✓ | ✓ | ✓ | — | — | — | browser |

VS Code and Zed ship extensions that fetch the matching binary themselves; the JetBrains
plugin runs on Community editions and shows the dashboard in a tool window. **Views** are
the sidebar's own findings, files and rules trees, which only VS Code has a place for —
everywhere else the dashboard is that view. Everything else runs `rqx lsp`, so a
capability added to the server appears in every editor in the same commit — which is why
these clients stay small enough to survive being unmaintained.

**Quick fixes are the real edits**, verified exactly as `rqx fix` verifies them, tagged
`source.fixAll` so an editor can apply every one of them on save. The rules whose fix is
a decision offer to explain themselves or turn the rule off instead.

**The dashboard is one page.** `rqx.dashboard` starts `rqx serve` and answers with its
URL, so VS Code shows it in an editor tab, JetBrains in a tool window, and every other
editor opens it beside the window — the same page, not a second half-built one per
client. Asking twice answers with the same URL rather than starting a second server.

Cross-file findings arrive without being asked for: the server scans the workspace on
its own and republishes every file, so ten of the seventy-five rules do not go silently
missing in an editor that has one file open.

[docs/editor.md](docs/editor.md) has the commands, the custom notifications and a
paste-in block per editor.

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
| `verify` | What a diff did to every finding: resolved, improved, relocated, introduced |
| `analyze` | An unsaved buffer, without touching the disk |
| `patterns` · `explain` · `rules` · `config` | The catalogues, and the settings in force |

All ten are read-only: nothing rqx exposes to an agent writes a file. `plan` returns
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
| Findings | every finding with the offending code marked, and the keys to filter, group, open, fix or waive it |
| Files | one row per file, worst grade first, with churn and blast radius |
| Plan | the work ranked by what it buys against what it costs, with a budget |
| Patterns | which design patterns are in use, which are half applied, and where one would fit |
| Rules | the catalogue, searchable, with each rule's family, metric, fix and why |
| Config | the effective settings and where each one came from |

`tab` cycles all seven, `1`–`7` jump, `r` runs a scan, `/` searches, `f` filters by rule,
family and path in the same spelling the flags take, `g` groups, `?` lists the keys of the
screen you are on, `esc` steps back, `q` leaves and prints what the session found.

`enter` on a finding opens it in `$EDITOR` at its line and comes back when you leave;
`enter` on a file shows that file's findings. `a` proves the fixes for a file and shows
what they would change, `y` writes them, and `x` waives the selected finding once you
give a reason. A tab opened before a scan says what it will hold rather than refusing to
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
