# Presets

A preset is a named set of settings a config can build on. Name one under `extends`
and it is merged under the file's own keys, so it moves the defaults and never
overrides a value you wrote.

```yaml
# rqx.yaml
extends: [legacy-adoption, ci-gate]

thresholds:
  max_params: 5
```

`extends` takes preset names and relative paths to other YAML files, and later entries
win over earlier ones. A path is relative to the file that names it. Files that extend
each other in a circle are an error, and so is a name that is neither a preset nor a
file: rqx suggests the closest preset and lists the rest.

## The presets

| Preset | What it sets |
|---|---|
| `recommended` | Nothing. It is the defaults rqx ships, named so a file can say so. |
| `strict` | The strict level: thresholds close to what a well-factored module already meets, four naming rules on, and a gate that fails on a warning. |
| `legacy-adoption` | The relaxed level's thresholds and rules, a gate that fails only on errors, and the ratchet on. Pair it with `rqx baseline` and `gate.baseline`; until a baseline exists the ratchet does nothing, so extending it never fails a run on its own. |
| `ci-gate` | A gate for a pipeline: no errors, a health score of at least 70, and no single file graded `critical`. Thresholds stay at their defaults, so it changes what fails the build and not what is reported. |
| `perf` | Nested loops and nested callbacks are reported one level earlier than the defaults allow. |

The level names `relaxed`, `standard` and `pristine` also work under `extends`, since
`strict` already does. `rqx levels` prints what each one changes.

## Precedence

From lowest to highest:

1. presets and extended files
2. the user config
3. the policy a project points at
4. `rqx.yaml` files from the scanned directory upward, outermost first
5. `RQX_CONFIG`
6. `--config`
7. command-line flags

Everything a file extends goes under every file rqx reads, not just under the file
that named it. A preset therefore cannot override a policy, and it cannot move a key
the policy locks. A subtree's `overrides` are applied last, to the files they match.

## Seeing where a value came from

`rqx config` lists the presets a run was built on and puts the source of each value in
the `from` column, as `preset:ci-gate`, as `base.yaml (extends)` for a file pulled in
by path, or as the name of the file that wrote it. `rqx config set` never writes to a
preset or to an extended file; with no `--file` it writes to the nearest project config.
