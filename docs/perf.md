# Where the time goes

`rqx scan` reports findings. `rqx complexity` reports how hard each function is to
read. `rqx perf` reports which function bodies do work that grows faster than the
data they process, and whether a profile shows that anything executes them.

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

Each row is a function. Rows are ranked by the weight of the function's findings
against how much of the project depends on the file they are in, using the same churn
and importer count as the `Cost:` line.

| Column | |
|---|---|
| `order` | `O(1)`, `O(n)`, `O(n²)`, `O(n³+)` or `exp`, from the loops around the innermost body and whether the function calls itself twice |
| `loops` | loop nesting depth, counting a `forEach`-style callback and each clause of a comprehension |
| `io` | round trips and waits inside a loop, which are the cost a user feels as latency |
| `reach` | churn times importers, capped: how costly the file already is to change |
| `sampled` | the share of profile samples that had the function on the stack, when a profile was read |

`order` is derived from loop structure and does not prove the cost. Three nested loops
over a list of three elements perform little work. The `sampled` column shows whether
a profile reached the function.

## Asking a different question

```sh
rqx perf --sort io          # round trips first, whatever the loop order
rqx perf --sort reach       # what the rest of the project leans on hardest
rqx perf --sort sampled     # just the hot list, when a profile was read
rqx perf --min-depth 2      # skip anything that is not already nested
rqx perf --by dir --top 5   # the worst each directory holds
```

`--by file` and `--by dir` report the worst loop order among the functions they group
and the sum of their round trips. An average would hide one expensive function among
many ordinary ones.

## The rules behind it

Eight rules in the `performance` family each report a code shape that can be checked
by reading:

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

Names are weak evidence, so the rules that depend on one are `heuristic` and are not
reported as `error`. A receiver that is already keyed (a `Set`, a `Map`, a `dict`, or
anything named `index`, `lookup` or `cache`) is treated as a lookup, not as a scan or
a round trip, and is not reported. `performance.receivers` and `performance.methods`
in `rqx.yaml` add the names a project uses for its own gateways.

## What the profile actually ran

The rules above describe how work grows. Whether any code path executes the function
is shown only by a run, which a profile records:

```sh
rqx perf --profile cpu.pprof
rqx scan . --profile run.cpuprofile
```

Five formats are recognised by their content, not by file extension:

- a V8 or Chrome `.cpuprofile`;
- a Go pprof;
- a speedscope JSON file, either `sampled` profiles (stacks with weights) or `evented`
  profiles (open and close events, folded into stacks weighted by the time each was
  open). A file is a speedscope file when its `$schema` names speedscope or it has a
  `shared.frames` array;
- the text that `jfr print --events jdk.ExecutionSample recording.jfr` writes. Each
  event is one sample, and a frame's class gives its file, so `com.acme.Billing` is
  matched as `com/acme/Billing.java` under any source root;
- collapsed stacks as py-spy, async-profiler and `perf` produce them. async-profiler
  frames in `com/acme/Billing.method` or `com.acme.Billing.method` form are matched to
  the same file, and to the line when the profile was recorded with line numbers.

A binary `.jfr` recording is not read. Convert it with
`jfr print --events jdk.ExecutionSample` and pass the text. rqx reports this on
standard error and skips the file.

Several profile files are merged. A file that cannot be read, including a truncated
one, is skipped with a warning, and the scan continues.

A performance finding in a function that the profile never put on the stack is kept,
but its confidence drops to `low`. A `low` confidence finding is shown and does not
count against the health score. A finding in a sampled function states the share of
samples:

```
  `db.order.findMany` runs on every pass of the loop. On the stack in 41.2% of samples.
```

Samples in code outside every function, and frames that name a file rqx did not scan,
are counted in the footer and are not attributed to any function:

```
    profile: 41062 samples, 318 matched, 12 frames unplaced
```

## Regressions in review

The same orders are compared between the base revision and the change by `rqx check` and
`rqx comment`. A function that went from `O(n)` to `O(n²)`, including one that only calls
something that did, is reported, and `gate.complexity_regression` can fail the run on it.
`performance.max_order` fails a review that holds a function above a limit. See
[Complexity regressions](ci.md#complexity-regressions).

## JSON

`--json` writes one object per row with the keys `name`, `at`, `order`, `loops`, `io`,
`reach`, `weight`, `sampled` and `findings`. Each entry of `findings` has its rule, its
evidence and the technique that addresses it. `functions` is the number of functions
that a row of a grouped view stands for. The numbers are the ones `rqx scan` gates on,
so output built from this command and a gate built on `loop_nesting` use the same
values.
