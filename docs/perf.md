# Where the time goes

`rqx scan` reports findings. `rqx complexity` reports how hard each function is to
read. `rqx perf` reports which function bodies do work that grows faster than the
data they process, what that work costs once the functions they call are counted, and
whether a profile shows that anything executes them.

Big-O cannot be decided from source, so every order `rqx perf` prints is an estimate.
The analysis reads loops, the types of the receivers it can find, and the calls it can
follow, and each row carries a confidence and, with `--explain`, the evidence behind
it. On the precision corpus the time and space orders were exact for 0.91 of the 437
functions written for it and for 0.48 of the 31 functions copied from five open source
projects (vite, flask, cobra, gson and fmt). The real sample is small, and its per-language
figures run from 0.20 to 0.80. Treat an order as something to check, and use
`--profile` to see whether anything runs the function. The figures are in
[perf-precision.md](perf-precision.md).

```sh
rqx perf
```

```
  Bottlenecks  4 functions worth a look

                   order   time space   io alloc      conf loops trips reach  where
    reconcileBatch  O(n) O(n·m)  O(n) O(n)     2  verified     1     2  ×1.0  reconcile.ts:3
                   `db.order.findMany` runs on every pass of the loop.  · Fix: Batch the Work
    fibonacci        exp    exp  O(n) O(1)     –  verified     0     0  ×1.0  reconcile.ts:12
                   Calls itself 2 times per invocation with no memo in sight.  · Fix: Memoize
    dedupe          O(n)  O(n²)  O(n) O(1)     –  verified     1     0  ×1.0  reconcile.ts:17
                   `out.includes` runs on every pass of the loop.  · Fix: Replace Loop with Lookup
    drain           O(n) O(n·m)  O(m) O(1)     – heuristic     1     0  ×1.0  stream.ts:1
                   `wanted.indexOf` runs on every pass of the loop.  · Fix: Replace Loop with Lookup

    p50 1 · p90 1 · p99 1 loop depth · 0 over 2 · 2 with a round trip inside a loop
```

Each row is a function. Rows are ranked by the weight of the function's findings
against how much of the project depends on the file they are in, using the same churn
and importer count as the `Cost:` line, and by the order of its time.

| Column | |
|---|---|
| `order` | the syntactic loop order: `O(1)`, `O(n)`, `O(n²)`, `O(n³+)` or `exp`, from the loop nesting of the body and whether the function calls itself twice |
| `time` | the order of the work the function does, with the cost of the functions it calls included |
| `space` | the order of the memory the function holds at its peak, including what its callees hold and, for a recursion, the stack |
| `io` | the order of the calls that leave the process (queries, requests, file and queue operations) the function makes, directly or through its callees |
| `alloc` | allocations the body makes on each pass of its innermost loop, shown as `–` when there are none |
| `conf` | how sure the analysis is of the orders: `verified`, `heuristic` or `unknown` |
| `loops` | loop nesting depth, counting a `forEach`-style callback and each clause of a comprehension |
| `trips` | round trips and waits found inside a loop by the performance rules, which are the cost a user feels as latency |
| `reach` | churn times importers, capped: how costly the file already is to change |
| `sampled` | the share of profile samples that had the function on the stack, when a profile was read |

`order` and `loops` come from the shape of the loops and are kept as they were. Three
nested loops over a list of three elements show `O(n³+)` there. `time` reads what the
loops run over, so the same function shows `O(1)`. A function that the analysis found
nothing to say about has `O(1)` in `time`, `space` and `io`.

Sizes are named in the table by letter, in the order they first appear in the
function: `n`, then `m`, then `k`. `--explain` and the JSON output say which parameter
or value each letter stands for.

## How the orders are read

The analysis reads source only and does not run it. It works on one function at a time
and then composes the functions across calls and files.

**Loops.** Each loop has a bound, which is one of:

- a size: the loop runs over a collection, a parameter, or a count taken from
  `len(x)`, `x.length` or `x.size()`, or from an integer parameter. A `for-each` over
  `x.keys()`, `x.values()`, `x.items()` or `Object.keys(x)` is bounded by the size of
  `x`. Callbacks passed to `map`, `filter`, `forEach` and the like, and each clause of
  a comprehension, run over the size of the receiver. A collection made from another
  by `filter`, `slice` or a copy shares its size;
- a constant: a literal, a constant name, an upper-case name assigned a literal, or
  `range(3)`. A loop over a constant multiplies the cost by one;
- a logarithm: a counter that is multiplied, divided or shifted on each pass
  (`i *= 2`, `n /= 2`, `n >>= 1`), and the binary search shape where `lo` and `hi`
  move to `mid`;
- not readable: anything else. The loop is given a size of its own, and the function's
  confidence is `heuristic`.

An early exit does not lower a bound, because the analysis cannot tell how early it is
taken.

**Library calls.** A call into the standard library or a common package has a cost
that depends on the receiver: `includes` on an array is linear, on a set it is
constant. Each language has a table of such calls in `crates/rqx-lang/data/cost/`,
each entry with an id such as `js.array.includes`, a time, a space, and whether the
call is pure. The receiver is typed from a declaration (`List<String> xs`,
`[]T`, `map[K]V`, `std::vector<int>`, a TypeScript annotation, a Python type hint),
from the initializer (`new Set()`, `set()`, `make(map[string]int)`, `[]`), or from the
class fields. A cost read from a declared or constructed type is `verified`. When
there is no type, a cost can be read from the method name alone if one table entry
applies, and then the confidence is `heuristic`.

`performance.costs` in `rqx.yaml` adds entries for the calls a project makes on its own
types. See [configuration](configuration.md#performance-cost-settings).

**Calls.** A call to a function the scan has read costs what that function costs, with
the callee's parameters replaced by the sizes of the arguments. A call with a constant
argument costs a constant. Calls are followed:

- by name within a file, and to a method of the same class;
- through imports into other files, including re-exports through up to three barrel
  files;
- to a method whose receiver has a known class, and otherwise to the methods of that
  name, taking the costliest, when there are at most four of them. More than four is
  too common a name to say anything, and the call counts as `O(1)`.

A call that reaches no function the scan has read counts as `O(1)` and does not lower
the confidence. A call that was matched by method name alone lowers it to
`heuristic`.

**Recursion.** A function that calls itself is solved from how its argument changes:

| shape | time |
|---|---|
| one call, argument one smaller (`n - 1`) | `n` times the work of one level |
| one call, argument one smaller and copied (`xs[1:]`) | `n` times the work of a level plus the copy |
| two or more calls, argument one smaller, no cache | `exp` |
| the same, with a cache read before recursing (`@lru_cache`, a `memo`, `seen` or `dp` map) | `n` times the work of one level |
| argument halved (`n / 2`, `n >> 1`, `mid`) | the master theorem: `log n`, `n` or `n log n` for the common shapes |
| an argument whose change is not read | `n` times the work of one level, as a guess, so `heuristic` |

A recursion that goes one level deeper per unit of input also needs that much stack,
which `space` includes. Functions that call one another in a circle are costed together,
as the sum of the members once per level of depth, and the confidence is `heuristic`
because the depth is not known.

## Confidence

| `conf` | meaning |
|---|---|
| `verified` | every bound was read and every library cost came from a typed receiver or a declared cost |
| `heuristic` | at least one part was read from a name, a guess at the callee, or an unreadable bound |
| `unknown` | reserved for a function the analysis cannot cost at all. The current analysis reports `heuristic` in those cases, and the precision corpus counts an `unknown` answer as an abstention |

A composed cost is only as sure as its least sure part. `rqx perf --explain` lists the
reasons.

How often the orders matched a hand-worked answer is measured on a corpus of several hundred
functions, some of them copied from real projects, and published per language and per rule
in [perf-precision.md](perf-precision.md). `rqx explain RULE` prints the same measured
precision for each performance rule the corpus scores.

## Explaining a function

```sh
rqx perf --explain reconcile
rqx perf --explain Billing.reconcile
rqx perf --explain src/orders/reconcile.ts:88
```

The argument is a function name, `Class.method`, or `file:line`. A name that several
functions share is an error that lists them with their locations; naming one by
`file:line` selects it. The output is the derivation of `time`, then of `space` and
`io` where the body accounts for them:

```
  reconcile  reconcile.ts:3
    time O(orders·prices)  space O(orders)  io O(1)  stack O(1)  confidence verified

  time O(orders·prices)
  └─ :5 loop is bounded by the size of `orders` (parameter)
     └─ :6 calls `hasPrice` (prices.ts:1) with `prices`, `order`  → O(prices)
        └─ :2 `js.array.includes` on `prices`  → O(prices)

  space O(orders)
  └─ :5 loop is bounded by the size of `orders` (parameter)

  confidence verified
    - every bound and library cost was read from the source
```

Each loop states where its bound came from. Each call states the function it reached,
the arguments passed and the callee's cost in the callee's own terms, with the callee's
loops beneath it down to three calls deep. Each library call names the table entry that
priced it. A recursion is described above the trees. The last block says why the
confidence is what it is: which loop had an unreadable bound, which call was matched by
name, which callee is itself only `heuristic`.

`--explain` can be written as `/perf --explain NAME` in `rqx` interactive sessions and
takes `--json`. It cannot be combined with `--by`, `--sort`, `--top` or `--min-depth`.

## Asking a different question

```sh
rqx perf --sort time        # the largest composed time first; --sort order ranks the same way
rqx perf --sort space       # the largest memory first
rqx perf --sort io          # round trips first, whatever the loop order
rqx perf --sort reach       # what the rest of the project leans on hardest
rqx perf --sort sampled     # just the hot list, when a profile was read
rqx perf --min-depth 2      # skip anything that is not already nested
rqx perf --by dir --top 5   # the worst each directory holds
```

`--by file` and `--by dir` report the heaviest time, space and io order among the
functions they group, the least sure confidence, and the sum of their round trips. An
average would hide one expensive function among many ordinary ones.

The default ranking multiplies the weight of a function's findings by its `reach`, by a
factor for the share of samples when a profile was read, and by `1 + rank / 4`, where
`rank` grows with the time order: 5 for `O(n)`, 6 for `O(n log n)`, 9 for `O(n²)` or
`O(n·m)`, 13 for `O(n³)`, and 17 for `exp`.

## Limits

- **No compiler types.** Receiver types come from what the source states. A variable
  declared with `auto`, `var` or `:=` from a call has no type, and a call on it is
  costed from the method name or not at all. Dynamic dispatch is approximated by
  method name.
- **Sizes are not values.** A loop over a list of three elements and a loop over a table
  of a million rows both read as `O(n)`. Whether `n` is large is for a profile or the
  reader to say.
- **Only the files the scan read take part.** A scan of a single file does not follow
  calls out of it, and the summary then counts those calls as constant.
- **Functions found only at runtime.** A call through a callback parameter, a function
  stored in a table or a name built from a string cannot be followed.
- **Early exits and data shape.** The analysis reports the upper bound of a search or
  of a sort whose input is often already ordered.
- **Names are weak evidence.** The rules that depend on one are `heuristic`.

## The rules behind it

Ten of the rules in the `performance` family each report a code shape that can be
checked by reading:

| rule | |
|---|---|
| `loop-concatenation` | text built one piece at a time inside a loop |
| `sequential-await` | two waits in a row where the second does not read the first's answer |
| `scan-in-loop` | a whole-collection search, sort or copy inside a loop |
| `deep-loop-nesting` | loops nested past the `loop_nesting` threshold |
| `query-in-loop` | a database, HTTP or queue call inside a loop |
| `await-in-loop` | a wait inside a loop body, where the passes could overlap |
| `invariant-in-loop` | a regular expression compiled from a literal on every pass |
| `accumulate-by-copy` | an accumulator rebuilt by copying itself each time round |
| `branching-recursion` | a function calling itself twice with no memo in sight |
| `blocking-call-in-async` | a synchronous file, process or sleep call in an async function |

The cost analysis behind `rqx perf` feeds another set of rules. Each reads one function
at a time, from the declared types and the library cost tables, so a finding does not
depend on any other file:

| rule | |
|---|---|
| `linear-op-in-loop` | a library call that walks or shifts a collection of a declared type, inside a loop, where `scan-in-loop` does not already report it |
| `sort-in-loop` | a sort of a collection the loop does not change |
| `allocation-in-loop` | three or more allocations, or one fixed allocation of a thousand elements or more, per pass of an innermost loop (off by default) |
| `missing-capacity` | an empty Go, Java or C++ collection appended to in a loop over something of known size (off by default) |
| `lock-in-loop` | a lock taken on every pass when the loop does not change which lock it is |
| `unbounded-concurrency` | work that leaves the process, started once per element of a collection, with no limiter, semaphore or pool in the function |
| `unbounded-cache` | a shared collection that functions only add keyed entries to, or `lru_cache(maxsize=None)` |
| `defer-in-loop` | a Go `defer` with a loop between it and its function |
| `copy-in-range-for` | a C++ range-for variable declared by value with a string or container type |
| `large-by-value` | a C++ string or container parameter taken by value and only read |
| `boxing-in-loop` | a Java `Integer`, `Long` or `Double` local that a loop updates with `+=` or `++` |

`accumulate-by-copy` also reports a name declared as `String`, `string` or `str` that a
loop adds to with `+=`, at `verified` confidence. A call that `scan-in-loop` reports is
never reported again by `linear-op-in-loop` or `sort-in-loop`. The `perf` preset switches
on the rules above that ship off.

Four more rules read the cost of a function composed with the functions it calls,
across files, so an edit to a callee changes what its callers are reported for:

| rule | |
|---|---|
| `quadratic-via-call` | a loop over a collection around a call whose time grows with a collection the function is given, the same one or another. It is the cost `scan-in-loop` reports for a library call, behind a function of the project. The evidence is the way to the cost, such as `O(n²): loop :90 → findMatch O(n) :12` |
| `deep-recursion` | a function that calls itself once on an argument one smaller than its own, so its stack grows with the input. Not reported for Go, whose goroutine stacks grow on demand |
| `repeated-computation-in-loop` | a call to a function that takes linear time or more, or to a library function the cost tables mark as pure, whose arguments the loop does not assign and which are not the loop variable. It stands in for `invariant-in-loop` where the call is not a regular expression, and a line `invariant-in-loop` reports is not reported again |
| `pair-scan-could-hash` | two nested loops over different collections whose inner body compares something of each element with `==` (off by default) |

A call that is a statement on its own is not a repeat, because it is made for what it
does. A call is only known to be repeated, not to be free of side effects, so
`repeated-computation-in-loop` is `heuristic`. `quadratic-via-call` is `verified`
only when every function on the way to the cost is, and `heuristic` otherwise. Where
a call is both repeated and quadratic, it is reported as repeated, since hoisting it is
the fix. A callee is looked up by name, so a call that could reach more than four
functions of the same name reaches none of them.

These rules need every file of the project: a call into a file the scan did not read
would be costed as constant. A scan of a single file, as in `rqx scan src/a.ts`,
therefore skips all four.

Names are weak evidence, so the rules that depend on one are `heuristic` and are not
reported as `error`. A receiver that is already keyed (a `Set`, a `Map`, a `dict`, or
anything named `index`, `lookup` or `cache`) is treated as a lookup, not as a scan or
a round trip, and is not reported. `performance.receivers` and `performance.methods`
in `rqx.yaml` add the names a project uses for its own gateways.

A function appears in `rqx perf` when it carries one of these findings or, when a
profile was read, when it was on the stack in enough samples.

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

## The HTML report

> Deprecated: the `html` reporter will be removed in 0.6.0. Use `json` or `sarif` for
> a report kept as an artifact.

`rqx scan --reporter html` has a Performance section after the findings by rule. It
lists up to ten functions in the order `rqx perf` ranks them, with `time`, `space`,
`io` and `conf`, and links each to its file further down the page. Under each row a
`<details>` element holds the same derivation `--explain` prints. The page uses no
script. A project with nothing to rank has no section.

## JSON

`--json` writes one object per row. The keys `name`, `at`, `order`, `loops`, `io`,
`reach`, `weight`, `sampled`, `findings` and `functions` are as they have always been:
`order` is the syntactic loop order, `loops` the loop nesting depth, and `io` the count
of round trips found inside loops. Each entry of `findings` has its rule, its evidence
and the technique that addresses it. `functions` is the number of functions that a
row of a grouped view stands for. The numbers are the ones `rqx scan` gates on, so
output built from this command and a gate built on `loop_nesting` use the same values.

Rows also carry these keys, which were added in 0.5.0:

| key | value |
|---|---|
| `time`, `space` | the composed orders, such as `"O(n·m)"`, with sizes named `n`, `m`, `k` |
| `io_order` | the order of the calls that leave the process. It is not `io`, which keeps its meaning |
| `alloc` | allocations per pass of the innermost loop, as a number |
| `confidence` | `"verified"`, `"heuristic"` or `"unknown"` |
| `cost` | `{"expr", "trace", "vars"}`, or `null` for a row that groups functions |

`cost.expr` is the time written with the names the function uses, such as
`"O(orders·prices)"`. `cost.trace` lists the path that makes the time what it is, at
most six steps, each with `file`, `line`, `what` and `order`. `cost.vars` says what each
letter stands for: `id`, `name` and `source` (`parameter`, `local value`, `field`,
`module-level value`, `unknown` or `not named`). For a grouped row (`--by file` or
`--by dir`) the orders are the heaviest in the group and the confidence is the least
sure.

`rqx perf --explain NAME --json` writes one object with `name`, `at`, `time`, `space`,
`io_order`, `stack`, `confidence`, `recursion`, `reasons`, and `derivation`: a list of
`{dimension, order, steps}` where each step has `kind` (`loop`, `call` or `library`),
`what`, `order` and `children`. The MCP `perf` tool returns the same row keys.
