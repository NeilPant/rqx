# Hotspots

`rqx hotspots` lists the files where a problem costs the most: those that change
often and have findings or complex functions. A problem in a file nobody edits is paid
for rarely. The same problem in a file edited every week is paid for on each change.

```sh
rqx hotspots
rqx hotspots --top 25 --since 180
rqx hotspots --reporter json
```

```
  Hotspots  commits in the last 90 days multiplied by the weight of what is wrong

   1. src/orders/reconcile.ts  420
      14 commits in 90 days, 6 findings (weight 22), cognitive complexity 80, mostly deep-nesting

   2. src/orders/ledger.ts  96
      8 commits in 90 days, 2 findings (weight 6), cognitive complexity 60, mostly long-function
```

## The score

```
score = commits × (finding weight + cognitive complexity / 10)
```

| Term | Source |
|---|---|
| `commits` | Commits that touched the file in the churn window. |
| `finding weight` | The sum of severity weights of the file's findings that count toward the health score: 1 for info, 3 for warning, 9 for error. Findings at `low` confidence and findings read with `--sarif-in` are not counted, as in the score. |
| `cognitive complexity` | The sum over the file's functions, so a file of many moderately complex functions ranks with one that has a single very complex function. |

A file with no commits in the window, or with no weight, is not listed.

## Options

| Option | Effect |
|---|---|
| `--top N` | The number of files listed. Default 10. |
| `--since DAYS` | The number of days of git history to count commits over. Default `scan.churn_days`, which is 90. `0` reads no history, and the command says so. |
| `--reporter terminal\|json` | `json` writes one object per file with `file`, `score`, `commits`, `findings`, `finding_weight`, `cognitive_complexity`, `main_rule` and `explanation`. |

`/hotspots` in the interactive session runs the same command. It scans again, because
the session's scan was taken with the default churn window and a capped findings list.

The `hotspot` rule reports a file when its commits reach `thresholds.hotspot_commits`
and it has any finding. `rqx hotspots` ranks files that are below that threshold too.
