# Change

[Back to the catalogue](README.md)

## `hotspot`

the cost of what is wrong here is paid on every change, and this file changes more than any other, so it is where cleaning up buys the most

| | |
|---|---|
| Measures | a file that is edited often and is not clean |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Class / Extract Method |
| Tuned by | `thresholds.hotspot_commits` |

From `fixtures/typescript/index.ts`, line 1:

```ts
export { add, describe } from "./clean";
export { tooManyParameters, deeplyNested, Overgrown, SameAsFeeder, Formatting, Catalogue, crowdedScope, widelyGuarded, manyPaths, spreadOut, loops } from "./bloat";
```

Suggested fix: clean this one first: whatever is wrong here is paid for on every change

## `single-author-file`

nobody else has read it closely enough to change it, so whatever it knows leaves with the person who wrote it

| | |
|---|---|
| Measures | a file with a history of its own and only one name in it |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Tuned by | `thresholds.sole_author_commits` |

From `fixtures/typescript/flow.ts`, line 1:

```ts
export function restated(ready: boolean): boolean {
  if (ready) {
```

Suggested fix: get a second reader onto it before the first one moves on

## `shotgun-surgery`

one decision is spread across files that must be edited in lockstep, and nothing in the code says so, so the day someone edits four of the five is the day it breaks

| | |
|---|---|
| Measures | files that git history shows almost always change together |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Class / Move Method |

## `divergent-change`

this file is edited for many unrelated reasons, which is the observable signature of more than one responsibility living in it

| | |
|---|---|
| Measures | a file with many commits and no single frequent co-change partner |
| Reported as | info, low confidence |
| Reads | the syntax tree alone |
| Fix | Extract Class |
| Default | off; name it under `rules.enabled` |

## `parallel-inheritance`

every new case means a new subclass in both trees, and the day someone adds only one the two hierarchies quietly stop lining up

| | |
|---|---|
| Measures | two class hierarchies whose subclasses are named for the same set of things |
| Reported as | info, low confidence |
| Reads | the import graph across every file |
| Fix | Move Method / Move Field / Collapse Hierarchy |
| Default | off; name it under `rules.enabled` |
| Not in Go | hierarchies need inheritance, and Go has none to build one from |

From `fixtures/typescript/families.ts`, line 30:

```ts

export class RailLogistics extends Logistics {
  createTransport(): Transport {
```

Suggested fix: fold what varies into one hierarchy, so adding a case means one subclass rather than one in `Logistics` and one in `Transport`

## `stale-todo`

a note nobody has touched in this long is not a plan, and every reader since has had to decide for themselves whether it still applies

| | |
|---|---|
| Measures | days since the line holding a note about work still to do was last written |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Tuned by | `thresholds.todo_age_days` |

## `orphan-todo`

there is no ticket to close and nobody to ask, so it stays until somebody deletes it without knowing what it was for

| | |
|---|---|
| Measures | a note about work still to do with nobody attached to it |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/hierarchy.ts`, line 43:

```ts
export function halved(total: number): number {
  // TODO: work out what happens to the remainder
  return 7 / 2 + total;
```

Suggested fix: put a ticket or a name on it, or do the thing and delete it

