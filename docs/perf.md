# Where the time goes

`rqx scan` says what is wrong. `rqx complexity` says how hard a function is to read.
Neither answers the question asked when a service is slow: which body does work that
grows faster than the data it runs on, and does anything actually reach it.

```sh
rqx perf
```

```
  Bottlenecks  8 functions worth a look

                   order  loops   io  reach   sampled  where
    reconcileBatch  O(n²)      2    1   ×3.4    41.2%  orders/reconcile.ts:88
                  `db.order.findMany` runs on every pass of the loop. On the stack in
                  41.2% of samples.  · Fix: Batch the Work
    fibonacci         exp      0    0   ×1.1        –  math/series.ts:12
                  Calls itself 2 times per invocation with no memo in sight. A profile
                  never sampled this function.  · Fix: Memoize

    p50 0 · p90 1 · p99 2 loop depth · 14 over 2 · 9 with a round trip inside a loop
```

Every row is a function, ranked by what its findings weigh against how much of the
project depends on the file they are in — the same churn and importer count the
`Cost:` line already reports.

| Column | |
|---|---|
| `order` | `O(1)`, `O(n)`, `O(n²)`, `O(n³+)` or `exp`, from the loops around the innermost body and whether the function calls itself twice |
| `loops` | loop nesting depth, counting a `forEach`-style callback and each clause of a comprehension |
| `io` | round trips and waits inside a loop, which are the cost a user feels as latency |
| `reach` | churn times importers, capped — how expensive this file already is to change |
| `sampled` | what a profile saw, when one was read |

`order` is loop order, not a proof. Three nested loops over a list of three are still
three passes of nothing, which is exactly why the profile column exists.

## Asking a different question

```sh
rqx perf --sort io          # round trips first, whatever the loop order
rqx perf --sort reach       # what the rest of the project leans on hardest
rqx perf --sort sampled     # just the hot list, when a profile was read
rqx perf --min-depth 2      # skip anything that is not already nested
rqx perf --by dir --top 5   # the worst each directory holds
```

`--by file` and `--by dir` report the worst loop order of what they hold and the sum
of the round trips, because an average hides one terrible function behind twenty
ordinary ones.

## The rules behind it

Eight, in the `performance` family, each naming a shape a reader can check:

| rule | |
|---|---|
| `scan-in-loop` | a whole-collection search, sort or copy inside a loop |
| `deep-loop-nesting` | loops nested past the `loop_nesting` threshold |
| `query-in-loop` | a database, HTTP or queue call inside a loop |
| `await-in-loop` | a wait inside a loop body, where the passes could overlap |
| `invariant-in-loop` | a regular expression compiled from a literal on every pass |
| `accumulate-by-copy` | an accumulator rebuilt by copying itself each time round |
| `branching-recursion` | a function calling itself twice with no memo in sight |
| `blocking-call-in-async` | a synchronous file, process or sleep call in an async function |

Names are the weakest evidence in the tool, so the rules that lean on one are
`heuristic` and never `error`. A receiver that is already keyed — a `Set`, a `Map`, a
`dict`, anything named `index`, `lookup` or `cache` — is a lookup rather than a scan
or a round trip, and is not reported. `[performance] receivers` and
`[performance] methods` add the names a project uses for its own gateways.

## What the profile actually ran

A shape that costs is a claim about how work grows. Whether anything reaches it is
something only a run can say:

```sh
rqx perf --profile cpu.pprof
rqx scan . --profile run.cpuprofile
```

Three formats are recognised by content rather than by extension: a V8 or Chrome
`.cpuprofile`, a Go pprof, and collapsed stacks as py-spy, async-profiler and `perf`
fold them. Several files merge, and one that cannot be read is skipped with a warning
rather than failing the scan.

A performance finding in a function the profile never put on the stack keeps its
finding — the shape is still there, and the next release may reach it — but drops to
`low` confidence, which means it is shown and stops counting against the health
score. One that was sampled says so with the share:

```
  `db.order.findMany` runs on every pass of the loop. On the stack in 41.2% of samples.
```

Code outside every function, and frames naming a file rqx never scanned, are counted
in the footer rather than attributed to a guess:

```
    profile: 41062 samples, 318 matched, 12 frames unplaced
```

## JSON

`--json` gives one object per row — `name`, `at`, `order`, `loops`, `io`, `reach`,
`weight`, `sampled` and the `findings` under it, each with its rule, its evidence and
the technique that answers it. `functions` counts the rows a grouped view stands for.
The numbers are the ones `rqx scan` gates on, so a dashboard built on this and a gate
built on `loop_nesting` cannot disagree.
