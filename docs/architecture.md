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
2. **Walk.** `build_walker` in `crates/rqx-cli/src/scan.rs` builds an
   `ignore::WalkParallel` restricted to the enabled languages' extensions, the
   `ignore.patterns` globs and `scan.prune_dirs`. `scan.threads: 0` uses the available
   parallelism.
3. **Per-file analysis.** Each worker calls `analyze_file`. A file larger than
   `scan.max_filesize` is skipped with a reason. Otherwise the worker consults the
   cache, and on a miss calls `rqx_lang::analyze`, which parses the file and runs
   every rule that needs only that file. The result is a `FileAnalysis`: findings,
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
   says about files that did not change.
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
| `rqx-lang` | The tree-sitter side. `profile.rs` defines `LanguageProfile`, the table of node kinds each language fills in. `lib.rs` defines `Support` (module-surface reader, import resolver and class assembly per language) and `analyze`. Shared collectors read the profile: `hygiene.rs`, `metrics.rs`, `classes.rs`, `scope.rs`, `patterns.rs`, `parameters.rs`, `hierarchy.rs`, `module_graph.rs`. `custom.rs` runs user-written queries, `rewrite.rs` produces autofix edits, and `highlight.rs` colours source excerpts. One directory per language (`typescript`, `javascript`, `python`, `go`, `java`, `cpp`) holds its profile, surface reader and import resolver. |
| `rqx-cli` | The binary. `main.rs` defines the clap parser and dispatches commands. `scan.rs` and `cache.rs` are the pipeline above. `check.rs`, `fix.rs`, `verify.rs`, `plan.rs`, `graph.rs`, `perf.rs`, `health.rs`, `complexity.rs`, `recommend.rs` and `slop.rs` implement the commands. `history.rs` reads git. `forge.rs` posts comments to GitLab and GitHub, `update.rs` and `release.rs` implement `rqx update`. `report/` holds the reporters and the docs generator (`report/docs.rs`). `session/`, `lsp/` and `mcp/` are the interactive session, the language server and the MCP server. |

Rules are written once against facts that a language backend fills in. A rule in
`rqx-core` or `rqx-lang` never names a language, so a language is added by supplying
tables and readers and not by editing rules. A rule that cannot exist in a language is
declared in its catalogue entry with a reason.

## The cache

The cache is the file `.rqx/cache.bin` under the scanned root. `.rqx/` also holds
`baseline.json`, so `rqx cache clear` removes the cache file and leaves the directory.
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
  (thresholds, rule settings, pattern settings, `scan.skip_generated` and the same
  settings in each directory override), and a fingerprint of the catalogue (every rule
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
5. **Collector**: `crates/rqx-lang/src/hygiene.rs` for most checks, `metrics.rs` for
   measurements, `classes.rs` and `patterns.rs` for structure. Use a
   `LanguageProfile` table before writing per-language code, which is what lets one
   rule work in six languages.
6. **Rule logic**, in `crates/rqx-core/src/rules.rs`: turn the collected fact into a
   finding.
7. **Fixtures**: one per language the rule applies to, under `fixtures/<language>/`.
   Add the expected number of findings to that language's table in
   `crates/rqx-cli/tests/fixtures.rs` (`every_rule_fires_the_number_of_times_its_fixtures_ask_for`).
   `fixtures/all-rules.yaml` enables the off-by-default rules for these tests. A rule
   exempted from a language must stay silent on that language's fixtures.
8. **Levels**: `relaxed`, `strict` and `pristine` each need a value if the rule has a
   threshold, and `pristine` must name the rule if it ships off by default.
9. **Autofix**, if the rule declares one: an arm in `crates/rqx-lang/src/rewrite.rs`
   and an entry in its test list. A test fails for a rule that claims a mechanical fix
   and has none.
10. **Generated documentation**: run `cargo run --quiet -- docs`. It rewrites
    `docs/rules/`, `docs/cli.md`, the manual pages, the schema and the catalogue table
    in the README. A test fails if the checked-in copies differ.

Fixtures are deliberate violations. Do not run `rqx fix` over `fixtures/` and do not
correct the code in them. Run `tools/dev.sh check` before opening a pull request.

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
   uses, one fixture per applicable rule, and a new table in `fixtures.rs`. Every
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
