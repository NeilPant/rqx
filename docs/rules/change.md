# Change

[Back to the catalogue](README.md)

## `hotspot`

the cost of the problems in this file is paid on every change, and this file changes more often than any other, so cleaning it up has the largest effect

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

no one else has read the file closely enough to change it, so its knowledge is lost when its author leaves

| | |
|---|---|
| Measures | a file with its own history and a single author |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Share Ownership |
| Tuned by | `thresholds.sole_author_commits` |

From `fixtures/typescript/flow.ts`, line 1:

```ts
export function restated(ready: boolean): boolean {
  if (ready) {
```

Suggested fix: get a second reader onto it before the first one moves on

## `shotgun-surgery`

one decision is spread across files that must be edited together, and nothing in the code says so, so an edit to only some of them breaks the behaviour

| | |
|---|---|
| Measures | files that git history shows almost always change together |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Class / Move Method |

## `divergent-change`

the file is edited for many unrelated reasons, which indicates that more than one responsibility lives in it

| | |
|---|---|
| Measures | a file with many commits and no single frequent co-change partner |
| Reported as | info, low confidence |
| Reads | the syntax tree alone |
| Fix | Extract Class |
| Default | off; name it under `rules.enabled` |

## `parallel-inheritance`

each new case needs a new subclass in both hierarchies, and adding only one leaves the two hierarchies out of step

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

a note that has not been touched for this long is not a plan, and every reader since has had to decide whether it still applies

| | |
|---|---|
| Measures | days since the line holding a note about work still to do was last written |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Tuned by | `thresholds.todo_age_days` |

## `orphan-todo`

there is no ticket to close and no one to ask, so it stays until someone deletes it without knowing what it was for

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

Suggested fix: add a ticket or an owner, or do the work and delete the note

