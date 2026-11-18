# Presets

A preset is a named set of settings a config can build on. Name one under `extends`
and it is merged beneath the file's own keys. A preset changes the defaults and does
not override a value written in the file.

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
| `recommended` | Nothing. It is the defaults rqx ships, named so that a file can state them explicitly. |
| `strict` | The strict level: thresholds close to what a well-factored module already meets, four naming rules on, and a gate that fails on a warning. |
| `legacy-adoption` | The relaxed level's thresholds and rules, a gate that fails only on errors, and the ratchet on. Use it with `rqx baseline` and `gate.baseline`. The ratchet has no effect until a baseline exists, so extending the preset does not fail a run by itself. |
| `ci-gate` | A gate for a pipeline: no errors, a health score of at least 70, and no single file graded `critical`. Thresholds stay at their defaults, so the preset changes what fails the build and leaves what is reported unchanged. |
| `perf` | Every performance rule is reported as an error, so the default gate fails on one, and nested loops and nested callbacks are reported one level earlier than the defaults allow. The performance rules that ship off (`allocation-in-loop`, `missing-capacity` and `pair-scan-could-hash`) are switched on. |

The level names `relaxed`, `standard` and `pristine` can also be used under
`extends`, as `strict` can. `rqx levels` prints what each one changes.

## Precedence

From lowest to highest:

1. presets and extended files
2. the user config
3. the policy a project points at
4. `rqx.yaml` files from the scanned directory upward, outermost first
5. `RQX_CONFIG`
6. `--config`
7. command-line flags

Everything a file extends is applied beneath every file rqx reads, not only beneath
the file that named it. A preset therefore cannot override a policy or change a key
the policy locks. A subtree's `overrides` are applied last, to the files they match.

## Seeing where a value came from

`rqx config` lists the presets a run was built on and shows the source of each value
in the `from` column: `preset:ci-gate`, `base.yaml (extends)` for a file included by
path, or the name of the file that set the value. `rqx config set` does not write to
a preset or to an extended file. Without `--file`, it writes to the nearest project
configuration.
