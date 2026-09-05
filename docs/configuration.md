# Configuring rqx

Everything lives in `RQX.toml`. `rqx init` writes one with every setting at its default and a line of prose per table; `rqx config` prints what a run will actually use and which file each value came from.

## Where rqx looks

Layers are merged in this order, each one overriding the one above it. A table merges key by key, so a file can set one threshold without restating the other twenty-one; a list replaces rather than appends, so a narrower layer can shorten one.

| | Source |
|---|---|
| 1 | the built-in defaults |
| 2 | `$XDG_CONFIG_HOME/rqx/RQX.toml`, else `~/.config/rqx/RQX.toml` (`%APPDATA%\\rqx\\RQX.toml` on Windows) |
| 3 | every `RQX.toml` from the scanned directory upward, outermost first |
| 4 | `RQX_CONFIG`, if it is set |
| 5 | `--config <path>` |
| 6 | the command's own flags |

The upward search stops at a directory holding `.git`, or at a file that sets `root = true` — a config above a checkout describes a different project, and reading it would make a scan depend on where the repository happens to sit.

## A subtree with different standards

```toml
[[override]]
paths = ["src/generated/**", "vendor/**"]

[override.thresholds]
long_function_statements = 200

[override.rules]
disabled = ["waste"]
```

An override may set `thresholds` and `rules` and nothing else. Every other table describes the run as a whole — the file walker is built once and the gate reaches one verdict — so naming one in an override is an error rather than a setting that quietly does nothing.

The same holds for names: a rule, family, pattern or stance that does not exist is an error, not a silent no-op, because a `categories` allowlist naming only unknown families would otherwise turn every rule off. `rqx rules` lists every name that is valid.

## Levels

`level = "strict"` in `RQX.toml`, or `--level strict` on the command line, picks a named set of defaults. A level is merged *under* everything you wrote, so it moves the defaults and never overrides a value you stated; `rqx config` shows `level:strict` in the `from` column for a value the level moved. An `[[override]]` may name its own `level`, so a project can be strict in its domain and relaxed in its scripts. `rqx levels` prints each one as the settings it would take to write by hand.

## The gate

`rqx check` always gates. **`rqx scan` gates only when you pass one of `--fail-on`, `--allow-findings`, `--min-health` or `--min-grade`**, so a `[gate]` table in `RQX.toml` does nothing for a bare scan. That is deliberate: a scan that started failing because a config file appeared above it would change the exit code of pipelines that never asked for a gate.

## Design pattern advice

`[patterns]` decides which of the twenty-three Gang of Four patterns rqx reports on. It is addressable four ways, because the useful question differs: by advisory when one claim is wrong for this project, by pattern when the whole pattern is, by family (`creational`, `structural`, `behavioural`), and by stance.

```toml
[patterns]
# only report patterns that are here and broken, not ones that would fit
stances = ["incomplete"]
disabled = ["strategy-fits"]
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
