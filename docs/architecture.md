# Architecture

How rqx is put together, for people changing it. For user-facing behaviour, start at
the [documentation index](README.md).

## The pipeline

A scan runs in the stages below. `rqx scan`, `rqx check`, the interactive session, the
language server and the MCP server all use the same stages and differ in how they
present the result.

```text
  rqx.yaml layers, flags ──► Resolved settings
                                   │
  git history (own thread) ────────┤  starts before the walk
  cache load (own thread) ─────────┤  starts before the walk
                                   ▼
  walk: ignore::WalkParallel, N worker threads
    per file:  stamp matches the cache entry?  ──yes──► cached analysis
                    │ no
                    ▼
               read, compare fingerprint ──hit──► cached analysis
                    │ miss
                    ▼
               tree-sitter parse, per-file rules ──► store in cache
                                   │
                  ScanMessage::File, bounded channel (256)
                                   ▼
  collector (one thread): gathers per-file facts and findings
                                   │
          ┌────────────────────────┼────────────────────────┐
          ▼                        ▼                        ▼
   duplicates                 import graph             git history
   clone groups,              cycles, unused           churn, hotspots,
   ported code                exports, layers,         single author,
                              module size              co-change
          └────────────────────────┼────────────────────────┘
                                   ▼
  total sort of findings, advice, waivers ──► ScanOutcome
                                   │
                         gate::judge ──► Verdict
                                   │
               reporters (terminal, json, sarif, ...) or the session
```

1. **Settings.** `crates/rqx-cli/src/config.rs` merges the layers described in
   [configuration.md](configuration.md) into a `Resolved` value. It also holds the
   per-directory overrides, so settings can differ per file (`Resolved::for_file`).
2. **Walk.** `build_walker` in `crates/rqx-cli/src/scan/walk.rs` builds an
   `ignore::WalkParallel` restricted to the enabled languages' extensions, the
   `ignore.patterns` globs and `scan.prune_dirs`. `scan.threads: 0` uses the available
   parallelism.
3. **Per-file analysis.** Each worker calls `analyze_file`. A file larger than
   `scan.max_filesize` is skipped with a reason. Otherwise the worker consults the
   cache, and on a miss calls `rqx_lang::analyze_planned`, which parses the file and
   runs every rule that needs only that file. Which facts it builds is the file's
   `FactPlan` (`crates/rqx-core/src/fact_plan.rs`): the union of `RuleId::facts` over
   the rules and advisories enabled for that file, plus what any enabled rule reads
   from every file, such as the imports the graph is built from. The parse tree's
   metrics are always built; the scope resolver and the collectors on it are built
   only when the plan includes them. A rule's declared `Analysis` is documentation and
   does not decide what is built. The result is a `FileAnalysis`: findings,
   advice, clone sites, module facts, suppressions and other facts the cross-file
   stages need.
4. **Collector.** One thread receives `ScanMessage` values over a bounded channel
   (`CHANNEL_DEPTH`, 256). The bound makes workers wait when the collector is behind,
   so peak memory depends on the number of workers and not on the number of files.
5. **Cross-file stages.** When the walk ends, the collector runs three stages in
   parallel with `std::thread::scope` and absorbs their results in a fixed order:
   duplicates (clone groups and cross-language ports), the import graph (cycles,
   unused files and exports, layer violations, module size) and git history (churn,
   hotspots, single-author files, stale TODOs, co-change). These stages run on every
   scan, including a fully cached one, because a deleted import changes what the graph
   says about files that did not change. A stage proposes findings and does not
   decide whether they are reported. `Collector::absorb` admits each one under the
   settings of the file it is reported in (`Resolved::for_file`), which is the same
   resolution a per-file finding gets in its worker, and the stages read their
   thresholds from the same file's settings. A finding that spans files, such as a
   duplicated block or an import cycle, is judged by the file of its primary location.
6. **Outcome.** The collector sorts findings, advice and waivers with total keys and
   returns a `ScanOutcome`.
7. **Gate.** `gate::judge` in `crates/rqx-cli/src/gate.rs` turns an outcome and the
   `gate` settings into a `Verdict`, which decides the exit code.
8. **Output.** Reporters in `crates/rqx-cli/src/report/` write the outcome in each
   format. The interactive session in `crates/rqx-cli/src/session/` keeps one outcome
   and prints from it. `crates/rqx-cli/src/render.rs` holds the output functions that
   both the command line and the session call.

## Crates

| Crate | Role |
|---|---|
| `rqx-core` | The data model and the rules of the game: the rule catalogue (`catalogue.rs`, `rule.rs`), findings and their evidence (`finding.rs`), finding identity for baselines (`identity.rs`), the health score (`score.rs`), configuration types and the `rqx.yaml` template (`config.rs`, `rqx.template.yaml`), levels and presets, suppressions (`suppress.rs`), the design-pattern catalogue (`pattern.rs`, `advice.rs`), refactoring names (`technique.rs`), coverage and runtime profiles, and the import-impact map (`impact.rs`). No parsing and no I/O beyond reading configuration. |
| `rqx-lang` | The tree-sitter side. `profile.rs` defines `LanguageProfile`, the table of node kinds each language fills in. `lib.rs` defines `Support` (module-surface reader, import resolver and class assembly per language) and `analyze`. Shared collectors read the profile: `hygiene/`, `metrics.rs`, `classes.rs`, `scope.rs`, `patterns.rs`, `parameters.rs`, `hierarchy.rs`, `module_graph.rs`. `custom.rs` runs user-written queries, `rewrite.rs` produces autofix edits, and `highlight.rs` colours source excerpts. One directory per language (`typescript`, `javascript`, `python`, `go`, `java`, `cpp`) holds its profile, surface reader and import resolver. |
| `rqx-cli` | The binary. `main.rs` defines the clap parser and dispatches commands. `scan/` and `cache.rs` are the pipeline above. `check.rs`, `fix.rs`, `verify.rs`, `plan.rs`, `graph.rs`, `perf.rs`, `health.rs`, `complexity.rs`, `recommend.rs`, `slop.rs`, `hotspots.rs` and `badge.rs` implement the commands. `history.rs` reads git. `forge.rs` posts comments to GitLab and GitHub, `update.rs` and `release.rs` implement `rqx update`. `report/` holds the reporters and the docs generator (`report/docs.rs`). `session/`, `lsp/` and `mcp/` are the interactive session, the language server and the MCP server. |

Rules are written once against facts that a language backend fills in. A rule in
`rqx-core` or `rqx-lang` never names a language, so a language is added by supplying
tables and readers and not by editing rules. A rule that cannot exist in a language is
declared in its catalogue entry with a reason.

## The cache

The cache is the file `.rqx/cache.bin` under the scanned root. `.rqx/` also holds
`baseline.json`, so `rqx cache clear` removes the cache file and leaves the directory.
When `.rqx/` has no `.gitignore`, saving the cache or recording a trend snapshot there
writes one that lists `cache.bin`, `cache.json`, `.cache.bin.rqx-tmp` and
`trend.ndjson`. `baseline.json` is not listed, because it is meant to be committed.
`rqx cache status` reports what the cache holds. `scan.cache: false` in `rqx.yaml`
turns it off.

- The file is an index followed by one compressed entry per source file. Each worker
  inflates only the entries it uses.
- An entry is keyed by the file's path relative to the scan root. It stores a
  fingerprint of the source (FNV-1a over the text) and a stamp (length and
  modification time in nanoseconds).
- A matching stamp serves the entry without reading the file. A file modified within
  two seconds of the moment the cache was written is not trusted by stamp, because a
  coarse clock can give two different edits the same time. It is read and compared by
  fingerprint instead.
- The index carries a version, which is a hash of the cache format number, the rqx
  package version, a signature of the settings that change what a finding is
  (thresholds, rule settings, pattern settings, `scan.skip_generated`,
  `scan.skip_minified`, the facts those
  rule settings build, the project's custom rules, and the same settings in each
  directory override), and a fingerprint of the catalogue (every rule
  slug, severity, confidence and off-by-default flag, and every advisory slug). A
  different version makes every entry a miss.
- Only per-file results are cached. The cross-file stages and the git history are
  computed on every run.

## Determinism

Output does not depend on the number of threads. Workers finish in an arbitrary order,
so the pipeline never relies on arrival order:

- The final sort keys for findings, advice and waivers are total. Findings sort by
  rank, then file, line, start offset, end offset and fix text, so two findings on one
  line (two unused parameters, for example) keep a fixed order.
- The results of the cross-file stages are absorbed in a fixed sequence, regardless of
  which stage finishes first.
- Tests in `crates/rqx-cli/tests/fixtures.rs` compare `--threads 1` against
  `--threads 16` for the TypeScript and JavaScript fixtures, and `--threads 1` against
  `--threads 8` for advice.

A change that adds a collection filled by workers must sort it before it reaches the
outcome. A thread-count test failing is the usual sign that one was missed.

## Guide: add a rule

Rules are a declarative table. Every field is mandatory, so a rule that omits its
metric or the refactoring it names does not compile, and a test asserts that every rule
fires on a fixture. The compiler lists most of the steps below through exhaustive
matches.

1. **Catalogue entry**, in `crates/rqx-core/src/catalogue.rs`: slug, family, analysis
   tier (`Syntax`, `Semantics` or `Modules`), metric, fixes, confidence, severity,
   `why`, and a `not_applicable` entry with a reason for each language the rule cannot
   exist in.
2. **Evidence**, in `crates/rqx-core/src/finding.rs`: an `Evidence` variant and its
   `Display`, unless an existing variant fits.
3. **Identity**, in `crates/rqx-core/src/identity.rs`: `subject()` and `magnitude()` are
   exhaustive, so a new variant states whether it names something and whether it has a
   number that can shrink.
4. **Thresholds**, if the rule has one: the field, its default, a `TUNES` row, the
   template entry with its prose in `rqx.template.yaml`, and the `Comparable` list.
   Tests keep those four in step.
5. **Collector**: `crates/rqx-lang/src/hygiene/` for most checks, `metrics.rs` for
   measurements, `classes.rs` and `patterns.rs` for structure. Use a
   `LanguageProfile` table before writing per-language code, which is what lets one
   rule work in six languages.
6. **Rule logic**, in `crates/rqx-core/src/rules.rs`: turn the collected fact into a
   finding.
7. **Fixtures**: one per language the rule applies to, under `fixtures/<language>/`.
   Record the findings in each language's `.expect` file with `RQX_BLESS=1`, as
   described in [Fixture expectations](#fixture-expectations).
   `every_applicable_rule_fires_on_every_language` scans with `fixtures/all-rules.yaml`,
   which enables the off-by-default rules. A rule exempted from a language must stay
   silent on that language's fixtures.
8. **Levels**: `relaxed`, `strict` and `pristine` each need a value if the rule has a
   threshold, and `pristine` must name the rule if it ships off by default.
9. **Autofix**, if the rule declares one: an arm in `crates/rqx-lang/src/rewrite.rs`
   and an entry in its test list. A test fails for a rule that claims a mechanical fix
   and has none.
10. **Generated documentation**: run `cargo run --quiet -- docs`. It rewrites
    `docs/rules/`, `docs/cli.md`, the manual pages, the schema, `docs/perf-precision.md`
    and the catalogue table in the README. A test fails if the checked-in copies differ.

Fixtures are deliberate violations. Do not run `rqx fix` over `fixtures/` and do not
correct the code in them. Run `tools/dev.sh check` before opening a pull request.

### Fixture expectations

`every_fixture_finding_is_expected` in `crates/rqx-cli/tests/fixtures.rs` scans each
`fixtures/<language>/` directory with the default configuration and compares the
findings with that directory's `.expect` file. A finding without an entry fails the
test. The harness is in `crates/rqx-cli/tests/fixture_expectations/`.

Each line of the file is one entry: the path relative to the language directory, the
expectation, and the trimmed source line after ` | `.

```text
fixtures/bloat.go rule=no-base-case | func Loops(rows []int) int {
fixtures/params.go rule=unused-parameter x2 | func TwoDeadOnOneLine(used int, deadA string, deadB string) int {
fixtures/patterns.go rule=redundant-else #2 | } else if d.state == "open" {
hierarchy.py known-fp=discarded-error _count_rows returns a number and cannot fail, so no error is dropped | _ = _count_rows(rows)
```

| Entry | Passes when | Written by |
|---|---|---|
| `rule=<slug>` | The rule fires on the line, exactly `xN` times if a count is given | `RQX_BLESS=1` |
| `none=<slug>` | The rule does not fire on the line | Hand |
| `may=<slug>` | Always; the rule may fire on the line or not | Hand |
| `known-fp=<slug> <reason>` | The rule fires on the line. The finding is wrong for the stated reason | Hand |

An entry locates its line by text, so adding lines elsewhere in a fixture changes no
entry. `#N` selects the Nth line in the file with that text. Line numbers appear only
in failure messages.

The expectations are kept outside the fixture sources because comments are analysed.
A marker comment on a finding's line, or the line above it, changes the results of
rules such as `comment-per-line` and `duplicate-code`.

`RQX_BLESS=1 cargo test -p rqx --test fixtures every_fixture_finding_is_expected`
regenerates the `rule=` entries in source order and leaves the other kinds as written.
The test still fails afterwards in three cases, each of which needs an edit by hand:

- a rule fires on a line marked `none=`;
- a `known-fp=` finding stopped firing, which means the false positive is fixed and the
  entry should become `none=`;
- a hand-written entry names a line that no longer exists.

`hotspot`, `single-author-file`, `shotgun-surgery` and `stale-todo` read the history of
the repository that contains the fixtures, so they have no entries. A test that builds
its own repository covers them.

## Guide: add a precision corpus case

`fixtures/complexity` holds the functions the cost analysis is scored on, and
`crates/rqx-cli/tests/complexity_corpus.rs` runs it as part of `cargo test`. A case is
a function with a comment above it that gives the answer:

```text
// @complexity time=O(n*m) space=O(n) conf=verified rule=scan-in-loop none=sort-in-loop
```

(`#` replaces `//` in Python.) `time` and `space` are orders, with `space` counting the
call stack. `rule` names rules that must report on the function, `none` rules that must
not, and `may` a rule that describes the same code under another name, such as
`linear-op-in-loop` beside `scan-in-loop`. `scope=local` says the time is the function's
own, with no calls, so the extraction alone is held to it.

1. **Add the function** to a file under `fixtures/complexity/<language>/`, or to a
   directory under `fixtures/complexity/projects/` when the cost depends on a call into
   another file. Write the annotation from what the code does and not from what
   `rqx perf` prints for it.
2. **Read the result.** `cargo test -p rqx --test complexity_corpus -- --nocapture`
   lists each function that is not exact and each rule mistake. Where the analysis is
   wrong, leave the annotation as it is. Where the annotation is wrong, correct it.
3. **Re-bless.** `RQX_BLESS=1 cargo test -p rqx --test complexity_corpus` rewrites
   `fixtures/complexity/baseline.json` and `crates/rqx-core/src/precision/rules.json`,
   the copy of the rule counts that `rqx explain` embeds. Then run
   `cargo run --quiet -- docs` to regenerate `docs/perf-precision.md`. Commit the four
   files with the case.

The test fails when a function that was exact is no longer exact, and when a language's
precision or recall, or a rule's right, false alarm or missed count, gets worse than the
baseline. Re-blessing a lower number is a decision for the commit message to explain.

Functions copied from real projects go under `fixtures/complexity/real/<language>/`
and are scored in the `real` section of the baseline. Each file names the project and
commit it was copied from, one of the commits in `tools/bench/corpus.lock`, and the
project's license text is in `fixtures/complexity/real/NOTICE`.

## Guide: add a language

1. **Language value**, in `crates/rqx-core/src/language.rs`: a `Language` variant,
   `ALL`, `slug`, `title`, `extensions` and the other matches the compiler reports.
   Keep the new variant last, because the clone salt and the per-language counters
   depend on the existing order. `crates/rqx-core/src/suppress.rs` needs the comment
   syntax used by `rqx-ignore` waivers.
2. **Grammar**: add the `tree-sitter-<language>` crate to `crates/rqx-lang/Cargo.toml`.
3. **Profile**: `crates/rqx-lang/src/<language>/profile.rs` defines a
   `LanguageProfile` static: the grammar, function, class, method, field, parameter
   and statement kinds, block kinds, and the vocabularies for tests and performance
   rules. Compare `go/profile.rs` and `java/profile.rs`.
4. **Module surface**: `surface.rs` reads what a file exports and imports into
   `ModuleFacts`.
5. **Import resolver**: `imports.rs` implements `Resolver`, turning a written import
   into a project file. Use the language's own manifest, such as `go.mod`.
6. **Class assembly**: `classes::units` handles a class in one node. A language that
   declares a type and its methods in different places supplies its own `units`, as Go
   does.
7. **Registration**: add a `Support` static and a `support()` arm in
   `crates/rqx-lang/src/lib.rs`, and declare the module there. `highlight.rs` and
   `rewrite.rs` have language-specific arms that the compiler reports.
8. **Exemptions**: for each rule that cannot exist in the language, add a
   `not_applicable` entry with a reason in the catalogue.
9. **Fixtures**: `fixtures/<language>/` with the entry-point manifest the language
   uses, one fixture per applicable rule, and an `.expect` file generated with
   `RQX_BLESS=1` (see [Fixture expectations](#fixture-expectations)). Every
   applicable rule must fire and every exempted rule must stay silent.
10. **Documentation**: update the language table in the README and run
    `cargo run --quiet -- docs`.

## Guide: add a slash command

A slash command is a command-line command. The session parses `/plan --budget 20` with
the same clap definition as `rqx plan --budget 20`, so a new command appears in the
session without a second parser.

1. **Define the command** in `crates/rqx-cli/src/main.rs`: a variant of `Command` with
   its flags, and a place in the grouped command list (a test fails for a command that
   is not listed under a heading).
2. **Write the output function** in `crates/rqx-cli/src/render.rs`. It usually takes a
   `ScanOutcome`, a request value, a `Palette` and a `Write`, and prints. The command line
   and the session both call it, which keeps their output the same.
3. **Dispatch from the command line**: add the match arm in `main.rs` that scans and
   calls the render function.
4. **Dispatch from the session**: add the command name to `TWINS` in
   `crates/rqx-cli/src/session/commands.rs`, adjust the array length, and add a match
   arm that runs the render function on the session's existing outcome through
   `print`. Completion and `/help` read the clap definition, so flags and the one-line
   description appear without more work.
5. **Document it**: run `cargo run --quiet -- docs` to update `docs/cli.md` and the
   manual pages, and describe the command in the page that fits it.

A command that needs something a scan does not hold, such as a base revision, goes back
to the files and does not use the session's outcome. `check`, `verify` and `fix` do this.
Commands that exist only in the session (`rescan`, `watch`, `open`, `clear`, `help` and
`exit`) are listed in `SESSION_ONLY` instead.
