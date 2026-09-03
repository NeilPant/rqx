# rqx

Finds dead code, duplication and architectural drift in TypeScript — and tells you
*why* each finding matters and *what refactoring* fixes it.

One Rust binary. No Node, no npm, no runtime dependencies.

```
rqx                             # pick what to do, then browse the findings
rqx scan ./src                  # report everything
rqx scan --severity error       # only the findings that are never false
rqx check --base main           # review only what changed, fail CI on the gate
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

`rqx rules` prints all 42, grouped by refactoring.guru's five families plus three of
Clean Code's own. `rqx explain <rule>` prints one in full.

| Family | Rules |
|---|---|
| **Bloaters** | long-function, cognitive-complexity, deep-nesting, long-parameter-list, large-class, data-clump, primitive-obsession |
| **Object-Orientation Abusers** | type-code-switch, temporary-field, refused-bequest, base-depends-on-derived, inappropriate-static |
| **Change Preventers** | shotgun-surgery, divergent-change |
| **Dispensables** | duplicate-code, unused-export, unused-file, unused-parameter, unused-local, unused-private-member, unreachable-code, commented-out-code, lazy-class, data-class, speculative-generality, empty-catch, magic-number |
| **Couplers** | circular-dependency, message-chain, feature-envy, middle-man |
| **Naming** (Clean Code ch. 2) | meaningless-name, short-name-wide-scope, hungarian-notation |
| **Safety** (G4, F2, F3, G28, G29) | suppressed-diagnostic, flag-argument, output-parameter, complex-condition, negative-conditional |
| **Tests** (Clean Code ch. 9) | skipped-test, focused-test, assertionless-test |

### What is deliberately absent

Of the 88 entries across both catalogues, 42 are implemented. The rest fall into two
groups, and neither is a backlog:

**Needs a type checker** — Primitive Obsession beyond string-literal type codes,
Refused Bequest for members a subclass silently ignores, dead branches after type
narrowing. These wait for an opt-in `--type-aware` pass rather than being guessed at.

**Not statically decidable** — G21 "Understand the algorithm", C2 "Obsolete comment",
C4 "Poorly written comment", G2 "Obvious behavior is unimplemented", T2 "Use a
coverage tool", E1/E2 build and test process. A tool that claimed to check these
would be inventing findings, which costs more trust than the missing rules do.

## Families can be switched on and off

```toml
[rules]
categories = ["dispensables", "couplers"]   # only these families
disabled   = ["magic-number"]                # or a single rule
enabled    = ["naming"]                      # opt into an off-by-default family
```

Six rules ship off by default — the ones whose signal is real but noisy, or which
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

The interface has no animation: it renders once per change and then blocks on
input, so it costs nothing to leave open.

## CI

`rqx check --base main` reviews the changed files plus everything that imports
them, and exits non-zero when the gate fails.

| Exit code | Meaning |
|---|---|
| `0` | ran, gate satisfied |
| `1` | ran, gate failed |
| `2` | rqx could not run |

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

The gate lives in `rqx.toml` and every part of it can be overridden per run with
`--fail-on`, `--allow-findings` and `--min-health`, so a strict gate on the
default branch and a lenient one on feature branches do not need two config files.

```toml
# rqx.toml
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

A 12k-file, 468k-line repository: **~700ms**, with memory a function of thread
count rather than file count. Output is byte-identical regardless of how many
threads it ran on, so a CI gate is never flaky.
