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
does not declare its smell, metric and fix does not build.

## The catalogue

Two sources are canonical: **refactoring.guru's** smell and refactoring catalogues,
and **Clean Code (Robert C. Martin), ch. 17 "Smells and Heuristics"**. Every rule
cites an entry in at least one, names the metric that decides it, and names the
refactoring that fixes it — verbatim from refactoring.guru's technique list.

That traceability is enforced by the compiler. The catalogue is one declarative
table (`crates/rqx-core/src/catalogue.rs`) in which every field is mandatory, so a
rule that omits its smell, its metric or its fix does not build. A test additionally
asserts that every Clean Code citation names a real heuristic identifier, and
another asserts that **every rule in the catalogue actually fires on a fixture** —
a rule nothing can trigger is a documentation entry pretending to be a check.

`rqx rules` prints all 60 grouped by family, and `rqx explain <rule>` prints one in
full. [`docs/rules/`](docs/rules/README.md) is the same catalogue as a reference: what
each rule measures, the threshold that tunes it, the languages it cannot apply to, and
a worked example lifted from the fixture that fires it.

| Family | Rules |
|---|---|
| **Bloaters** | 8 |
| **Object-Orientation Abusers** | 7 |
| **Change Preventers** | 3 |
| **Dispensables** | 18 |
| **Couplers** | 5 |
| **Naming** (Clean Code ch. 2) | 7 |
| **Safety** (Clean Code ch. 17 general) | 8 |
| **Tests** (Clean Code ch. 9) | 4 |

Those pages are generated from the catalogue by `rqx docs`, and a test fails if they
drift from it — a hand-written table of sixty rules is a table that goes stale, which
is what the one here had already done.

### What is deliberately absent

Both catalogues are held as data, so the coverage is counted rather than claimed —
`rqx rules --coverage` prints every one of refactoring.guru's 22 smells and Clean
Code's 66 heuristics with what Rqx does about it, and the test suite fails if an
entry has no answer. `rqx techniques` does the same for the 66 refactorings.

An entry Rqx does not report says why:

**Not yet** — decidable from the source, no rule for it. This is the only group that
is a backlog, and it is the only group whose size the suite pins to an exact number.

**Not decidable** — G21 "Understand the algorithm", C2 "Obsolete comment", C4 "Poorly
written comment", G2 "Obvious behavior is unimplemented", T2 "Use a coverage tool",
E1/E2 build and test process. A tool that claimed to check these would be inventing
findings, which costs more trust than the missing rules do.

**Not applicable** — J1–J3 are Java-specific; TypeScript has no wildcard imports and
no constants to inherit.

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

## Editor

`rqx lsp` is a language server; `editors/vscode` is the client. Diagnostics on save
(2ms for a file), hover for the full explanation, workspace health in the status
bar.

## The interface

`rqx` with no arguments opens a full-screen interface. Every screen is reachable
from the menu and every screen returns to it, so nothing is a dead end:

| Tab | Shows |
|---|---|
| Overview | health, severity and grade distributions, what to open first |
| Findings | every finding with the offending code marked, filter, group, severity floor |
| Files | one row per file, worst grade first, with churn and blast radius |
| Patterns | which design patterns are in use, which are half applied, and where one would fit |
| Rules | the catalogue, searchable, with each rule's smell, metric, fix and why |
| Coverage | every entry in both source catalogues and what Rqx does about it |
| Config | the effective settings and where each one came from |

`tab` cycles all seven, `1`–`7` jump, `r` runs a scan, `/` filters, `?` lists the keys
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
