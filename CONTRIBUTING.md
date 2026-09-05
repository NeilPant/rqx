# Contributing

```sh
./rqx scan ../some-project      # builds, then execs — no install
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
```

`./rqx` execs the binary rather than using `cargo run`, so cargo's progress output
never lands in the middle of the interface and paths resolve against your own working
directory.

## Adding a rule

Rules are a declarative table. Every field is mandatory, so a rule that omits its
metric or the refactoring it names does not compile, and a test asserts every rule
fires on a fixture — a rule nothing can trigger is a documentation entry pretending
to be a check.

The compiler will walk you through most of this. In order:

1. **`crates/rqx-core/src/catalogue.rs`** — the entry. Slug, family, analysis tier,
   metric, fixes, confidence, severity, and `why` in rqx's own words.
2. **`crates/rqx-core/src/finding.rs`** — an `Evidence` variant and its `Display`,
   unless an existing one fits.
3. **`crates/rqx-core/src/identity.rs`** — `subject()` and `magnitude()` are
   exhaustive on purpose: a new variant has to say whether it names something and
   whether it has a number that can shrink.
4. **Thresholds**, if it has one: the field, its default, a `TUNES` row, the template
   entry with its prose, and the `Comparable` list. Tests hold those four in step.
5. **The collector** — `crates/rqx-lang/src/hygiene.rs` for most things,
   `metrics.rs` for measurements, `classes.rs` and `patterns.rs` for structure. Reach
   for a `LanguageProfile` table before writing per-language code; that is what makes
   one rule work in five languages.
6. **`crates/rqx-core/src/rules.rs`** — turn the fact into a finding.
7. **Fixtures** — one per language it applies to, firing the exact number of times
   the count table in `crates/rqx-cli/tests/fixtures.rs` says.
8. **Levels** — `relaxed`, `strict` and `pristine` each need a value if it has a
   threshold, and `pristine` must name it if it ships off by default.
9. **`rqx docs`** — regenerates the reference. A test compares it byte for byte.

If it declares an autofix, `crates/rqx-lang/src/rewrite.rs` needs an arm and a test
list entry; another test catches a rule that claims a mechanical fix and has none.

## What a rule has to be

**Decidable without types.** rqx parses; it does not type-check. Syntax, scope
resolution, the import graph and git history are what a rule may read. Anything else
ships `Confidence::Low` and off by default, or not at all.

**Honest about what it knows.** `Verified` means the analysis cannot be wrong.
`Heuristic` means a threshold was compared. `Low` means it guesses, and those ship
off.

**Named by its fix.** A finding that says something is bad without saying what to do
is a complaint.

## Precision

Before a rule ships on by default, run it on real repositories and read every finding
it produces. Below roughly 70% true positives it is `Heuristic` and off until the
collector improves. Put the numbers in the pull request — they are the review.

## Style

The code aims to read like prose. Comments carry reasons, never descriptions: if a
comment says what the code does rather than why it is that way, the code needs a
better name instead. Tests are named as sentences that state what must hold.
