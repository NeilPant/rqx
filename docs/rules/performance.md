# Performance

[Back to the catalogue](README.md)

## `loop-concatenation`

each pass copies everything written so far, so the work grows with the square of the number of pieces

| | |
|---|---|
| Measures | text built one piece at a time inside a loop |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Algorithm |

From `fixtures/typescript/flow.ts`, line 186:

```ts
  for (const row of rows) {
    out += row;
  }
```

rqx says: collect the pieces and join them once

## `sequential-await`

nothing makes them an order, so the call takes the sum of two waits where it could have taken the longer one

| | |
|---|---|
| Measures | two waits in a row where the second does not read the first's answer |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Algorithm |
| Not in Go | waiting is a channel receive, which the language does not mark |
| Not in Java | waiting is a call like any other |
| Not in C++ | waiting is a call like any other |

From `fixtures/typescript/performance.ts`, line 93:

```ts
  const first = await loadOne();
  const second = await loadTwo();
  return first + second;
```

rqx says: start both and wait for the pair

## `scan-in-loop`

the inner scan runs once per pass of the outer loop, so the work grows with the product of the two sizes and a list that was fine at a hundred items stalls at ten thousand

| | |
|---|---|
| Measures | a whole-collection search, sort or copy inside a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Loop with Lookup |

From `fixtures/typescript/performance.ts`, line 9:

```ts
  for (const row of rows) {
    if (wanted.indexOf(row) >= 0) {
      kept.push(row);
```

rqx says: build a set or a map of what the scan looks for once, above the loop

## `deep-loop-nesting`

each level multiplies the passes of the one inside it, so the innermost body runs a number of times no test on small data will ever show

| | |
|---|---|
| Measures | loop nesting depth, counting callbacks and comprehensions as loops |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Loop with Lookup / Replace Algorithm |
| Tuned by | `thresholds.loop_nesting` |

From `fixtures/typescript/performance.ts`, line 16:

```ts

export function sumCube(grid: number[][][]): number {
  let total = 0;
```

rqx says: index what the inner loops search, so `sumCube` walks the data once instead of once per pass

## `query-in-loop`

every pass pays a round trip, so listing a hundred rows makes a hundred and one requests and the page waits for the sum of them

| | |
|---|---|
| Measures | a call on a database, HTTP or queue client inside a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Batch the Work |

From `fixtures/typescript/performance.ts`, line 31:

```ts
  for (const id of ids) {
    rows.push(client.get(id));
  }
```

rqx says: gather the keys first and ask for all of them in one call

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

From `fixtures/typescript/performance.ts`, line 39:

```ts
  for (const path of paths) {
    texts.push(await reader.read(path));
  }
```

rqx says: start every call before waiting on any of them

## `invariant-in-loop`

the same pattern is parsed and compiled on every pass when once would do, and compiling is the expensive half of matching

| | |
|---|---|
| Measures | a regular expression compiled from a literal inside a loop |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Hoist Invariant |

From `fixtures/typescript/performance.ts`, line 47:

```ts
  for (const line of lines) {
    const digits = new RegExp("^[0-9]+$");
    hits += digits.test(line) ? 1 : 0;
```

rqx says: compile it once above the loop and reuse it

## `accumulate-by-copy`

each step copies everything gathered so far, so building a result of n items touches half of n squared of them

| | |
|---|---|
| Measures | an accumulator rebuilt by copying itself on every pass of a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Accumulate in Place |

From `fixtures/typescript/flow.ts`, line 186:

```ts
  for (const row of rows) {
    out += row;
  }
```

rqx says: add to the accumulator in place instead of rebuilding it

## `branching-recursion`

two calls per level is an exponential number of calls, most of them recomputing an answer some other branch already found

| | |
|---|---|
| Measures | a function calling itself two or more times with no memo in sight |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Memoize / Replace Algorithm |

From `fixtures/typescript/performance.ts`, line 61:

```ts

export function fibonacci(step: number): number {
  if (step < 2) {
```

rqx says: keep what `fibonacci` has already answered, or rewrite it as a loop over a table

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

From `fixtures/typescript/performance.ts`, line 69:

```ts
export async function settingsOf(path: string): Promise<string> {
  return readFileSync(path, "utf8");
}
```

rqx says: call the asynchronous form of `readFileSync`, or move the work off this thread

