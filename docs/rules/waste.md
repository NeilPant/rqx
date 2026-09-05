# Waste

[Back to the catalogue](README.md)

## `duplicate-code`

a fix applied to one copy leaves the others wrong, and nothing in the code points from the copy you edited to the ones you did not

| | |
|---|---|
| Measures | identical normalized-AST block hash across sites |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Pull Up Method |
| Tuned by | `thresholds.clone_min_statements`, `thresholds.clone_min_nodes` |

From `fixtures/typescript/flow.ts`, line 173:

```ts
export function asksTwice(kind: string): number {
  if (kind === "a") {
    return 1;
  } else if (kind === "b") {
    return 2;
  } else if (kind === "a") {
    return 3;
  }
  return 0;
}
```

rqx says: extract the shared block into one function and call it from all 2 sites

## `unused-export`

an export nobody imports still has to be read, typechecked and kept working during every refactor that touches it

| | |
|---|---|
| Measures | exported symbol with no importer in the resolved module graph |
| Reported as | warning, verified |
| Reads | the import graph across every file |
| Fix | Remove Dead Code |

From `fixtures/typescript/dead-export.ts`, line 7:

```ts

export function neverImported(value: number) {
  return value * 2;
```

rqx says: delete `neverImported`, or add its module to `entrypoints` if something outside the project imports it

## `unused-file`

nothing in the build reaches this file, so every hour spent maintaining it buys nothing

| | |
|---|---|
| Measures | module reachable from no entry point, with no importer |
| Reported as | warning, verified |
| Reads | the import graph across every file |
| Fix | Delete File |

From `fixtures/typescript/orphan.ts`, line 1:

```ts
export function noOneImportsThisFile() {
  return "dead";
```

rqx says: delete the file, or add it to `entrypoints` if it is reached from outside the project

## `unused-parameter`

every caller is passing a value that is read nowhere, so the argument list is teaching readers something untrue about the function

| | |
|---|---|
| Measures | declared parameter with no resolved reference in its own scope |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (rewrite) |
| Fix | Remove Parameter |

From `fixtures/typescript/families.ts`, line 130:

```ts
export class Printer {
  visitLiteral(node: Literal) {
    return "1";
```

rqx says: drop `node` from `visitLiteral` and from its call sites

## `unused-local`

the value is computed and thrown away, which either wastes the work or hides the fact that somebody meant to use it

| | |
|---|---|
| Measures | local binding with no resolved reference |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 8:

```ts
export function pricing(cart: number[], applyDiscount: boolean): number {
  let neverRead = cart.length;
  let total = cart.reduce((a, b) => a + b, 0);
```

rqx says: delete `neverRead`, or use it if the work computing it still matters

## `unused-private-member`

a private member can only be used from inside its class, so one nothing in the class touches is unreachable by construction

| | |
|---|---|
| Measures | private class member never referenced inside its own class |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |
| Not in Go | Go scopes visibility to the package, not the type, so a field unused in one file may be read by another beside it |

From `fixtures/typescript/patterns.ts`, line 195:

```ts
export class Settings {
  private static instance: Settings;
  private constructor() {}
```

rqx says: delete `instance`; nothing inside `Settings` can reach it

## `unreachable-code`

this code cannot run, so whatever it was meant to do is not happening and has not been happening

| | |
|---|---|
| Measures | statement following an unconditional return, throw, break or continue |
| Reported as | error, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 30:

```ts
  return value + seed;
  value = 1;
}
```

rqx says: delete it, or move it above the exit if it was meant to run

## `commented-out-code`

nobody dares delete it because nobody knows if it matters, so it sits there rotting while version control already remembers it perfectly

| | |
|---|---|
| Measures | consecutive comment lines that read as statements rather than prose |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |
| Tuned by | `thresholds.commented_code_lines` |

From `fixtures/typescript/hygiene.ts`, line 11:

```ts

  // const legacy = total * 0.2;
  // return legacy + shipping;

```

rqx says: delete it; git already has it, and a comment cannot say why it is still here

## `lazy-class`

the class costs a file, an import and a name, and does not yet do enough to earn any of them

| | |
|---|---|
| Measures | class with at most one member and no heritage |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Inline Class |

From `fixtures/typescript/shapes.ts`, line 4:

```ts
export class Coordinate { x = 0; y = 0; get pair() { return [this.x, this.y]; } }
export class Marker { label = "x"; }
export class Facade {
```

rqx says: fold `Marker` into its only caller until it earns its own file

## `data-class`

the behaviour that belongs with this data lives somewhere else, so the rules about the data are enforced everywhere except where the data is

| | |
|---|---|
| Measures | class whose members are all fields or trivial accessors |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Move Method / Encapsulate Field |

From `fixtures/typescript/shapes.ts`, line 3:

```ts

export class Coordinate { x = 0; y = 0; get pair() { return [this.x, this.y]; } }
export class Marker { label = "x"; }
```

rqx says: move the logic that operates on `Coordinate`'s fields onto `Coordinate`

## `speculative-generality`

it was built for a case that never arrived, and until it does it is only an extra indirection every reader has to step through. Interfaces are not reported: TypeScript's are structural, so nothing implementing one says nothing about whether it is speculative

| | |
|---|---|
| Measures | abstract class with no subclass anywhere in the project |
| Reported as | info, verified |
| Reads | the import graph across every file |
| Fix | Collapse Hierarchy / Inline Class |

From `fixtures/typescript/flow.ts`, line 202:

```ts

export abstract class Settled {
  run(): number {
```

rqx says: inline `Settled` into its single user until a second one arrives

## `empty-catch`

the failure is swallowed silently, so the first sign of it is wrong data somewhere far away with nothing in the logs to connect the two

| | |
|---|---|
| Measures | catch clause with an empty body |
| Reported as | error, verified |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |
| Not in Go | Go has no exceptions and so no catch clause; an ignored error is a different shape this rule does not measure |

From `fixtures/typescript/hygiene.ts`, line 21:

```ts
    return total + 4999;
  } catch (error) {
  }
  return 0;
```

rqx says: log it, rethrow it, or write the reason ignoring it is correct

## `magic-number`

the number encodes a decision nobody wrote down, so the next reader cannot tell whether changing it is safe or what else shares it

| | |
|---|---|
| Measures | unnamed numeric literal outside a declaration, excluding conventional values |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Magic Number with Constant / Extract Variable |
| Tuned by | `thresholds.magic_number_repeats` |

From `fixtures/typescript/flow.ts`, line 34:

```ts
  const scale = a * b;
  for (const step of [1, 2, 3]) {
    if (step > scale) {
```

rqx says: name `3` once and refer to the name at all 4 sites

## `mixed-language-file`

a second language in the file gets none of the tooling the first one has, so nothing checks it until it fails at run time

| | |
|---|---|
| Measures | SQL or HTML markup inside a template literal |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Class / Extract Variable |

From `fixtures/typescript/conventions.ts`, line 5:

```ts
export function query(userId: string): string {
  return `SELECT id, name FROM users WHERE id = '${userId}'`;
}
```

rqx says: move the SQL into its own file, where its own tooling can read it

## `distant-declaration`

everything in between has to be read while still holding this name in mind, and the reader has no way to know it will matter

| | |
|---|---|
| Measures | lines between a local's declaration and the line that first reads it |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Variable / Inline Variable |
| Tuned by | `thresholds.declaration_distance_lines` |

From `fixtures/typescript/queries.ts`, line 16:

```ts
export function summarise(rows: string[]): string {
  const separator = " | ";
  if (rows.length === 0) {
```

rqx says: move `separator` down to where it is first used

## `buried-configuration`

changing where the program points means finding this line first, and nothing about its position suggests it is a setting

| | |
|---|---|
| Measures | a URL or absolute path written below the configured block depth |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Magic Number with Constant / Extract Variable |
| Tuned by | `thresholds.configuration_depth` |

From `fixtures/typescript/config.ts`, line 5:

```ts
      if (retries > 0) {
        return "https://api.example.com/v2/orders";
      }
```

rqx says: lift it to a named constant at the top of the module, or into config

## `journal-comment`

version control already answers who and when, and unlike the comment it is not left behind by the next edit

| | |
|---|---|
| Measures | a comment carrying an author, a date or a change log |
| Reported as | info, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/precision.ts`, line 1:

```ts
/**
 * @author someone
 * Created on 2019-04-02
 */
export function widen(input: any): string {
```

rqx says: delete it; `git log` and `git blame` answer this and stay correct

## `redundant-comment`

it takes space and attention to say what the name has already said, and it is one more thing to keep true when the name changes

| | |
|---|---|
| Measures | a comment whose every word already appears in the name below it |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 128:

```ts

/** Score of user id */
export function scoreOfUser(userId: number): number {
```

rqx says: delete it, or say why `scoreOfUser` exists rather than what it is

## `cross-language-duplicate`

the same rule is written twice in two languages, so the day it changes it changes in one of them and the two quietly disagree

| | |
|---|---|
| Measures | one block's control-flow shape and constants found in another language |
| Reported as | info, low confidence |
| Reads | the syntax tree alone |
| Fix | Extract Shared Module |
| Default | off; name it under `[rules] enabled` |

## `return-of-fresh-local`

the name is read once, by the line under it, and says nothing the expression it holds did not already say

| | |
|---|---|
| Measures | a name introduced on one line and returned on the next |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Inline Variable |

From `fixtures/typescript/flow.ts`, line 168:

```ts
export function freshTotal(rows: number[]): number {
  const total = rows.length * 2;
  return total;
```

rqx says: return the expression, or keep the name because it says something the expression does not

## `empty-method`

a caller that reaches this gets silence and no way to tell it apart from work that succeeded, and the next reader cannot tell whether it is a stub or a decision

| | |
|---|---|
| Measures | a function or method with nothing in its body |
| Reported as | info, verified |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |

From `fixtures/typescript/bloat.ts`, line 21:

```ts
  a11 = 1; a12 = 1; a13 = 1; a14 = 1; a15 = 1; a16 = 1; a17 = 1; a18 = 1; a19 = 1;
  m1() {} m2() {} m3() {}
}
```

rqx says: give it a body, delete it, or make it explicit that doing nothing is the answer here

## `barrel-module`

it adds a name to every path through the project and answers no question of its own, while hiding from every tool which file a name really came from

| | |
|---|---|
| Measures | a file whose every import comes straight back out of it |
| Reported as | info, verified |
| Reads | resolved scopes and symbols |
| Fix | Inline Class / Remove Dead Code |
| Not in Go | a package is the unit of import, and it has no index file |
| Not in Java | a class is imported by its own name |
| Not in C++ | a header is included by its own path |
| Not in Python | a package re-exports through `__all__`, which names rather than forwards |

From `fixtures/typescript/index.ts`, line 1:

```ts
export { add, describe } from "./clean";
export { tooManyParameters, deeplyNested, Overgrown, SameAsFeeder, Formatting, Catalogue, crowdedScope, widelyGuarded, manyPaths, spreadOut, loops } from "./bloat";
```

rqx says: let the callers name the file they mean

## `passthrough-override`

it changes nothing and costs a reader a hop, while making the type look like it has an opinion about the member when it has none

| | |
|---|---|
| Measures | a member that overrides something and then calls it unchanged |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code / Collapse Hierarchy |
| Not in Go | there is no inheritance to override |
| Not in C++ | a call to the base is written with the type's own name, which one file cannot resolve |

From `fixtures/typescript/hierarchy.ts`, line 14:

```ts
export class QuietFeeder extends Feeder {
  feed(): string {
    return super.feed();
```

rqx says: delete it, and let the inherited one answer

## `duplicate-function`

one of them was copied from the other, and the next change to either has to find both or leave them disagreeing

| | |
|---|---|
| Measures | two functions in one file written the same way |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Remove Dead Code |

From `fixtures/typescript/hierarchy.ts`, line 25:

```ts

export function secondWay(rows: number[]): number {
  const total = rows.length;
  const doubled = total * 2;
  return doubled + 1;
}

```

rqx says: keep one and call it from the other, or delete the copy

## `dead-store`

the first write did nothing, so either a line meant to read it is missing or the line that wrote it is

| | |
|---|---|
| Measures | a value written and written again with nothing reading the first |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/flow.ts`, line 57:

```ts
export function assigns(a: number, b: number): number {
  let found = 0;
  if ((found = a + b)) {
```

rqx says: delete the write nobody reads, or read it

## `never-reassigned`

a reader has to hold open the possibility that it moves, and it never does, so the declaration is asking for attention it does not need

| | |
|---|---|
| Measures | a name declared as changeable that never changes |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Rename Variable |
| Not in Python | the language has no word for a name that cannot change |
| Not in Go | a short declaration says nothing about whether the name moves |

From `fixtures/typescript/shapes.ts`, line 59:

```ts
export function sweep(rows: number[]) {
  let n = 0;
  const seen: number[] = [];
```

rqx says: declare it as fixed, and let the compiler hold it there

## `discarded-expression`

either the line was meant to do something and does not, or it does nothing and should not be here, and a reader cannot tell which

| | |
|---|---|
| Measures | a statement that computes a value and does nothing with it |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Not in Go | the grammar allows only a call or a receive as a statement, so the compiler rejects this first |
| Not in Java | the grammar allows only a call, an assignment or an increment as a statement |

From `fixtures/typescript/flow.ts`, line 132:

```ts
export function dropped(total: number, other: number): number {
  total === other;
  return total;
```

rqx says: use the value, or delete the line

## `unused-import`

it makes the file look dependent on something it does not use, and the dependency survives every search for who needs that module

| | |
|---|---|
| Measures | a name imported into a file and never read there |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |
| Not in Go | the compiler refuses a file with one |
| Not in C++ | an include may be there for a header that needs it |

From `fixtures/typescript/hygiene.ts`, line 4:

```ts
import { orbit } from "./cycle-a";
import type { Far } from "../../../shared/far";
import { spinAgain } from "./cycle-a";
```

rqx says: delete it; the module it names is not one this file depends on

## `rethrow-only-catch`

it reads as a decision about the failure and makes none, while costing the stack trace in the languages that rebuild one on the way out

| | |
|---|---|
| Measures | a handler whose only statement raises again |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Not in Go | errors are values, so there is no handler to empty |

From `fixtures/typescript/hygiene.ts`, line 58:

```ts
    return orbit(1);
  } catch (error) {
    throw error;
  }
}
```

rqx says: delete the handler and let it propagate, or add what this level knows about the failure

## `placeholder-body`

it is a callable promise with nothing behind it, and every caller that trusts the signature finds that out at runtime

| | |
|---|---|
| Measures | a body whose only statement raises to say the work is still to come |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Not in Go | failure is a returned value, so no statement is a refusal |

From `fixtures/typescript/hygiene.ts`, line 51:

```ts

export function stillToWrite(): number {
  throw new Error("not implemented");
}

```

rqx says: write it, or delete it until there is a caller that needs it

## `passthrough-function`

it adds a name and a stack frame between a caller and the thing it wanted, and the indirection buys nothing while it forwards unchanged

| | |
|---|---|
| Measures | a function whose whole body hands its own arguments to one other call |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Inline Delegation |

From `fixtures/typescript/bloat.ts`, line 86:

```ts

export function loops(rows: number[]): number {
  return loops(rows);
}

```

rqx says: let the caller call the thing directly, unless this name is the seam

## `debug-output`

it was written to be read once by the person debugging, and it is still printing for everybody else

| | |
|---|---|
| Measures | a write to the terminal outside a test, of the kind left behind after debugging |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/hygiene.ts`, line 64:

```ts
export function traced(): void {
  console.log("traced", everything);
}
```

rqx says: delete it, or route it through whatever this project logs with

## `duplicate-import`

two statements bringing in one module means a reader has to check both to know what this file takes from it, and a merge put at least one there without anybody looking

| | |
|---|---|
| Measures | the same module imported by more than one statement in a file |
| Reported as | info, verified |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 5:

```ts
import type { Far } from "../../../shared/far";
import { spinAgain } from "./cycle-a";

```

rqx says: fold these names into the first `./cycle-a` import

