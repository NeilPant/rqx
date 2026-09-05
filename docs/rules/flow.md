# Flow

[Back to the catalogue](README.md)

## `redundant-else`

the branch already left, so everything after it is indented one level deeper than the logic actually is

| | |
|---|---|
| Measures | an `else` following a branch whose every path already exits |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Replace Nested Conditional with Guard Clauses |

From `fixtures/typescript/flow.ts`, line 4:

```ts
    return true;
  } else {
    return false;
  }
}
```

rqx says: drop the `else` and outdent its body; the branch above already returned

## `collapsible-if`

it is one condition written as two, so a reader holds a branch that never branches

| | |
|---|---|
| Measures | an `if` that is the only statement inside another, neither with an else |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/config.ts`, line 3:

```ts
  if (region.length > 0) {
    if (secure) {
      if (retries > 0) {
        return "https://api.example.com/v2/orders";
      }
    }
  }
```

rqx says: join the two conditions with `&&` and keep one branch

## `boolean-from-branch`

the condition is already the answer, and branching on it to restate it is four lines saying what one says

| | |
|---|---|
| Measures | a branch whose arms return opposite boolean literals |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 2:

```ts
export function restated(ready: boolean): boolean {
  if (ready) {
    return true;
  } else {
    return false;
  }
}
```

rqx says: return the condition itself

## `literal-comparison`

comparing a boolean to a boolean asks a question that was already answered, and reads as though the value might be something else

| | |
|---|---|
| Measures | an equality comparison against a boolean literal |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 10:

```ts
export function compared(ready: boolean, count: number): number {
  if (ready === true) {
    return 1;
```

rqx says: use the value directly, or negate it

## `self-comparison`

the answer is decided before it runs, so either the comparison does nothing or one side was meant to be something else

| | |
|---|---|
| Measures | an equality comparison whose operands are written identically |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 13:

```ts
  }
  if (count === count) {
    return 2;
```

rqx says: one side was meant to be something else; decide which

## `identical-branches`

the decision changes nothing, so either it is not a decision or one arm was meant to differ and does not

| | |
|---|---|
| Measures | two arms of a branch whose bodies are written identically |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 20:

```ts
export function sameEitherWay(count: number): number {
  if (count > 10) {
    return 99;
  } else {
    return 99;
  }
}
```

rqx says: keep one body and delete the branch, or make the arms differ

## `empty-block`

a decision was written and then nothing was done about it, which reads as unfinished work or as a condition somebody inverted by accident

| | |
|---|---|
| Measures | a branch or loop body with no statements in it |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Not in Python | Python has no empty block: a body needs `pass` or an ellipsis, which is a statement somebody wrote on purpose |

From `fixtures/typescript/flow.ts`, line 28:

```ts
export function decidedNothing(count: number): void {
  if (count > 10) {
  }
}
```

rqx says: write what should happen, or delete the branch

## `redundant-jump`

control was going there anyway, so the keyword makes a reader stop and look for the condition that is not there

| | |
|---|---|
| Measures | a continue or break as the last statement of a loop body |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/flow.ts`, line 113:

```ts
    total += item;
    continue;
  }
```

rqx says: delete it; control was going there anyway

## `tiny-switch`

a switch is a table a reader scans, and one with two arms is an if spelled in four more lines

| | |
|---|---|
| Measures | arms in a switch, below the count that earns the shape |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |
| Tuned by | `thresholds.min_cases` |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/flow.ts`, line 119:

```ts
export function twoArmed(kind: string): number {
  switch (kind) {
    case "one":
      return 1;
    default:
      return 0;
  }
}
```

rqx says: write it as an if, or give the switch the arms that justify it

