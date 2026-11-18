# Safety

[Back to the catalogue](README.md)

## `imprecise-type`

each of these types turns off type checking for the value, so the guarantees the rest of the file relies on do not hold here

| | |
|---|---|
| Measures | an `any` annotation, a non-null assertion, or a cast that is not `as const` |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Replace Data Value with Object / Introduce Assertion |
| Not in JavaScript | JavaScript declares no types, so there is no annotation or cast to be imprecise |

From `fixtures/typescript/hygiene.ts`, line 89:

```ts
    return rows.length;
  } catch (error: any) {
    return error ? 0 : 0;
```

Suggested fix: name the shape the code accepts, or use `unknown` and narrow it

## `boundary-arithmetic`

each place that recomputes the boundary can compute it wrongly, and off-by-one errors are found by readers, not by types

| | |
|---|---|
| Measures | the same subject adjusted by one at more sites than the threshold |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Variable / Extract Method |
| Tuned by | `thresholds.boundary_repeats` |

From `fixtures/typescript/conventions.ts`, line 9:

```ts
export function window(items: string[], cursor: number): string[] {
  const head = items[cursor + 1];
  const tail = items[cursor + 1];
```

Suggested fix: name the boundary once, so `cursor` is adjusted in one place

## `query-with-side-effect`

callers assume a query has no effect, so they call it more than once, reorder calls, and are surprised when the answer changes

| | |
|---|---|
| Measures | a get, is, has, should or can function that assigns to a member |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Separate Query from Modifier / Rename Method |

From `fixtures/typescript/queries.ts`, line 5:

```ts

  getTotal(): number {
    this.reads += 1;
```

Suggested fix: split `getTotal` into the question and the change, or rename it for what it does

## `assignment-in-condition`

it reads as a comparison and behaves as an assignment, so an accidental assignment is easy to miss in review

| | |
|---|---|
| Measures | an assignment written where a condition is expected |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Extract Variable |
| Not in Python | the only assignment Python allows in a condition is `:=`, which exists to be written there and is read as deliberate |
| Not in Go | Go allows an initialiser before the condition rather than inside it, so the two are never confused |

From `fixtures/typescript/flow.ts`, line 58:

```ts
  let found = 0;
  if ((found = a + b)) {
    return found;
```

Suggested fix: assign above the branch, or compare if that is what was meant

## `closure-over-loop-variable`

the closure reads whichever value the loop variable holds when it runs, so every closure sees the last element instead of its own

| | |
|---|---|
| Measures | a function written inside a loop that reads the name the loop moves |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method / Introduce Parameter Object |
| Not in Java | a captured local must already be effectively final |

From `fixtures/typescript/performance.ts`, line 75:

```ts
  for (var at = 0; at < rows.length; at++) {
    out.push(function later(): number {
      return at;
    });
  }
```

Suggested fix: pass the value in, so each function keeps the one it was made with

## `mutated-while-iterated`

the iteration and the mutation disagree about what remains, so elements are skipped or visited twice depending on where the change lands

| | |
|---|---|
| Measures | changing the collection a loop is walking, from inside the walk |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Replace Algorithm |

From `fixtures/typescript/performance.ts`, line 85:

```ts
    if (row < 0) {
      rows.splice(0, 1);
    }
```

Suggested fix: collect what to change, and change it after the walk

## `production-assert`

the language may compile the assertion out, so the check a reader relies on may not run in production

| | |
|---|---|
| Measures | an assertion outside a test |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |
| Not in Go | there is no assert; a failure is a value that is returned |

From `fixtures/typescript/performance.ts`, line 98:

```ts
export function guards(value: number): number {
  globalThis.console.assert(value > 0);
  return value;
```

Suggested fix: raise the failure this check guards against, so it is enforced wherever the code runs

## `sleep-in-production`

it waits for a fixed length of time instead of for the condition it needs, so the delay is either too short to work or longer than necessary

| | |
|---|---|
| Measures | a call that stops the thread for a fixed time, outside a test |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Algorithm |

From `fixtures/typescript/performance.ts`, line 117:

```ts
export async function paced(): Promise<number> {
  await sleep(500);
  return openTabs;
```

Suggested fix: wait for the condition itself, with an explicit timeout

## `shadowed-handler`

the handler below can never run, so whatever it was written to do about that failure has never been done

| | |
|---|---|
| Measures | a handler that takes everything, written before one that takes less |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional |
| Not in Go | errors are values, so there is no handler to order |
| Not in TypeScript | the language allows one handler per block |
| Not in JavaScript | the language allows one handler per block |

From `fixtures/python/hygiene.py`, line 156:

```python
        return len(rows)
    except Exception:
        return 0
    except ValueError:
```

Suggested fix: put the narrower handler first, so the wider one handles only the remaining failures

## `dropped-promise`

nothing waits for it and nothing observes its failure, so a failure surfaces elsewhere with no link to what started it

| | |
|---|---|
| Measures | an asynchronous call whose result is discarded |
| Reported as | error, measured threshold |
| Reads | the syntax tree alone |
| Fix | Separate Query from Modifier |
| Not in Go | work started in the background is a goroutine, which the language marks |
| Not in Java | a future is a value, and dropping it is dropping a value |
| Not in C++ | a future is a value, and dropping it is dropping a value |

From `fixtures/typescript/performance.ts`, line 126:

```ts
export async function fires(rows: number[]): Promise<number> {
  loadOne();
  return rows.length;
```

Suggested fix: await it, or state in the code what handles its result

## `guard-cascade`

the body has not started and the reader has already read several decisions, none of which says what the function is for

| | |
|---|---|
| Measures | checks at the top of a body that all answer the same way |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion / Introduce Parameter Object |
| Tuned by | `thresholds.guard_cascade` |

From `fixtures/typescript/bloat.ts`, line 63:

```ts

export function manyPaths(kind: string, rows: number[]): number {
  if (kind === "a") return 1;
```

Suggested fix: accept a type that cannot hold an invalid value, or validate once where the values enter

## `blanket-try`

the handler wraps the whole body instead of answering one failure, so a mistake anywhere inside arrives as the failure the handler was written for

| | |
|---|---|
| Measures | a guarded block holding nearly every statement of its function |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional / Extract Method |
| Not in Go | failure is a returned value, so no block is guarded |

From `fixtures/typescript/bloat.ts`, line 53:

```ts
export function widelyGuarded(seed: number): number {
  try {
    const d1 = seed; const d2 = seed; const d3 = seed; const d4 = seed;
    const d5 = seed; const d6 = seed; const d7 = seed; const d8 = seed;
    const d9 = seed; const d10 = seed; const d11 = seed;
    return [d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11].length;
  } catch (error) {
    return 0;
```

Suggested fix: guard only the line that can fail, and let the rest propagate

## `no-base-case`

nothing can stop the recursion, so the first call exhausts the stack

| | |
|---|---|
| Measures | a function that calls itself with no decision anywhere in the body |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional |

From `fixtures/typescript/bloat.ts`, line 86:

```ts

export function loops(rows: number[]): number {
  return loops(rows);
```

Suggested fix: give `loops` the case that answers without calling itself

## `integer-division`

the remainder is discarded and nothing on the line says that was intended, so a reader has to know the operand types to know whether the result is right

| | |
|---|---|
| Measures | one whole number divided by another |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |

From `fixtures/typescript/hierarchy.ts`, line 44:

```ts
  // TODO: work out what happens to the remainder
  return 7 / 2 + total;
}
```

Suggested fix: state what happens to the remainder, by rounding explicitly or dividing as a fraction

## `discarded-error`

the code receives the result and ignores it, and nothing records what happens when the call fails

| | |
|---|---|
| Measures | a call whose answer is assigned to a name that means "ignore this" |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional |

From `fixtures/typescript/hierarchy.ts`, line 48:

```ts
export function ignored(rows: number[]): number {
  const _ = countRows(rows);
  return rows.length;
```

Suggested fix: handle the result, or state in the code why nothing needs to be done

## `float-equality`

a binary float rarely equals the decimal a person wrote, so a branch that depends on it is decided by rounding and not by the value

| | |
|---|---|
| Measures | an equality test against a literal with a fractional part |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |

From `fixtures/typescript/flow.ts`, line 128:

```ts
export function settled(balance: number): boolean {
  return balance === 0.1;
}
```

Suggested fix: compare the difference against a tolerance chosen for the domain

## `mixed-return`

a caller cannot write one line that handles both kinds of result, so every call site carries a check for the case the author happened to consider

| | |
|---|---|
| Measures | one function answering with a value on one path and with nothing on another |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Separate Query from Modifier / Decompose Conditional |
| Not in Go | a naked return beside a named result is the language's own idiom |
| Not in Java | the compiler refuses a body that answers two shapes |
| Not in C++ | the compiler refuses a body that answers two shapes |

From `fixtures/typescript/hygiene.ts`, line 69:

```ts
  if (rows.length === 0) {
    return;
  }
```

Suggested fix: return the same shape on every path, or split the two cases into two functions

## `exit-from-finally`

it replaces whatever the block was leaving with, so a failure raised in the body disappears and nothing records that it happened

| | |
|---|---|
| Measures | a return, break, continue or raise inside the clause that runs on the way out |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional |
| Not in Go | cleanup is a deferred call, which cannot replace the return |
| Not in C++ | there is no finally clause |

From `fixtures/typescript/hygiene.ts`, line 78:

```ts
  } finally {
    return 0;
  }
```

Suggested fix: move it after the block, so the block's own exit is preserved

## `catch-all-handler`

it catches the failures it was written for and every other failure too, including failures that mean the program is already in an invalid state

| | |
|---|---|
| Measures | a handler naming the root of the error hierarchy, or naming nothing at all |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional |
| Not in Go | errors are values, so there is no handler to widen |
| Not in JavaScript | a catch clause cannot name a type in JavaScript, so none can name the root of the hierarchy |

From `fixtures/typescript/hygiene.ts`, line 89:

```ts
    return rows.length;
  } catch (error: any) {
    return error ? 0 : 0;
  }
}
```

Suggested fix: name the failures this level handles, and let the rest propagate

## `generic-throw`

the only way to react to it is to catch every failure, so a caller that wants to handle this one cannot

| | |
|---|---|
| Measures | raising the root error type rather than one that names the failure |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Replace Data Value with Object |
| Not in Go | errors are values, and the language raises nothing |

From `fixtures/typescript/hygiene.ts`, line 52:

```ts
export function stillToWrite(): number {
  throw new Error("not implemented");
}
```

Suggested fix: raise a type named for the failure, so a caller can catch this one

## `library-exit`

it takes the decision away from every caller above it, and a test that reaches the line terminates the test runner

| | |
|---|---|
| Measures | a call that ends the process, written outside the entry point |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Separate Query from Modifier |

From `fixtures/typescript/hygiene.ts`, line 95:

```ts
export function stops(code: number): void {
  process.exit(code);
}
```

Suggested fix: return or raise, and let the code that owns the process decide whether to stop

## `boolean-literal-argument`

nothing at the call site says what the argument turns on, so the reader has to open the callee to find out

| | |
|---|---|
| Measures | a bare true or false among a call's arguments |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Parameter with Explicit Methods |

From `fixtures/typescript/hygiene.ts`, line 99:

```ts
export function requests(url: string): string {
  return fetchPage(url, true);
}
```

Suggested fix: name the value at the call site, or give the callee two methods

## `unjustified-suppression`

the waiver is refused and the finding still counts, because a waiver that needs no justification allows a rule to be switched off one line at a time

| | |
|---|---|
| Measures | an rqx-ignore comment with no reason, or one whose date has passed |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 27:

```ts
export function unreachable(value: number) {
  // rqx-ignore magic-number
  const seed = 1;
```

Suggested fix: say why the finding is acceptable here, or fix it

## `suppressed-diagnostic`

a check that would have caught the next mistake here has been turned off, and the comment that turns it off outlives the reason for it

| | |
|---|---|
| Measures | ts-ignore, ts-nocheck, eslint-disable, or a cast through any |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 33:

```ts

// @ts-ignore
export const suppressed = 1;
```

Suggested fix: fix what the check is reporting, or record why suppressing it is right

## `flag-argument`

the function does two things and the caller selects which, so `f(x, true)` tells a reader nothing about what will happen

| | |
|---|---|
| Measures | parameter declared with a boolean type |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Parameter with Explicit Methods |
| Not in JavaScript | a parameter declares no type, so nothing says it is a boolean |

From `fixtures/typescript/config.ts`, line 1:

```ts
export function endpoint(region: string, secure: boolean, retries: number): string {
  if (region.length > 0) {
```

Suggested fix: split into two functions named for what each does, so no call site reads `(…, true)` and leaves `secure` unexplained

## `output-parameter`

the caller's value changes without the call site saying so, which is hard to find when it turns out to be wrong

| | |
|---|---|
| Measures | parameter reassigned or mutated inside the function body |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Remove Assignments to Parameters / Separate Query from Modifier |

From `fixtures/typescript/hygiene.ts`, line 30:

```ts
  return value + seed;
  value = 1;
}
```

Suggested fix: assign to a local instead of `value`, and return the result

## `complex-condition`

readers cannot hold this many clauses at once, so the branch is taken on trust and a bug in it survives review

| | |
|---|---|
| Measures | boolean operands in a single condition beyond the configured limit |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Consolidate Conditional / Extract Variable |
| Tuned by | `thresholds.condition_operands` |

From `fixtures/typescript/hygiene.ts`, line 14:

```ts

  if (!applyDiscount && total > 4999 && cart.length > 12 && orbit()) {
    return total;
```

Suggested fix: name the condition in a variable or a predicate that says what it means

## `negative-conditional`

the reader has to invert the condition mentally to follow the `else`, and inverting a compound condition by hand is a source of off-by-one logic errors

| | |
|---|---|
| Measures | negated condition that has an else branch |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Decompose Conditional |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/shapes.ts`, line 51:

```ts
export function gate(ready: boolean, retries: number) {
  if (!ready) {
    return 0;
```

Suggested fix: invert the condition and swap the branches so the common case reads first

