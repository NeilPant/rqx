# Size

[Back to the catalogue](README.md)

## `long-function`

a body this long is doing several things, so a reader has to hold all of them at once to change any one of them

| | |
|---|---|
| Measures | statement count in a function body |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Tuned by | `thresholds.long_function_statements` |

From `fixtures/typescript/long.ts`, line 1:

```ts
export function manySteps() {
  let total = 0;
```

Suggested fix: split `manySteps` into named steps, each one nameable without the word "and"

## `cognitive-complexity`

every nested decision point multiplies the paths a reader has to trace, and untraced paths are where regressions survive review

| | |
|---|---|
| Measures | SonarSource cognitive complexity |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional / Extract Method |
| Tuned by | `thresholds.cognitive_complexity` |

From `fixtures/typescript/bloat.ts`, line 63:

```ts

export function manyPaths(kind: string, rows: number[]): number {
  if (kind === "a") return 1;
```

Suggested fix: lift the conditions in `manyPaths` into named predicates, then extract the deepest block into its own function

## `deep-nesting`

at this depth the conditions that led here are off-screen, so the preconditions of the innermost code are invisible where it matters

| | |
|---|---|
| Measures | maximum block nesting depth |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Nested Conditional with Guard Clauses |
| Tuned by | `thresholds.nesting_depth` |

From `fixtures/typescript/bloat.ts`, line 5:

```ts

export function deeplyNested(rows: number[]) {
  for (const row of rows) {
```

Suggested fix: invert the outer conditions of `deeplyNested` into early returns so the innermost code sits at depth 1

## `long-parameter-list`

callers have to remember an order this long, and same-typed neighbours make a silent transposition possible at every call site

| | |
|---|---|
| Measures | parameter count |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Parameter Object / Pass Whole Object |
| Tuned by | `thresholds.max_params` |

From `fixtures/typescript/bloat.ts`, line 1:

```ts
export function tooManyParameters(a: number, b: number, c: number, d: number, e: number) {
  return a + b + c + d + e;
```

Suggested fix: group the 5 parameters of `tooManyParameters` into one options object, or pass the object the caller already has

## `large-class`

a class this size has more than one reason to change, so unrelated work lands in the same file and the same merge conflicts

| | |
|---|---|
| Measures | member count and body statement count |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Class / Extract Subclass |
| Tuned by | `thresholds.class_members`, `thresholds.class_statements` |

From `fixtures/typescript/bloat.ts`, line 18:

```ts

export class Overgrown {
  a1 = 1; a2 = 1; a3 = 1; a4 = 1; a5 = 1; a6 = 1; a7 = 1; a8 = 1; a9 = 1; a10 = 1;
```

Suggested fix: extract the fields of `Overgrown` that only a subset of its 3 methods touch into their own class

## `data-clump`

these values already travel together everywhere, so they are a concept the code has not named yet, and every new use has to rediscover it

| | |
|---|---|
| Measures | the same group of three or more parameter names recurring across signatures |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Introduce Parameter Object / Extract Class |
| Tuned by | `thresholds.data_clump_size`, `thresholds.data_clump_sites` |

From `fixtures/typescript/shapes.ts`, line 39:

```ts
}
export function notify(userId: string, channel: string, locale: string) { return userId; }
export function enqueue(userId: string, channel: string, locale: string) { return channel; }
```

Suggested fix: give (channel, locale, userId) a name and pass that object instead

## `primitive-obsession`

a raw string or number carries no rules of its own, so every place that uses it re-implements the same validation, and one of them gets it wrong

| | |
|---|---|
| Measures | a type alias that is a union of three or more string literals |
| Reported as | info, low confidence |
| Reads | resolved scopes and symbols |
| Fix | Replace Type Code with Class / Introduce Parameter Object |
| Default | off; name it under `rules.enabled` |
| Not in Go | Go has no union types, so the string-literal alias this measures cannot be written |
| Not in Java | Java has no union types, so the string-literal alias this measures cannot be written |
| Not in C++ | C++ has no union types, so the string-literal alias this measures cannot be written |
| Not in JavaScript | JavaScript has no type aliases, so the string-literal alias this measures cannot be written |

From `fixtures/typescript/shapes.ts`, line 1:

```ts
export type Channel = "email" | "sms" | "push";

```

Suggested fix: give `Channel` a type that owns its rules, so validation lives with the data instead of at every use

## `obscured-expression`

nothing in the expression is named, so a reader has to re-derive every intermediate step the author already worked out

| | |
|---|---|
| Measures | operations packed into one returned expression beyond the limit |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Variable / Extract Method |
| Tuned by | `thresholds.expression_operations` |

From `fixtures/typescript/config.ts`, line 13:

```ts
export function score(a: number, b: number, c: number, d: number): number {
  return (a + b) * (c - d) + Math.max(a, b) + Math.min(c, d) - (a * d);
}
```

Suggested fix: lift the steps into named locals, so the expression reads as its own summary

## `many-exits`

a reader has to find every way out before they can say what the function answers, and the ones written deep in the body are the easy ones to miss

| | |
|---|---|
| Measures | count of returns, throws and raises in one body, past the guard clauses |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method / Decompose Conditional |
| Tuned by | `thresholds.max_exits` |

From `fixtures/typescript/flow.ts`, line 32:

```ts

export function buriedExits(a: number, b: number): number {
  const scale = a * b;
```

Suggested fix: give the function one exit, or extract the branches that end early

## `wide-switch`

past a certain width the arms stop being a table a reader scans and become a list they have to search

| | |
|---|---|
| Measures | count of arms in one switch or match |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Conditional with Polymorphism / Extract Method |
| Tuned by | `thresholds.max_cases` |

From `fixtures/typescript/flow.ts`, line 65:

```ts
export function wideSwitch(kind: string): number {
  switch (kind) {
    case "a":
      return 1;
    case "b":
      return 2;
    case "c":
    default:
      return 0;
  }
}
```

Suggested fix: give the thing being switched on a type, and let it answer for itself

## `callback-nesting`

the order the steps run in is the opposite of the order they are written in, so following the sequence means reading inside out

| | |
|---|---|
| Measures | depth of function literals declared inside other function literals |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Tuned by | `thresholds.callback_depth` |

From `fixtures/typescript/flow.ts`, line 101:

```ts
      setTimeout(() => {
        setTimeout(() => {
          globalThis.console.log("deep");
        }, 1);
      }, 1);
```

Suggested fix: name the inner steps and call them, so the order they run in is the order they read in

## `cyclomatic-complexity`

it is the number of tests it takes to cover the body, and past a point nobody writes them, so most of the paths have never run

| | |
|---|---|
| Measures | paths through one function: one plus every decision in it |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Decompose Conditional |
| Tuned by | `thresholds.cyclomatic_complexity` |

From `fixtures/typescript/bloat.ts`, line 63:

```ts

export function manyPaths(kind: string, rows: number[]): number {
  if (kind === "a") return 1;
```

Suggested fix: split `manyPaths` so each part has a number of paths somebody would write tests for

## `many-results`

the caller has to know the order to read them, and nothing at either end says what any of them is

| | |
|---|---|
| Measures | values handed back by one return |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Parameter Object / Extract Class |
| Tuned by | `thresholds.max_results` |
| Not in Java | a method answers with one value |
| Not in C++ | a function answers with one value |

From `fixtures/typescript/bloat.ts`, line 83:

```ts
export function spreadOut(rows: number[]): [number, number, number, number] {
  return [rows.length, rows.length, rows.length, rows.length];
}
```

Suggested fix: give them a name apiece by handing back one thing that holds them

## `many-locals`

every one of them is a thing the reader has to keep in mind to the end of the body, and there is a number past which nobody does

| | |
|---|---|
| Measures | distinct names one function body introduces |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method / Extract Variable |
| Tuned by | `thresholds.function_locals` |

From `fixtures/typescript/bloat.ts`, line 44:

```ts

export function crowdedScope(seed: number): number {
  const c1 = seed; const c2 = seed; const c3 = seed; const c4 = seed;
```

Suggested fix: pull the run that uses one group of them into a function of its own

## `wide-try`

the handler is written for one of them and catches all of them, so a failure from any other line arrives somewhere that cannot answer it

| | |
|---|---|
| Measures | statements inside one guarded block |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method / Decompose Conditional |
| Tuned by | `thresholds.try_statements` |
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

Suggested fix: guard the line that can fail, and leave the rest outside

## `oversized-module`

a file this long is several files that have not been separated yet, and every one of them is edited, reviewed and merged as though it were one

| | |
|---|---|
| Measures | statements in one file, across every function in it |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Shared Module / Extract Class |
| Tuned by | `thresholds.module_statements` |

From `fixtures/typescript/bloat.ts`, line 1:

```ts
export function tooManyParameters(a: number, b: number, c: number, d: number, e: number) {
  return a + b + c + d + e;
```

Suggested fix: split it along whatever its functions already group into

