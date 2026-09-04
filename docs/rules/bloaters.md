# Bloaters

[Back to the catalogue](README.md)

## `long-function`

a body this long is doing several things, so a reader has to hold all of them at once to change any one of them

| | |
|---|---|
| Measures | statement count in a function body |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Smell | Long Method |
| Clean Code | G30 Functions Should Do One Thing |
| Fix | Extract Method |
| Tuned by | `thresholds.long_function_statements` |

From `fixtures/typescript/long.ts`, line 1:

```ts
export function manySteps() {
  let total = 0;
```

rqx says: split `manySteps` into named steps, each one nameable without the word "and"

## `cognitive-complexity`

every nested decision point multiplies the paths a reader has to trace, and untraced paths are where regressions survive review

| | |
|---|---|
| Measures | SonarSource cognitive complexity |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Smell | Long Method |
| Clean Code | G34 Functions Should Descend Only One Level of Abstraction |
| Fix | Decompose Conditional / Extract Method |
| Tuned by | `thresholds.cognitive_complexity` |

From `fixtures/typescript/reconcile.ts`, line 7:

```ts

  async reconcileBatch(batch: Batch, tolerance: number, dryRun: boolean, retries: number, verbose: boolean) {
    let settled = 0;
```

rqx says: lift the conditions in `reconcileBatch` into named predicates, then extract the deepest block into its own function

## `deep-nesting`

at this depth the conditions that led here are off-screen, so the preconditions of the innermost code are invisible where it matters

| | |
|---|---|
| Measures | maximum block nesting depth |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Smell | Long Method |
| Clean Code | G34 Functions Should Descend Only One Level of Abstraction |
| Fix | Replace Nested Conditional with Guard Clauses |
| Tuned by | `thresholds.nesting_depth` |

From `fixtures/typescript/bloat.ts`, line 5:

```ts

export function deeplyNested(rows: number[]) {
  for (const row of rows) {
```

rqx says: invert the outer conditions of `deeplyNested` into early returns so the innermost code sits at depth 1

## `long-parameter-list`

callers have to remember an order this long, and same-typed neighbours make a silent transposition possible at every call site

| | |
|---|---|
| Measures | parameter count |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Smell | Long Parameter List |
| Clean Code | F1 Too Many Arguments |
| Fix | Introduce Parameter Object / Preserve Whole Object |
| Tuned by | `thresholds.max_params` |

From `fixtures/typescript/bloat.ts`, line 1:

```ts
export function tooManyParameters(a: number, b: number, c: number, d: number, e: number) {
  return a + b + c + d + e;
```

rqx says: group the 5 parameters of `tooManyParameters` into one options object, or pass the object the caller already has

## `large-class`

a class this size has more than one reason to change, so unrelated work lands in the same file and the same merge conflicts

| | |
|---|---|
| Measures | member count and body statement count |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Smell | Large Class |
| Clean Code | Ch.10 The Single Responsibility Principle |
| Fix | Extract Class / Extract Subclass |
| Tuned by | `thresholds.class_members`, `thresholds.class_statements` |

From `fixtures/typescript/bloat.ts`, line 18:

```ts

export class Overgrown {
  a1 = 1; a2 = 1; a3 = 1; a4 = 1; a5 = 1; a6 = 1; a7 = 1; a8 = 1; a9 = 1; a10 = 1;
```

rqx says: extract the fields of `Overgrown` that only a subset of its 3 methods touch into their own class

## `data-clump`

these values already travel together everywhere, so they are a concept the code has not named yet, and every new use has to rediscover it

| | |
|---|---|
| Measures | the same group of three or more parameter names recurring across signatures |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Smell | Data Clumps |
| Clean Code | G22 Make Logical Dependencies Physical |
| Fix | Introduce Parameter Object / Extract Class |
| Tuned by | `thresholds.data_clump_size`, `thresholds.data_clump_sites` |

From `fixtures/typescript/shapes.ts`, line 39:

```ts
}
export function notify(userId: string, channel: string, locale: string) { return userId; }
export function enqueue(userId: string, channel: string, locale: string) { return channel; }
```

rqx says: give (channel, locale, userId) a name and pass that object instead

## `primitive-obsession`

a raw string or number carries no rules of its own, so every place that uses it re-implements the same validation, and one of them gets it wrong

| | |
|---|---|
| Measures | a type alias that is a union of three or more string literals |
| Reported as | info, low confidence |
| Reads | resolved scopes and symbols |
| Smell | Primitive Obsession |
| Clean Code | G25 Replace Magic Numbers with Named Constants |
| Fix | Replace Type Code with Class / Introduce Parameter Object |
| Default | off; name it under `[rules] enabled` |
| Not in Go | Go has no union types, so the string-literal alias this measures cannot be written |
| Not in Java | Java has no union types, so the string-literal alias this measures cannot be written |
| Not in C++ | C++ has no union types, so the string-literal alias this measures cannot be written |

From `fixtures/typescript/shapes.ts`, line 1:

```ts
export type Channel = "email" | "sms" | "push";

```

rqx says: give `Channel` a type that owns its rules, so validation lives with the data instead of at every use

## `obscured-expression`

nothing in the expression is named, so a reader has to re-derive every intermediate step the author already worked out

| | |
|---|---|
| Measures | operations packed into one returned expression beyond the limit |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Smell | no direct catalogue entry |
| Clean Code | G16 Obscured Intent |
| Fix | Extract Variable / Extract Method |
| Tuned by | `thresholds.expression_operations` |

From `fixtures/typescript/config.ts`, line 13:

```ts
export function score(a: number, b: number, c: number, d: number): number {
  return (a + b) * (c - d) + Math.max(a, b) + Math.min(c, d) - (a * d);
}
```

rqx says: lift the steps into named locals, so the expression reads as its own summary

