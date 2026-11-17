# Configuring rqx

Everything lives in `rqx.yaml`. `rqx init` writes one with every setting at its default and a line of prose per table; `rqx config` prints what a run will actually use and which file each value came from.

## Where rqx looks

Layers are merged in this order, each one overriding the one above it. A table merges key by key, so a file can set one threshold without restating the other twenty-one; a list replaces rather than appends, so a narrower layer can shorten one.

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

Everything a file extends goes under *every* file rqx reads, not just under the file that named it, so a preset can never override a policy or a project's own key. [`docs/presets.md`](presets.md) lists the presets.

A directory may call its config `rqx.yaml`, `.rqx.yml`, `rqx.yml` or `.rqx.yaml`; holding two is an error. A `RQX.toml` is an error too: rqx no longer reads TOML, and skipping one could change what a pipeline gates on. `rqx config migrate` converts it.

The upward search stops at a directory holding `.git`, or at a file that sets `root: true` — a config above a checkout describes a different project, and reading it would make a scan depend on where the repository happens to sit.

**An `ignore.patterns` glob is matched against paths relative to the directory being scanned**, not to the file the pattern is written in. `rqx scan .` and `rqx scan src` therefore see different things from one `patterns: [src/**]`, and a pattern naming a directory whose name appears again deeper in the tree — `fixtures/**` against a `fixtures/go/fixtures/` — matches in both places. Name one level more when that matters: `fixtures/*/**`.

## A subtree with different standards

```yaml
overrides:
  - paths: [src/generated/**, vendor/**]
    thresholds:
      long_function_statements: 200
    rules:
      disabled: [waste]
```

An override may set `thresholds`, `rules`, `patterns` and `level` and nothing else. Every other table describes the run as a whole — the file walker is built once and the gate reaches one verdict — so naming one in an override is an error rather than a setting that quietly does nothing.

The same holds for names: a rule, family, pattern or stance that does not exist is an error, not a silent no-op, because a `categories` allowlist naming only unknown families would otherwise turn every rule off. `rqx rules` lists every name that is valid.

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

A key under `rules` that is not `disabled`, `enabled`, `categories` or a rule slug is an error that suggests the closest name, and so is an option the rule does not have. The same settings work inside an entry of `overrides`, which is how one subtree gets a stricter rule than the rest. A rule *you* wrote under `rule` carries its own `severity` and `confidence`, so it is not a key here.

## Levels

`level: strict` in `rqx.yaml`, or `--level strict` on the command line, picks a named set of defaults. A level is merged *under* everything you wrote, so it moves the defaults and never overrides a value you stated; `rqx config` shows `level:strict` in the `from` column for a value the level moved. An entry of `overrides` may name its own `level`, so a project can be strict in its domain and relaxed in its scripts. `rqx levels` prints each one as the settings it would take to write by hand.

## The gate

`rqx check` always gates. **`rqx scan` gates only when you pass one of `--fail-on`, `--allow-findings`, `--min-health` or `--min-grade`**, so a `gate` mapping in `rqx.yaml` does nothing for a bare scan. That is deliberate: a scan that started failing because a config file appeared above it would change the exit code of pipelines that never asked for a gate.

## Design pattern advice

`patterns` decides which of the twenty-three Gang of Four patterns rqx reports on. It is addressable four ways, because the useful question differs: by advisory when one claim is wrong for this project, by pattern when the whole pattern is, by family (`creational`, `structural`, `behavioural`), and by stance.

```yaml
patterns:
  # only report patterns that are here and broken, not ones that would fit
  stances: [incomplete]
  disabled: [strategy-fits]
```

**None of it moves the health score and none of it decides the gate.** A pattern that would fit is not a defect, and a score that fell because code declined to use Strategy would stop meaning anything. `rqx patterns --coverage` lists every pattern, what decides it, and the reason where rqx declines to guess.

## Thresholds

Every threshold is the highest acceptable value: a finding fires strictly above it. Raising one makes its rules quieter, and this is which rules.

| Key | Default | Tunes |
|---|---|---|
| `long_function_statements` | 40 | `long-function` |
| `cognitive_complexity` | 15 | `cognitive-complexity` |
| `nesting_depth` | 3 | `deep-nesting` |
| `max_params` | 4 | `long-parameter-list` |
| `class_members` | 20 | `large-class` |
| `class_statements` | 200 | `large-class` |
| `clone_min_statements` | 3 | `duplicate-code` |
| `clone_min_nodes` | 12 | `duplicate-code` |
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
| `boundary_repeats` | 3 | `boundary-arithmetic` |
| `declaration_distance_lines` | 20 | `distant-declaration` |
| `expression_operations` | 6 | `obscured-expression` |
| `configuration_depth` | 3 | `buried-configuration` |
| `loop_nesting` | 2 | `deep-loop-nesting` |
