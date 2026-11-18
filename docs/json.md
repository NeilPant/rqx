# The JSON stream

`--reporter json` writes newline-delimited JSON: one object per line. `--json` is the
short form of it on the commands that have always had the flag. This page describes the
parts of the format that are stable, for code that parses it.

## The promise

- A key is not removed or renamed. A release can add keys; it does not remove one or
  change what a value means.
- Every line states its type in `kind`. A consumer should branch on `kind` and skip
  any `kind` it does not know, because a later release may add lines of a new kind.
- A consumer should ignore keys it does not recognise, so that the format can gain
  keys without breaking it.
- The summary line carries `schema_version`, currently `1`. It increases only if a
  key has to change meaning, so a consumer can refuse a document newer than it
  understands.

## `rqx scan --reporter json`

One line per finding, then one summary line.

A finding line has `"kind": "finding"` and the finding's own fields: `rule`,
`category`, `severity`, `confidence`, `location`, `evidence`, `why`, `fixes`, `metric`,
`fix_hint` and `related`. A finding also has `focus` when its rule narrowed the problem
down: the range inside `location` that a reader should look at, in the same shape as
`location`. A finding without the key has no narrower range than `location`.

The summary line has `"kind": "summary"` and:

| Key | What it holds |
|---|---|
| `schema_version` | The version of this format, `1` |
| `rqx_version` | The rqx that wrote the document |
| `health_score` | The project's score out of 100 |
| `files_scanned`, `code_lines`, `elapsed_ms` | The size and cost of the run |
| `counts` | Findings by severity and by rule, before any display cap |
| `omitted` | Findings dropped by `--max-findings` |
| `suppressed` | Findings a waiver kept out of `counts` |
| `skipped` | Files rqx did not analyse, each as `"path: reason"` |
| `skipped_by` | The skipped files counted per reason, as `{ "<reason>": { "files": n } }`. The `submodule` entry also has `paths`, the skipped submodules relative to the scanned directory. The reasons are `submodule`, `minified`, `generated`, `gitattributes`, `too-large`, `unreadable`, `parse-timeout`, `unsupported` and `walk`. A reason with no skipped files is left out |
| `grades` | A letter grade per file in scope |
| `gate` | Present when a verdict exists: `{ "passed": bool, "failures": [string] }` |
| `review` | Present for `rqx check`: the base, the reviewed counts and the verdict |
| `resolved` | Present for `rqx check` and `rqx comment` against a revision: the findings the base had that the change removed, each as `{ "rule", "subject", "file", "line" }` |
| `resolved_scope` | Present with `resolved`: `project` when the base was scanned whole, `changed-files` when the credit covers only the findings one file's analysis produces in the files the change touched |
| `health_delta` | Present when `resolved_scope` is `project`: the health score after the change minus the score at the base |
| `owners` | Present when the project has a CODEOWNERS file: per owner, `files`, `findings`, `health` and `grade`. See [ownership.md](ownership.md) |
| `regressions` | Present for `rqx check` when `gate.complexity_regression` is not `off` or `--regressions` was passed: the functions whose time order grew against the base revision, each as `{ "file", "qual", "line", "before", "after", "trace" }`. `qual` is the function's name qualified by its class, `before` and `after` are orders such as `O(n)` and `O(n²)`, and `trace` is a list of `{ "file", "line", "what", "order" }` steps from the dominant loop or call down to the cost. Empty when `regressions_skipped` is present. See [ci.md](ci.md#complexity-regressions) |
| `regressions_skipped` | Present with `regressions` when no comparison could be made, with the reason as a string |

`gate` is present for a scan only when the run was gated by a flag (`--fail-on`,
`--allow-findings`, `--min-health`, `--min-grade` or `--fail-on-incomplete`), and
always for `check` and `comment`. `passed` matches the exit code, so a pipeline that
cannot see the exit code can read the verdict here. `review` predates `gate` and is
kept unchanged for consumers written against it.

## The other commands

When the project has a CODEOWNERS file, each finding line also has an `owners` array,
empty for a file no rule names.

`rqx owners --reporter json` writes one object per owner with the keys `owner`,
`files`, `findings`, `health` and `grade`.

`rqx scan --record` appends a `"kind": "snapshot"` line to a trend file, and
`rqx trend --reporter json` writes `series`, `mover` and `summary` lines. Both are
described in [trends.md](trends.md).

`fix`, `verify` and `plan` end with a `"kind": "summary"` line that also carries
`schema_version`. Their other lines have their own kinds: `fix` and `withheld` for
`fix`, `change` for `verify`, `item` for `plan`.

`verify` and `plan` write camelCase keys (`healthBefore`, `healthAfter`), while the
scan summary writes snake_case. Both are kept as they are, because renaming either
would break existing consumers.

`rqx config --json` writes one `"kind": "setting"` line per setting, then a
`"kind": "summary"` line:

```json
{"kind":"setting","key":"thresholds.max_params","value":6,"from":"rqx.yaml","default":false}
{"kind":"summary","schema_version":1,"files":["rqx.yaml"]}
```

`key` is the setting as written in a configuration file. `value` is the resolved
value, with the type the configuration gives it (a number, a string, a list). `from`
is what the `from` column of the table prints (`default`, a file name,
`preset:NAME`, `level:NAME`). `default` is `true` when nothing set the key. A row
that summarises a table, such as how many rules run, has no single value, and its
`value` is the table's text as a string. `files` lists the configuration files that
were read, nearest last.

`rqx config validate --json` writes one `"kind": "problem"` line per problem, then a
summary:

```json
{"kind":"problem","severity":"warning","file":"rqx.yaml","line":2,"column":14,"message":"`nonsense` in `rules.disabled` names nothing; …"}
{"kind":"summary","schema_version":1,"valid":true,"errors":0,"warnings":1,"files":["rqx.yaml"]}
```

`severity` is `error` or `warning`. `file`, `line` and `column` are `null` when a
problem has no location in a file. `valid` is `true` when the command exits 0, so
under `--strict` a warning makes it `false`.

The commands that print a table as data (`complexity`, `perf`, `health`, `slop`, `hotspots`,
`recommend`, `waivers`, `patterns`) write one object per row and no summary. Their
keys follow the same rules.

In a `perf` row, `order` is the syntactic loop order, read from loop nesting, and `loops`
is the nesting depth. Both keep their meaning. From 0.5.0 a row also has `time`,
`space`, `io_order`, `alloc`, `confidence` and `cost`, which come from the cost
analysis that follows calls across files. `io` is still the count of round trips found
inside loops. [perf.md](perf.md#json) lists the keys.
