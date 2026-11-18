# Configuring rqx

Everything lives in `rqx.yaml`. `rqx init` writes one with every setting at its default and a line of prose per table; `rqx config` prints what a run will actually use and which file each value came from.

## Where rqx looks

Layers are merged in this order, each one overriding the one above it. A table merges key by key, so a file can set one threshold without restating the others; a list replaces rather than appends, so a narrower layer can shorten one.

| | Source |
|---|---|
| 1 | the built-in defaults, and the `level` |
| 2 | every preset and file named by `extends`, in the order they are listed |
| 3 | `$XDG_CONFIG_HOME/rqx/rqx.yaml`, else `~/.config/rqx/rqx.yaml` (`%APPDATA%\\rqx\\rqx.yaml` on Windows) |
| 4 | the policy a project points at, which keeps a locked key from moving |
| 5 | every `rqx.yaml` from the scanned directory upward, outermost first |
| 6 | `RQX_CONFIG`, if it is set |
| 7 | `--config <path>` |
| 8 | the command's own flags |

Everything a file extends goes under *every* file rqx reads, not just under the file that named it, so a preset does not override a policy or a project's own key. [`docs/presets.md`](presets.md) lists the presets.

A directory may call its config `rqx.yaml`, `.rqx.yml`, `rqx.yml` or `.rqx.yaml`; holding two is an error. A `RQX.toml` is an error too: rqx no longer reads TOML, and skipping one could change what a pipeline gates on. `rqx config migrate` converts it.

The upward search stops at a directory holding `.git`, or at a file that sets `root: true`. A config above a checkout describes a different project, and reading it would make a scan depend on where the repository is located.

**An `ignore.patterns` glob is matched against paths relative to the directory being scanned**, not to the file the pattern is written in. `rqx scan .` and `rqx scan src` therefore see different things from one `patterns: [src/**]`, and a pattern naming a directory whose name appears again deeper in the tree (`fixtures/**` against a `fixtures/go/fixtures/`) matches in both places. Name one level more when that matters: `fixtures/*/**`.

## Code that is not the project's own

A scan leaves out what it can tell is someone else's code or a tool's output, and the report counts it by reason: git submodules hosted under another organisation (`scan.skip_submodules`), minified files (`scan.skip_minified`), files with a generator's header or a bundler's runtime (`scan.skip_generated`), and files `.gitattributes` marks `linguist-generated` or `linguist-vendored` (`scan.respect_gitattributes`). `rqx scan -v` lists every skipped file and the setting that brings it back. The scan also never enters a directory with a conventional vendored or build-output name (`scan.prune_dirs`), and does not count what is in one.

Anything else vendored, such as a library copied into the source tree under a name of its own, is left out with `ignore.patterns`. A list in a config file replaces the default list, so start from the one `rqx init` writes and add to it:

```yaml
ignore:
  patterns:
    - "**/*.d.ts"
    # ...the rest of the defaults...
    - "src/assets/vendor-lib/**"
```

## A subtree with different standards

```yaml
overrides:
  - paths: [src/generated/**, vendor/**]
    thresholds:
      long_function_statements: 200
    rules:
      disabled: [waste]
```

**An override's `paths` are matched against paths relative to the directory the config file is in**, however the files were reached: `rqx scan legacy`, `rqx scan .` from inside `legacy`, and a scan through a symlink all see `legacy/**` the same way. A file outside that directory is never covered.

An override applies to every finding reported in the files it covers, including the findings that need the whole project, such as `unused-export`, `wide-fan-out` and `duplicate-code`. A finding that involves more than one file, such as a duplicated block or an import cycle, follows the settings of the file it is reported at, its first location.

An override may set `thresholds`, `rules`, `patterns` and `level` and nothing else. Every other table describes the run as a whole, because the file walker is built once and the gate reaches one verdict. Naming one in an override is an error.

The same holds for names: a rule, family, pattern or stance that does not exist is an error, not a silent no-op, because a `categories` allowlist naming only unknown families would otherwise turn every rule off. `rqx rules` lists every name that is valid.

## Asking what a setting means

`rqx config explain thresholds.max_params` prints a setting's description, its type, its default and its current value, and says which file or preset set it. The words are the ones the editor shows from [`rqx.schema.json`](rqx.schema.json), which `rqx config schema` also prints, so the two use the same text. A misspelt key is refused with the closest real one and the keys beside it.

## Checking a config before it runs

`rqx config validate` resolves the config the way a scan does (presets and `extends`, the user config, the policy, every `rqx.yaml` up to the repository root, `RQX_CONFIG` and `--config`) and reports where it is wrong, with the file, line and column:

```
rqx.yaml:3:3: error: unknown field `bogus`, expected one of long_function_statements, …
✗ config is invalid
```

An **error** is anything that stops a run: a YAML mistake, an unknown key, a key the policy locks, a retired `RQX.toml`. It exits 2. A **warning** is something that loads and does nothing: a name in `rules.disabled` or `patterns.enabled` that is not in the catalogue, a threshold that only disabled rules read, an `overrides` entry with no `paths`. Warnings print and exit 0, or exit 1 under `--strict`. A scan still refuses the unknown names, so a warning here is a scan that would stop.

```
rqx.yaml:2:14: warning: `nonsense` in `rules.disabled` names nothing; …
✓ config is valid (1 file, 1 warning)
```

`rqx config validate [PATH]` takes the directory to resolve from, and `--config` and `--level` as `scan` does. `--json` prints one line per problem and a summary instead; `rqx config --json` does the same for the table of settings. Both formats are in [json.md](json.md).

## One rule, on its own terms

A rule's slug is a key under `rules`, and it takes the settings that belong to that rule alone.

```yaml
rules:
  long-parameter-list:
    severity: error
    max_params: 6
    paths: ["src/domain/**"]
  magic-number:
    enabled: false
```

| Key | Meaning |
|---|---|
| `severity` | `info`, `warning` or `error`: what the rule's findings are reported as, and so what `gate.fail_on` and the health score make of them. |
| `enabled` | `false` switches the rule off; `true` switches it on even when it ships off. It is the most specific word on the rule, so it wins over `disabled`, `enabled` and `categories`. |
| `paths` | Globs relative to the scanned directory. When given, the rule only reports in files they match. |
| `confidence` | `low`, `heuristic` or `verified`. Only verified findings count towards the health score. |
| a threshold | Any threshold the rule reads, by its key. The table below says which. It is the same setting as `thresholds.<key>`, written next to the rule; naming both with different values is an error. |

A key under `rules` that is not `disabled`, `enabled`, `categories` or a rule slug is an error that suggests the closest name, and so is an option the rule does not have. The same settings work inside an entry of `overrides`, which is how one subtree gets a stricter rule than the rest. A custom rule written under `rule` carries its own `severity` and `confidence`, so it is not a key here.

## Levels

`level: strict` in `rqx.yaml`, or `--level strict` on the command line, picks a named set of defaults. A level is merged under everything the project sets, so it moves the defaults and does not override a stated value; `rqx config` shows `level:strict` in the `from` column for a value the level moved. An entry of `overrides` may name its own `level`, so a project can be strict in its domain and relaxed in its scripts. `rqx levels` prints each one as the settings it would take to write by hand.

## The gate

`rqx check` always gates. `rqx scan` gates only when one of `--fail-on`, `--allow-findings`, `--min-health` or `--min-grade` is passed, so a `gate` mapping in `rqx.yaml` has no effect on a bare scan. This keeps a config file that appears above a project from changing the exit code of a pipeline that never asked for a gate.

`gate.owners` sets a health floor and a severity limit for each owner named in a CODEOWNERS file. See [ownership.md](ownership.md). `comment.mention_owners` makes `rqx comment` write those owners as real mentions.

`gate.baseline_match` takes `count` (the default) or `set`. It sets how many findings with one identity a baseline entry covers: as many as the baseline recorded, or all of them. See [baseline.md](baseline.md#repeated-findings).

`gate.complexity_regression` takes `off` (the default), `warning` or `error`. It sets what a function whose time order grew against the base revision does to the verdict of `rqx check`: `warning` reports it, and `error` fails the run. Anything but `off` makes `rqx check` and `rqx comment` compare against the base revision, which `--regressions` does for one run. See [ci.md](ci.md#complexity-regressions).

## Fixes

`rqx fix` applies a mechanical fix only for a finding at or above `fix.min_confidence`. The value is `verified` (the default), `heuristic` or `low`. `low` applies every mechanical fix, which is what rqx 0.5.0 did. The preview reports the fixes the floor left out on a `Floor` row, and the JSON summary carries them as `below_floor`.

```yaml
fix:
  min_confidence: heuristic
```

Before anything is written, `rqx fix` compares what each edited file declares before and after the edit. An edit is withheld when it removes, or changes the signature of, a name that another file still uses. The files searched are every file the scan read, and every file of the same language in the edited file's directory, which covers a Go file that a build tag or a `_windows.go` suffix excludes from the current build. A Go edit that changes any function signature is withheld, because every file in a Go package can call every function in it. A comment block that carries licence or copyright text, and any comment that holds a URL, is never removed.

`rqx fix --verify-build` also runs `go vet ./...` on a temporary copy of each Go module that has an edit, before and after the edits are applied. When the module passes before and fails after, every edit in it is withheld. When it already fails before, its edits are withheld as well, because the check cannot judge them. The flag needs `go` on `PATH` and is off by default.

## Performance cost settings

`performance.receivers` and `performance.methods` add names to the vocabulary of the I/O rules. Two further keys belong to the complexity analysis. They are validated when the config loads.

```yaml
performance:
  max_order: O(n log n)
  costs:
    - {id: acme.contains, receiver: [AcmeQueue], method: contains, cost: n}
    - {id: acme.merge, fn: acme::merge, size_of: arg0.receiver, cost: n*m}
```

`max_order` is the largest time order a reviewed function may have, and a function above it fails `rqx check`. It is written as `O(1)`, `O(log n)`, `O(n)`, `O(n log n)`, `O(n^2)`, `O(n^3)`, `O(n*m)` or `exp`, and an unset value means no limit. `costs` lists library calls the built-in cost tables do not know. An entry names the call with `receiver` and `method`, or with `fn`, and gives its `cost` in the same notation, where `n` is the size of the receiver and `m` the size of the first argument. `space`, `size_of`, `when` and `pure` are optional. `id` must be unique, and a value that is not an order is an error with its line and column.

## Design pattern advice

`patterns` decides which of the twenty-three Gang of Four patterns rqx reports on. It can be addressed four ways: by advisory, when one claim does not apply to the project; by pattern, when the whole pattern does not; by family (`creational`, `structural`, `behavioural`); and by stance.

```yaml
patterns:
  # only report patterns that are here and broken, not ones that would fit
  stances: [incomplete]
  disabled: [strategy-fits]
```

Pattern advice does not change the health score. It reaches the exit code only through `gate.fail_on_incomplete`, which considers half-applied patterns. A pattern that would fit is not a defect. `rqx patterns --coverage` lists every pattern, what decides it, and the reason where rqx declines to report.

## Thresholds

Every threshold is the highest acceptable value: a finding fires strictly above it. Raising one makes its rules quieter. The table lists the rules each key affects.

| Key | Default | Tunes |
|---|---|---|
| `long_function_statements` | 40 | `long-function` |
| `cognitive_complexity` | 15 | `cognitive-complexity` |
| `nesting_depth` | 3 | `deep-nesting` |
| `max_params` | 4 | `long-parameter-list` |
| `class_members` | 20 | `large-class` |
| `class_statements` | 200 | `large-class` |
| `clone_min_statements` | 3 | `duplicate-code`, `reimplemented-existing` |
| `clone_min_nodes` | 12 | `duplicate-code`, `reimplemented-existing` |
| `condition_operands` | 3 | `complex-condition` |
| `message_chain_depth` | 3 | `message-chain` |
| `commented_code_lines` | 2 | `commented-out-code` |
| `magic_number_repeats` | 3 | `magic-number` |
| `class_public_members` | 20 | `wide-surface` |
| `fan_out` | 20 | `wide-fan-out` |
| `fan_in` | 20 | `wide-fan-in` |
| `relative_climb` | 2 | `deep-relative-import` |
| `module_statements` | 400 | `oversized-module` |
| `function_locals` | 15 | `many-locals` |
| `try_statements` | 10 | `wide-try` |
| `narrated_run` | 4 | `comment-per-line` |
| `explanation_ratio` | 2 | `explanation-longer-than-code` |
| `guard_cascade` | 3 | `guard-cascade` |
| `sole_author_commits` | 6 | `single-author-file` |
| `hotspot_commits` | 12 | `hotspot` |
| `cyclomatic_complexity` | 12 | `cyclomatic-complexity` |
| `max_results` | 3 | `many-results` |
| `class_collaborators` | 8 | `many-collaborators` |
| `log_percent` | 40 | `over-logging` |
| `inheritance_depth` | 3 | `deep-inheritance` |
| `todo_age_days` | 180 | `stale-todo` |
| `max_exits` | 4 | `many-exits` |
| `max_cases` | 12 | `wide-switch` |
| `min_cases` | 3 | `tiny-switch` |
| `callback_depth` | 3 | `callback-nesting` |
| `feature_envy_ratio` | 3 | `feature-envy` |
| `data_clump_size` | 3 | `data-clump` |
| `data_clump_sites` | 3 | `data-clump` |
| `short_name_scope_lines` | 25 | `short-name-wide-scope` |
| `slow_test_ms` | 100 | `slow-test` |
| `test_decisions` | 2 | `branching-test` |
| `boundary_repeats` | 3 | `boundary-arithmetic` |
| `declaration_distance_lines` | 20 | `distant-declaration` |
| `expression_operations` | 6 | `obscured-expression` |
| `configuration_depth` | 3 | `buried-configuration` |
| `loop_nesting` | 2 | `deep-loop-nesting` |
