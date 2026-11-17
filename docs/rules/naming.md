# Naming

[Back to the catalogue](README.md)

## `mixed-conventions`

a reader who has to guess which convention a name follows cannot skim, and the mixture spreads each time a neighbouring line is copied

| | |
|---|---|
| Measures | camelCase and snake_case names declared in one file |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Variable |

From `fixtures/typescript/conventions.ts`, line 1:

```ts
const retry_limit = 3;
const retryBackoff = 250;
```

Suggested fix: pick the convention the rest of the project uses and rename to match

## `vague-function-name`

the name does not say what the function does, so each call site requires reading the definition

| | |
|---|---|
| Measures | a function named with a bare verb and no object |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Method |

From `fixtures/typescript/names.ts`, line 1:

```ts
export function process(rows: number[]) {
  const data = rows.filter(Boolean);
```

Suggested fix: rename `process` with a verb and the object it acts on

## `misleading-pattern-name`

a pattern name describes an interface, and a reader who trusts it looks for members that do not exist

| | |
|---|---|
| Measures | a class named Factory or Builder with no method that makes or builds |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Method / Extract Class |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/lifecycle.ts`, line 24:

```ts

export class ReportFactory {
  private rows: string[] = [];
```

Suggested fix: rename `ReportFactory` after what it holds, or give it the members a factory has

## `inconsistent-vocabulary`

a reader who has seen `fetchUser` cannot tell whether `getUser` is the same operation or a different one, so both have to be read before either is used

| | |
|---|---|
| Measures | two interchangeable verbs used for the same object in one file |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Method |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/vocabulary.ts`, line 1:

```ts
export function getAccount(id: string): string {
  return id;
```

Suggested fix: pick one verb for the idea and rename the rest to match

## `meaningless-name`

a name that describes nothing requires each reader to work out the value's purpose from its uses

| | |
|---|---|
| Measures | identifier matching a fixed list of non-descriptive names |
| Reported as | info, low confidence |
| Reads | the syntax tree alone |
| Fix | Rename Variable |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/names.ts`, line 2:

```ts
export function process(rows: number[]) {
  const data = rows.filter(Boolean);
  const temp2 = data.length;
```

Suggested fix: rename `data` to what it holds

## `short-name-wide-scope`

a one-letter name is readable while its whole lifetime fits on screen, and stops being readable when it does not

| | |
|---|---|
| Measures | binding of one or two characters living across many lines |
| Reported as | info, low confidence |
| Reads | resolved scopes and symbols |
| Fix | Rename Variable |
| Tuned by | `thresholds.short_name_scope_lines` |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/shapes.ts`, line 59:

```ts
export function sweep(rows: number[]) {
  let n = 0;
  const seen: number[] = [];
```

Suggested fix: give `n` a name that stays meaningful at its last use

## `hungarian-notation`

the type is already in the declaration and checked by the compiler, so the prefix is a second copy that nothing keeps accurate

| | |
|---|---|
| Measures | identifier carrying a type prefix, or an interface named with a leading I |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Variable |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/shapes.ts`, line 23:

```ts
}
export interface IShipment { strLabel: string; }

```

Suggested fix: drop the prefix; the declaration already states the type and the compiler checks it

## `negated-name`

every test of the name that is itself negated has to be inverted twice by the reader before the condition has a meaning

| | |
|---|---|
| Measures | a name that states what a thing is not |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Rename Variable |

From `fixtures/typescript/names.ts`, line 8:

```ts
export function guardsTheGate(rows: number[]): number {
  const notReady = rows.length === 0;
  const Promise = rows.length;
```

Suggested fix: name the positive and let the call sites negate it

## `shadows-builtin`

the original is unreachable for the rest of the scope, and a later line that expects it fails in a place that names neither

| | |
|---|---|
| Measures | a declared name the language or its standard library already owns |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Rename Variable |

From `fixtures/typescript/names.ts`, line 9:

```ts
  const notReady = rows.length === 0;
  const Promise = rows.length;
  return notReady ? 0 : Promise;
```

Suggested fix: give it a name specific to this project

## `boolean-not-a-question`

a reader has to open the definition to learn which way round the answer runs, and a name phrased as a question says it in one word

| | |
|---|---|
| Measures | a boolean-valued member whose name is not phrased as a question |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Rename Method |
| Not in JavaScript | a function declares no return type, so nothing says it answers true or false |

From `fixtures/typescript/flow.ts`, line 1:

```ts
export function restated(ready: boolean): boolean {
  if (ready) {
```

Suggested fix: name it as the question it answers, so a condition reads as one

## `name-repeats-type`

the adjacent declaration already says what it is, so the word is read twice and becomes wrong when the type changes

| | |
|---|---|
| Measures | a name ending in the word for what it holds |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Rename Variable |

From `fixtures/typescript/conventions.ts`, line 18:

```ts
export class Ledger {
  private entryList: number[] = [];
  private label = "ledger";
```

Suggested fix: name what it is for, and let the declaration say what it is

## `plural-mismatch`

a reader relies on the name to say whether there is one value or many, and here it says the opposite

| | |
|---|---|
| Measures | a name whose number disagrees with what it was given |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Rename Variable |

From `fixtures/typescript/conventions.ts`, line 30:

```ts

export const tier = [1, 2, 3];

```

Suggested fix: make the name agree with how many there are

