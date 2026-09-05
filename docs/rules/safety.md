# Safety

[Back to the catalogue](README.md)

## `imprecise-type`

each of these tells the checker to stop checking, so the guarantee the rest of the file is written against quietly stops holding here

| | |
|---|---|
| Measures | an `any` annotation, a non-null assertion, or a cast that is not `as const` |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Replace Data Value with Object / Introduce Assertion |

From `fixtures/typescript/precision.ts`, line 5:

```ts
 */
export function widen(input: any): string {
  const raw = input as string;
```

rqx says: name the shape you actually accept, or `unknown` and narrow it

## `boundary-arithmetic`

every place the boundary is recomputed is a place it can be recomputed wrongly, and off-by-one errors are found by readers rather than types

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

rqx says: name the boundary once, so `cursor` is adjusted in one place

## `query-with-side-effect`

callers assume asking is free, so they ask twice, order the calls freely, and are surprised when that changes the answer

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

rqx says: split `getTotal` into the question and the change, or rename it for what it does

## `assignment-in-condition`

it reads as a comparison and behaves as a write, which is why it is the typo that survives review

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

rqx says: assign above the branch, or compare if that is what was meant

## `float-equality`

a binary float almost never lands on the decimal a person wrote, so the branch that depends on it is decided by rounding rather than by the value

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

rqx says: compare the difference against a tolerance the domain decides

## `unjustified-suppression`

the waiver is refused and the finding still stands, because a waiver nobody has to justify is how a rule gets switched off one line at a time

| | |
|---|---|
| Measures | an rqx-ignore comment with no reason, or one whose date has passed |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 25:

```ts
export function unreachable(value: number) {
  // rqx-ignore magic-number
  const seed = 1;
```

rqx says: say why the finding is acceptable here, or fix it

## `suppressed-diagnostic`

a check that would have caught the next mistake here has been turned off, and the comment turning it off outlives the reason for it

| | |
|---|---|
| Measures | ts-ignore, ts-nocheck, eslint-disable, or a cast through any |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 31:

```ts

// @ts-ignore
export const suppressed = 1;
```

rqx says: fix what the check is reporting, or record why suppressing it is right

## `flag-argument`

the function does two things and the caller picks which, so `f(x, true)` tells a reader nothing about what will happen

| | |
|---|---|
| Measures | parameter declared with a boolean type |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Parameter with Explicit Methods |

From `fixtures/typescript/config.ts`, line 1:

```ts
export function endpoint(region: string, secure: boolean, retries: number): string {
  if (region.length > 0) {
```

rqx says: split into two functions named for what each does, so no call site reads `(…, true)` and leaves `secure` a mystery

## `output-parameter`

the caller's value changes without the call site saying so, which is the hardest kind of change to find when it turns out to be wrong

| | |
|---|---|
| Measures | parameter reassigned or mutated inside the function body |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Remove Assignments to Parameters / Separate Query from Modifier |

From `fixtures/typescript/flow.ts`, line 112:

```ts
  for (const item of items) {
    total += item;
    continue;
```

rqx says: assign to a local instead of `total`, and return the result

## `complex-condition`

nobody can hold this many clauses at once, so the reader takes the branch on trust and the bug in it survives every review

| | |
|---|---|
| Measures | boolean operands in a single condition beyond the configured limit |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Consolidate Conditional / Extract Variable |
| Tuned by | `thresholds.condition_operands` |

From `fixtures/typescript/hygiene.ts`, line 12:

```ts

  if (!applyDiscount && total > 4999 && cart.length > 12 && orbit()) {
    return total;
```

rqx says: name the condition in a variable or a predicate that says what it means

## `negative-conditional`

the reader has to invert it mentally to follow the else, and inverting a compound condition by hand is where off-by-one logic errors come from

| | |
|---|---|
| Measures | negated condition that has an else branch |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Decompose Conditional |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/shapes.ts`, line 51:

```ts
export function gate(ready: boolean, retries: number) {
  if (!ready) {
    return 0;
```

rqx says: invert the condition and swap the branches so the common case reads first

