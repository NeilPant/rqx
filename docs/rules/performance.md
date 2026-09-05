# Performance

[Back to the catalogue](README.md)

## `scan-in-loop`

the inner scan runs once per pass of the outer loop, so the work grows with the product of the two sizes and a list that was fine at a hundred items stalls at ten thousand

| | |
|---|---|
| Measures | a whole-collection search, sort or copy inside a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Loop with Lookup |

## `deep-loop-nesting`

each level multiplies the passes of the one inside it, so the innermost body runs a number of times no test on small data will ever show

| | |
|---|---|
| Measures | loop nesting depth, counting callbacks and comprehensions as loops |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Loop with Lookup / Replace Algorithm |
| Tuned by | `thresholds.loop_nesting` |

## `query-in-loop`

every pass pays a round trip, so listing a hundred rows makes a hundred and one requests and the page waits for the sum of them

| | |
|---|---|
| Measures | a call on a database, HTTP or queue client inside a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Batch the Work |

## `await-in-loop`

each pass waits for the one before it to finish, so work that could overlap runs end to end and the loop takes as long as all of its waits added together

| | |
|---|---|
| Measures | an await inside a loop body, outside any nested function |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Batch the Work / Await Concurrently |
| Not in Go | a goroutine blocks by design, so there is no await here to serialise |
| Not in Java | Java has no await expression: a blocking call is a call, and futures compose by method |

## `invariant-in-loop`

the same pattern is parsed and compiled on every pass when once would do, and compiling is the expensive half of matching

| | |
|---|---|
| Measures | a regular expression compiled from a literal inside a loop |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Hoist Invariant |

## `accumulate-by-copy`

each step copies everything gathered so far, so building a result of n items touches half of n squared of them

| | |
|---|---|
| Measures | an accumulator rebuilt by copying itself on every pass of a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Accumulate in Place |

## `branching-recursion`

two calls per level is an exponential number of calls, most of them recomputing an answer some other branch already found

| | |
|---|---|
| Measures | a function calling itself two or more times with no memo in sight |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Memoize / Replace Algorithm |

## `blocking-call-in-async`

an async function promises not to hold the thread and this call holds it, so everything else waiting on that loop queues behind one file read

| | |
|---|---|
| Measures | a synchronous file, process or sleep call inside an async function |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Blocking Call |
| Not in Go | blocking a goroutine is the intended model, not a mistake to report |
| Not in Java | Java has no async function marker, so there is no promise here to break |
| Not in C++ | a coroutine is decided by co_return in its body rather than by a marker on its signature, so there is nothing to test a call against |

