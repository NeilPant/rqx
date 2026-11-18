# Flow

[Back to the catalogue](README.md)

## `redundant-else`

the branch above already exits, so the code after the `else` is indented one level deeper than its logic requires

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

Suggested fix: drop the `else` and outdent its body; the branch above already returned

## `collapsible-if`

this is one condition written as two, so a reader has to follow a branch that never branches

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

Suggested fix: join the two conditions with `&&` and keep one branch

## `boolean-from-branch`

the condition is already the result, so branching on it to produce the same boolean takes four lines to say what one says

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

Suggested fix: return the condition itself

## `literal-comparison`

comparing a boolean to a boolean literal asks a question the value already answers, and suggests the value might be something other than a boolean

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

Suggested fix: use the value directly, or negate it

## `self-comparison`

the result is decided before the comparison runs, so either it does nothing or one side was meant to be a different expression

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

Suggested fix: one side was meant to be something else; decide which

## `identical-branches`

the decision changes nothing, so either it is not a decision or one branch was meant to differ

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

Suggested fix: keep one body and delete the branch, or make the arms differ

## `empty-block`

a decision was written and no action was taken, which reads as unfinished work or an accidentally inverted condition

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

Suggested fix: write what should happen, or delete the branch

## `duplicate-condition`

the second branch cannot be reached, so whatever it was written to do has never run

| | |
|---|---|
| Measures | the same condition tested twice in one chain of branches |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 177:

```ts
    return 2;
  } else if (kind === "a") {
    return 3;
```

Suggested fix: decide which of the two arms was meant, and delete the other

## `silent-fallthrough`

the arm below also runs, and nothing here says whether that was intended

| | |
|---|---|
| Measures | a switch arm that does work and then continues into the next one |
| Reported as | error, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |
| Not in Go | an arm ends where the next begins unless it says otherwise |
| Not in Python | an arm ends where the next begins |

From `fixtures/typescript/flow.ts`, line 193:

```ts
  switch (kind) {
    case 1:
      globalThis.console.warn("one");
    case 2:
```

Suggested fix: end the arm, or say in the code that falling through is the point

## `nested-ternary`

two conditions folded into one expression have to be unfolded by the reader before either can be checked

| | |
|---|---|
| Measures | a conditional expression written inside another one |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Decompose Conditional / Extract Variable |
| Not in Go | Go has no conditional expression |

From `fixtures/typescript/flow.ts`, line 137:

```ts
export function tierOf(score: number): string {
  return score > 90 ? "gold" : score > 50 ? "silver" : "bronze";
}
```

Suggested fix: give the inner condition a named value, or write the whole thing as statements

## `ambiguous-precedence`

the language defines which operator binds tighter but the reader has to remember it, and parentheses remove the question at no cost

| | |
|---|---|
| Measures | two different logical operators in one condition with no parentheses between them |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Variable / Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 141:

```ts
export function admits(rank: number, guests: number, staff: number): boolean {
  return rank > 0 && guests > 0 || staff > 0;
}
```

Suggested fix: put the tighter half in parentheses, or name it

## `negated-comparison`

the language already has an operator for this comparison, and the shorter form can be read without inverting anything

| | |
|---|---|
| Measures | a negation wrapped around an equality test |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |

From `fixtures/typescript/flow.ts`, line 145:

```ts
export function changed(before: string, after: string): boolean {
  return !(before === after);
}
```

Suggested fix: invert the operator and drop the negation

## `no-default-branch`

a value the switch does not anticipate passes through it unhandled, and nothing in the code says whether that is intended

| | |
|---|---|
| Measures | a switch with enough arms to count as one and no arm for the values it does not name |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion / Replace Conditional with Polymorphism |

From `fixtures/typescript/flow.ts`, line 149:

```ts
export function labelOf(state: number): string {
  switch (state) {
    case 0:
      return "new";
    case 1:
      return "open";
    case 2:
      return "closed";
  }
  return "unknown";
```

Suggested fix: add a default arm that handles unnamed values, even if it only raises

## `loop-runs-once`

it is written as repetition and runs once, so either the exit is misplaced or the loop is a lookup written as a loop

| | |
|---|---|
| Measures | a loop whose body always leaves before a second iteration |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code / Replace Nested Conditional with Guard Clauses |

From `fixtures/typescript/flow.ts`, line 162:

```ts
  for (const item of items) {
    return item;
  }
```

Suggested fix: drop the loop and take the first match, or move the exit behind the condition that should end it

## `redundant-jump`

control reaches that point anyway, so the keyword makes a reader look for a condition that does not exist

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

Suggested fix: delete it; control reaches that point without it

## `tiny-switch`

a switch is a table a reader scans, and a switch with two arms is an `if` written in four more lines

| | |
|---|---|
| Measures | arms in a switch, below the count that justifies a switch |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Consolidate Conditional |
| Tuned by | `thresholds.min_cases` |
| Default | off; name it under `rules.enabled` |

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

Suggested fix: write it as an `if`, or add the arms that justify a switch

## `nested-then`

the order is real but is not written as an order, so a reader follows it by tracing which callback holds which

| | |
|---|---|
| Measures | a callback that starts another wait inside itself |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Replace Algorithm |
| Not in Go | waiting is a channel receive, not a callback |
| Not in Python | waiting is written as a statement |
| Not in C++ | waiting is a call that returns |

From `fixtures/typescript/hierarchy.ts`, line 32:

```ts
export function chained(rows: number[]): Promise<number> {
  return Promise.resolve(rows.length).then((count) => {
    return Promise.resolve(count).then((inner) => inner + 1);
  });
}
```

Suggested fix: await each in turn where the language allows it, so the order of execution is the order of the lines

