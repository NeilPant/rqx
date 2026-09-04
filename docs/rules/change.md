# Change

[Back to the catalogue](README.md)

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
| Default | off; name it under `[rules] enabled` |

## `parallel-inheritance`

every new case means a new subclass in both trees, and the day someone adds only one the two hierarchies quietly stop lining up

| | |
|---|---|
| Measures | two class hierarchies whose subclasses are named for the same set of things |
| Reported as | info, low confidence |
| Reads | the import graph across every file |
| Fix | Move Method / Move Field / Collapse Hierarchy |
| Default | off; name it under `[rules] enabled` |
| Not in Go | hierarchies need inheritance, and Go has none to build one from |

From `fixtures/typescript/families.ts`, line 30:

```ts

export class RailLogistics extends Logistics {
  createTransport(): Transport {
```

rqx says: fold what varies into one hierarchy, so adding a case means one subclass rather than one in `Logistics` and one in `Transport`

