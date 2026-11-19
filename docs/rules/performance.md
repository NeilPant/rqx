# Performance

[Back to the catalogue](README.md)

## `loop-concatenation`

each pass copies everything written so far, so the work grows with the square of the number of pieces

| | |
|---|---|
| Measures | text built one piece at a time inside a loop |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Accumulate in Place |

From `fixtures/typescript/flow.ts`, line 199:

```ts
  for (const row of rows) {
    out += row;
  }
```

Suggested fix: collect the pieces and join them once

## `sequential-await`

nothing orders them, so the call takes the sum of two waits where it could take the longer one

| | |
|---|---|
| Measures | two waits in a row where the second does not read the first's answer |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Await Concurrently |
| Not in Go | waiting is a channel receive, which the language does not mark |
| Not in Java | waiting is a call like any other |
| Not in C++ | waiting is a call like any other |

From `fixtures/typescript/performance.ts`, line 93:

```ts
  const first = await loadOne();
  const second = await loadTwo();
  return first + second;
```

Suggested fix: start both and wait for both together

## `scan-in-loop`

the inner scan runs once per pass of the outer loop, so the work grows with the product of the two sizes and a list that is fast at a hundred items can stall at ten thousand

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

Suggested fix: build a set or a map of what the scan looks for once, above the loop

## `deep-loop-nesting`

each level multiplies the passes of the one inside it, so the innermost body runs a number of times that tests on small data do not show

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

Suggested fix: index what the inner loops search, so `sumCube` walks the data once instead of once per pass

## `query-in-loop`

every pass pays a round trip, so listing a hundred rows makes a hundred and one requests and the page waits for the sum of them

| | |
|---|---|
| Measures | a call on a database, HTTP or queue client inside a loop |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Batch the Work |

From `fixtures/typescript/cost-unbounded-concurrency.ts`, line 4:

```ts
async function loadEveryPage(ids: string[]): Promise<unknown[]> {
  return Promise.all(ids.map((id) => fetch(id)));
}
```

Suggested fix: gather the keys first and ask for all of them in one call

## `await-in-loop`

each pass waits for the previous one to finish, so work that could overlap runs end to end and the loop takes as long as all of its waits added together

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

Suggested fix: start every call before waiting on any of them

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

Suggested fix: compile it once above the loop and reuse it

## `accumulate-by-copy`

each step copies everything gathered so far, so building a result of n items touches about half of n squared items

| | |
|---|---|
| Measures | an accumulator rebuilt by copying itself on every pass of a loop |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Accumulate in Place |

From `fixtures/typescript/flow.ts`, line 199:

```ts
  for (const row of rows) {
    out += row;
  }
```

Suggested fix: add to the accumulator in place instead of rebuilding it

## `branching-recursion`

two calls per level is an exponential number of calls, and most of them recompute an answer another branch already found

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

Suggested fix: cache the results of `fibonacci`, or rewrite it as a loop over a table

## `blocking-call-in-async`

an async function is expected not to hold the thread and this call holds it, so everything else waiting on that event loop queues behind one blocking call

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

Suggested fix: call the asynchronous form of `readFileSync`, or move the work off this thread

## `linear-op-in-loop`

the call costs time proportional to the size of the collection on every pass, so the loop grows with the product of its own length and the collection's

| | |
|---|---|
| Measures | a library call that walks or shifts its whole collection, on a collection of a known type, inside a loop that scan-in-loop does not report |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Loop with Lookup |

From `fixtures/typescript/cost-linear-op-in-loop.ts`, line 5:

```ts
  while (entries.length > 0) {
    total += (entries.shift() ?? 0);
  }
```

Suggested fix: use a structure whose operation takes constant time, or take the operation out of the loop

## `sort-in-loop`

every pass after the first sorts input that is already sorted, at a cost of n log n each time

| | |
|---|---|
| Measures | a sort of a collection that the loop does not change, inside a loop that scan-in-loop does not report |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Hoist Invariant |

From `fixtures/typescript/cost-sort-in-loop.ts`, line 4:

```ts
  for (const group of groups) {
    ranked.push([...base].sort());
    ranked.push(group);
```

Suggested fix: sort once above the loop: nothing in the loop changes what is being sorted

## `allocation-in-loop`

each pass pays for memory it could have reused, which adds allocator and collection work to every iteration

| | |
|---|---|
| Measures | three or more allocations, or one fixed allocation of a thousand elements or more, in each pass of an innermost loop |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Hoist Invariant |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/cost-allocation-in-loop.ts`, line 4:

```ts
  for (const row of rows) {
    const pair = [row, row + 1];
    const label = { row, next: row + 1 };
```

Suggested fix: allocate once above the loop and reuse it, clearing it between passes

## `missing-capacity`

the collection grows by reallocating and copying its contents as it fills, work that a capacity set from the loop's size avoids

| | |
|---|---|
| Measures | an empty collection appended to in a loop over a collection whose size is known before the loop starts |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Presize the Collection |
| Default | off; name it under `rules.enabled` |
| Not in JavaScript | arrays grow without a capacity to set |
| Not in TypeScript | arrays grow without a capacity to set |
| Not in Python | lists grow without a capacity to set |

From `fixtures/go/fixtures/cost-linear-op-in-loop.go`, line 16:

```go
	for _, item := range items {
		results = append(results, item)
	}
```

Suggested fix: give it the loop's size as its capacity where it is made

## `lock-in-loop`

each pass pays to acquire and release the same lock, and other threads can only interleave at those points instead of waiting once for the whole loop

| | |
|---|---|
| Measures | a lock taken inside a loop on a lock the loop does not change |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Hoist Invariant |

From `fixtures/typescript/cost-lock-in-loop.ts`, line 9:

```ts
  for (const row of rows) {
    latch.lock();
    total += row;
```

Suggested fix: take the lock once around the whole loop, or keep it per pass only if other threads need the gaps

## `unbounded-cache`

it holds an entry for every distinct key the program has ever been given, so memory grows with the variety of the input for as long as the process runs

| | |
|---|---|
| Measures | a shared collection that functions add keyed entries to and nothing removes from or measures, or an lru_cache with no maximum size |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Bound the Cache |

From `fixtures/typescript/cost-unbounded-cache.ts`, line 4:

```ts
function rememberPage(key: string, size: number): number {
  pageSizes.set(key, size);
  return size;
```

Suggested fix: bound it with a maximum size and an eviction rule, or key it by something that has a limited number of values

## `defer-in-loop`

a deferred call runs when the function returns, not when the pass ends, so every pass holds what it opened until the whole loop is done

| | |
|---|---|
| Measures | a defer statement with a loop between it and its enclosing function |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Not in JavaScript | JavaScript has no defer statement; cleanup is written in a finally block |
| Not in TypeScript | TypeScript has no defer statement; cleanup is written in a finally block |
| Not in Python | a with block releases what it opens when the block ends, inside the pass |
| Not in Java | try-with-resources releases what it opens when its block ends, inside the pass |
| Not in C++ | a destructor runs when its scope ends, which is inside the pass |

From `fixtures/go/fixtures/cost-defer-in-loop.go`, line 12:

```go
		}
		defer file.Close()
		info, err := file.Stat()
```

Suggested fix: move the loop body into its own function, so the deferred call runs at the end of each pass

## `boxing-in-loop`

each update unboxes the value and boxes the result as a new object, which the primitive type does in place

| | |
|---|---|
| Measures | a local declared as Integer, Long, Double or another boxed number and changed with an arithmetic update inside a loop |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Use a Primitive Type |
| Not in JavaScript | numbers have one representation; there is no boxed form to avoid |
| Not in TypeScript | numbers have one representation; there is no boxed form to avoid |
| Not in Python | every int is an object; there is no primitive form to use instead |
| Not in Go | numeric values are stored directly; nothing is boxed |
| Not in C++ | numeric values are stored directly; nothing is boxed |

From `fixtures/java/src/main/java/rqx/CostBoxingInLoop.java`, line 9:

```java
        for (Integer row : rows) {
            total += row;
        }
```

Suggested fix: declare it with the primitive type

## `copy-in-range-for`

each pass copies the element into the variable, allocating and copying its contents, when a reference to it would read the same value

| | |
|---|---|
| Measures | a range-for variable declared by value with a string or container type, that the loop body does not change |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Take by Reference |
| Not in JavaScript | the loop variable refers to the element; nothing is copied |
| Not in TypeScript | the loop variable refers to the element; nothing is copied |
| Not in Python | the loop variable is a name for the element; nothing is copied |
| Not in Go | range always copies the element and offers no reference form to choose instead |
| Not in Java | the loop variable refers to the element; nothing is copied |

From `fixtures/cpp/src/cost_copy_in_range_for.cpp`, line 5:

```cpp
  std::size_t total = 0;
  for (std::string name : names) {
    total += name.size();
  }
  return total;
```

Suggested fix: declare the variable as a const reference

## `large-by-value`

every call copies the whole argument for a function that only reads it, and the cost grows with the size of what callers pass

| | |
|---|---|
| Measures | a string or container parameter taken by value that the function neither changes nor passes on with std::move |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Take by Reference |
| Not in JavaScript | arguments are references to the value; nothing is copied |
| Not in TypeScript | arguments are references to the value; nothing is copied |
| Not in Python | arguments are references to the value; nothing is copied |
| Not in Go | a slice, map or string argument copies only its small header |
| Not in Java | arguments are references to the object; nothing is copied |

From `fixtures/cpp/src/cost_large_by_value.cpp`, line 3:

```cpp

std::size_t count_characters(std::vector<std::string> lines) {
  std::size_t total = 0;
```

Suggested fix: take it as a const reference, or take it by value and std::move it if the function keeps it

## `unbounded-concurrency`

the number of requests in flight equals the size of the collection, so a large input exhausts connections, memory or the other side's rate limit

| | |
|---|---|
| Measures | work that leaves the process started once per element of a collection of any size, in a function that mentions no limit |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Bound Concurrency |

From `fixtures/typescript/cost-unbounded-concurrency.ts`, line 4:

```ts
async function loadEveryPage(ids: string[]): Promise<unknown[]> {
  return Promise.all(ids.map((id) => fetch(id)));
}
```

Suggested fix: run the work through a semaphore, a limiter or a fixed-size pool

## `quadratic-via-call`

the callee does work in proportion to a collection on every pass of the loop, so the total grows with the product of the two sizes, and with the square of the size when they are one collection, although the caller shows only one loop

| | |
|---|---|
| Measures | a loop over a collection around a call whose cost grows with a collection the function was given |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Loop with Lookup / Replace Algorithm |

From `fixtures/typescript/cost-quadratic-via-call.ts`, line 13:

```ts
  for (let at = 0; at < items.length; at++) {
    if (indexOfItem(items, items[at]) === at) {
      firsts.push(items[at]);
```

Suggested fix: index what the callee searches once above the loop, or change the callee to look up by key so each pass costs constant time

## `deep-recursion`

every level keeps its stack frame until the last one returns, so an input larger than the stack allows ends in a stack overflow that tests on small inputs never reach

| | |
|---|---|
| Measures | a function that calls itself once, on an argument one smaller than its own |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Replace Algorithm |
| Not in Go | a goroutine's stack grows on demand up to a limit set by the runtime, so depth is bounded by memory and not by a fixed stack size |

From `fixtures/typescript/cost-deep-recursion.ts`, line 5:

```ts
  }
  return n + sumTo(n - 1);
}
```

Suggested fix: walk the input with a loop and an explicit stack, so the depth is not limited by the call stack

## `repeated-computation-in-loop`

the call returns the same answer on every pass, so the loop pays its cost once per pass where once in total would do

| | |
|---|---|
| Measures | a call that costs at least linear time, in a loop that changes nothing the call reads |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Hoist Invariant |

From `fixtures/typescript/cost-repeated-computation-in-loop.ts`, line 12:

```ts
  for (const weight of weights) {
    const whole = totalLength(parts);
    out.push(weight / whole);
```

Suggested fix: compute it once above the loop: nothing in the loop changes what it is given

## `pair-scan-could-hash`

every element of one collection is compared with every element of the other to find its match, which a map keyed on the compared field answers in constant time per element

| | |
|---|---|
| Measures | two nested loops over different collections, comparing a field of each element with == |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Loop with Lookup |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/cost-pair-scan-could-hash.ts`, line 15:

```ts
    for (const user of users) {
      if (order.userId === user.id) {
        names.push(user.name);
```

Suggested fix: build a map from the key of one collection above the loop and look each element of the other up in it

