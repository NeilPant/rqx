# The JSON stream

`--reporter json` and the `--json` flag on the other commands write newline-delimited
JSON: one object per line. This page is what you can rely on when you parse it.

## The promise

- A key is never removed and never renamed. A release can add keys; it cannot take one
  away or change what a value means.
- Every line says what it is in `kind`. Switch on it, and skip a `kind` you do not
  know: a later release may add lines of a new kind.
- Ignore keys you do not recognise. That is how a format grows without breaking you.
- The summary line carries `schema_version`. It is `1` today and goes up only if a
  key ever has to change meaning, so a consumer can refuse a document newer than it
  understands.

## `rqx scan --reporter json`

One line per finding, then one summary line.

A finding line has `"kind": "finding"` and the finding's own fields: `rule`,
`category`, `severity`, `confidence`, `location`, `evidence`, `why`, `fixes`, `metric`,
`fix_hint` and `related`.

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
| `skipped` | Files rqx could not read, with the reason |
| `grades` | A letter grade per file in scope |
| `gate` | Present when a verdict exists: `{ "passed": bool, "failures": [string] }` |
| `review` | Present for `rqx check`: the base, the reviewed counts and the verdict |

`gate` is there for a scan only when the run was gated by a flag (`--fail-on`,
`--allow-findings`, `--min-health`, `--min-grade` or `--fail-on-incomplete`), and for
`check` and `comment` always. `passed` is the answer the exit code gives, so a pipeline that cannot see the
exit code can read it here. `review` is older and stays as it is, so a consumer
written against it keeps working.

## The other commands

`fix`, `verify` and `plan` end with a `"kind": "summary"` line that also carries
`schema_version`. Their other lines have their own kinds: `fix` and `withheld` for
`fix`, `change` for `verify`, `item` for `plan`.

`verify` and `plan` write camelCase keys (`healthBefore`, `healthAfter`) where the
scan summary writes snake_case. That mix is kept: renaming either would break the
consumers already reading it.

`rqx config --json` writes one `"kind": "setting"` line per setting, then a
`"kind": "summary"` line:

```json
{"kind":"setting","key":"thresholds.max_params","value":6,"from":"rqx.yaml","default":false}
{"kind":"summary","schema_version":1,"files":["rqx.yaml"]}
```

`key` is the setting as it is written in a config file, `value` is what it resolves
to as the type the config gives it (a number, a string, a list), `from` is what the
table's `from` column prints (`default`, a file name, `preset:NAME`, `level:NAME`),
and `default` is whether nothing set it. A row that summarises a table, such as how
many rules run, has no single value and carries the table's own words as a string.
`files` lists the config files that were read, nearest last.

`rqx config validate --json` writes one `"kind": "problem"` line per problem, then a
summary:

```json
{"kind":"problem","severity":"warning","file":"rqx.yaml","line":2,"column":14,"message":"`nonsense` in `rules.disabled` names nothing; …"}
{"kind":"summary","schema_version":1,"valid":true,"errors":0,"warnings":1,"files":["rqx.yaml"]}
```

`severity` is `error` or `warning`. `file`, `line` and `column` are `null` when a
problem has no place in a file. `valid` is `true` when the command exits 0, so under
`--strict` a warning makes it `false`.

The commands that print a table as data (`complexity`, `perf`, `health`, `slop`,
`recommend`, `waivers`, `patterns`) write one object per row and no summary. Their
keys follow the same promise.
