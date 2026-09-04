# Naming

[Back to the catalogue](README.md)

## `mixed-conventions`

a reader who has to guess which convention a name follows cannot skim, and the mixture spreads every time somebody copies a neighbouring line

| | |
|---|---|
| Measures | camelCase and snake_case names declared in one file |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (rewrite) |
| Fix | Rename Variable |

From `fixtures/typescript/conventions.ts`, line 1:

```ts
const retry_limit = 3;
const retryBackoff = 250;
```

rqx says: pick the convention the rest of the project uses and rename to match

## `vague-function-name`

the name has to be read past to learn anything, so every call site forces a trip to the definition

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

rqx says: rename `process` after what it does to what

## `misleading-pattern-name`

a pattern name is a promise about the interface, and a reader who trusts it goes looking for members that are not there

| | |
|---|---|
| Measures | a class named Factory or Builder with no method that makes or builds |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Method / Extract Class |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/lifecycle.ts`, line 24:

```ts

export class ReportFactory {
  private rows: string[] = [];
```

rqx says: rename `ReportFactory` after what it holds, or give it the members a factory has

## `inconsistent-vocabulary`

a reader who has met `fetchUser` cannot tell whether `getUser` is the same thing or a second one, so both have to be read before either is used

| | |
|---|---|
| Measures | two interchangeable verbs used for the same object in one file |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Method |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/vocabulary.ts`, line 1:

```ts
export function getAccount(id: string): string {
  return id;
```

rqx says: pick one verb for the idea and rename the rest to match

## `meaningless-name`

a name that describes nothing forces every reader to re-derive the value's purpose from its uses

| | |
|---|---|
| Measures | identifier matched against a curated non-name set |
| Reported as | info, low confidence |
| Reads | the syntax tree alone |
| Fix | Rename Variable |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/names.ts`, line 2:

```ts
export function process(rows: number[]) {
  const data = rows.filter(Boolean);
  const temp2 = data.length;
```

rqx says: rename `data` to what it holds

## `short-name-wide-scope`

a one-letter name is readable while its whole life fits on screen, and stops being readable the moment it does not

| | |
|---|---|
| Measures | binding of one or two characters living across many lines |
| Reported as | info, low confidence |
| Reads | resolved scopes and symbols |
| Fix | Rename Variable |
| Tuned by | `thresholds.short_name_scope_lines` |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/shapes.ts`, line 59:

```ts
export function sweep(rows: number[]) {
  let n = 0;
  const seen: number[] = [];
```

rqx says: give `n` a name that survives the distance to its last use

## `hungarian-notation`

the type is already in the declaration and checked by the compiler, so the prefix is a second copy that nothing keeps honest

| | |
|---|---|
| Measures | identifier carrying a type prefix, or an interface named with a leading I |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Rename Variable |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/shapes.ts`, line 23:

```ts
}
export interface IShipment { strLabel: string; }

```

rqx says: drop the prefix; the declaration already states the type and the compiler checks it

