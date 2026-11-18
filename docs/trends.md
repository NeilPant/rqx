# Health trends

A scan can append a snapshot of its result to a file. `rqx trend` reads the snapshots
back as a series, and the HTML report can draw them as a chart. The file is
newline-delimited JSON, so a CI cache can carry it from one run to the next and `git`
can diff it.

## Recording

```sh
rqx scan . --record
rqx check --base main --record history/rqx.ndjson
```

`--record [PATH]` on `rqx scan` and `rqx check` appends one line to `PATH` after the
scan, creating the file and its folder when they do not exist. Without a path it writes
to `.rqx/trend.ndjson`, relative to the working directory. rqx only appends to the
file and never rewrites a line. The report the run prints is the same with or without
the flag, and the exit code is not affected.

`rqx check --record` records the project's health and counts, not those of the reviewed
files, so snapshots from `scan` and `check` are comparable.

The session does not accept `--record`. A snapshot belongs to a run, and the session
holds one scan that it may refresh.

### The snapshot line

```json
{"kind":"snapshot","schema_version":1,"commit":"9ff4583c0e1d4a7b2c8e5f60a1b2c3d4e5f60718","time":"2026-11-17T15:43:25Z","rqx_version":"0.4.0","health":71,"grade":"fair","files":212,"counts":{"error":0,"warning":19,"info":42,"total":61},"rules":{"long-function":9,"unused-export":14},"owners":{"@web":58,"@api":83}}
```

| Key | What it holds |
|---|---|
| `kind` | `snapshot` |
| `schema_version` | `1`. It increases only if a key has to change meaning. A reader skips a line with a newer version than it understands |
| `commit` | `HEAD` of the scanned repository, or `null` when the directory is not a git repository |
| `time` | When the scan finished, in UTC |
| `rqx_version` | The rqx that wrote the line |
| `health`, `grade` | The project's health score and the grade that score maps to |
| `files` | Files scanned |
| `counts` | Findings by severity and in total, before any display cap |
| `rules` | Findings per rule. Rules the project wrote itself are counted together under `custom` |
| `owners` | Health per owner. Present only when the project has a CODEOWNERS file. See [ownership.md](ownership.md) |

A reader ignores keys it does not recognise, and a release can add keys. Lines that are
not snapshots, are not JSON, or carry a newer `schema_version` are skipped when the file
is read, so a file that another tool has also written to still reads.

## Reading the series

```sh
rqx trend [PATH] [--last N] [--by rule|owner] [--reporter terminal|json]
```

`PATH` defaults to `.rqx/trend.ndjson`. The command does not scan. A missing file is an
error that names `--record`.

```
  Trend  3 snapshots · 2026-11-01 to 2026-11-15

    health       ▁▅█     50 → 71  +21 improving
    findings     █▄▁     12 → 4    -8 improving

  Biggest movers by rule
    long-function  █▄▁     12 → 4    -8 improving
```

Each row has:

- a sparkline, one block character per snapshot, scaled between that series' own lowest
  and highest value;
- the first and last value;
- the change between them;
- a direction. Health rising and a count falling are both `improving`. The opposite
  movements are `worsening`, and no net change is `steady`.

The first two rows are the health score and the total number of findings. The movers
below them are the ten rules, or with `--by owner` the ten owners, whose value changed
most between the first and last snapshot, largest change first. Series that did not
change are left out. A rule missing from a snapshot counts as zero there. An owner
missing from a snapshot has no point there, so its series starts at its first
appearance.

`--last N` reads only the newest `N` snapshots. With fewer than two snapshots the
command prints how many it found and says a direction needs two.

### JSON

`--reporter json` (or `--json`) writes one object per line:

```json
{"kind":"series","name":"health","points":[50,62,71],"first":50,"last":71,"change":21,"direction":"improving"}
{"kind":"mover","by":"rule","name":"long-function","points":[12,7,4],"first":12,"last":4,"change":-8,"direction":"improving"}
{"kind":"summary","schema_version":1,"snapshots":3,"from":"2026-11-01T10:00:00Z","to":"2026-11-15T10:00:00Z","by":"rule"}
```

`/trend` in the interactive session takes the same arguments and prints the same
output.

## The chart in the HTML report

> Deprecated: the `html` reporter and this chart will be removed in 0.6.0.
> `rqx trend` prints the same series in the terminal.

```sh
rqx scan . --record --trend .rqx/trend.ndjson --reporter html=rqx.html
```

`--trend PATH` on `rqx scan` and `rqx check` adds a Trend section after the summary
when the reporter is `html`. It is an inline SVG line of the health score, 0 to 100,
with the snapshots evenly spaced and the first and last dates under the axis. It uses
the page's own theme colours, has a text description for screen readers and uses no
script.

When a run passes both flags, the snapshot is recorded before the file is read, so the
chart includes the run that drew it. A file that does not exist, or that holds fewer
than two snapshots, produces a note that a chart needs two snapshots. Without
`--trend` the page is unchanged. The session does not accept the flag.

## Keeping the file between CI runs

[ci.md](ci.md#keeping-a-health-trend) has a recipe for GitHub Actions and GitLab CI.
