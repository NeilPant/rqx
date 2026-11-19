# Waste

[Back to the catalogue](README.md)

## `duplicate-code`

a fix applied to one copy leaves the others unfixed, and nothing in the code links the edited copy to the others

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

Suggested fix: extract the shared block into one function and call it from all 2 sites

## `reimplemented-existing`

a second copy of an existing function has to be changed together with the original, and nothing in the code connects the two

| | |
|---|---|
| Measures | added function whose normalised body matches a function that exists at the base revision |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Algorithm / Extract Shared Module |
| Tuned by | `thresholds.clone_min_statements`, `thresholds.clone_min_nodes` |

## `unused-export`

an export that nothing imports still has to be read, type-checked and kept working during every refactoring that touches it

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

Suggested fix: delete `neverImported`, or add its module to `entrypoints` if something outside the project imports it

## `unused-file`

nothing in the build reaches this file, so the effort of maintaining it has no effect on the program

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

Suggested fix: delete the file, or add it to `entrypoints` if it is reached from outside the project

## `unused-parameter`

every caller passes a value that is never read, so the parameter list misdescribes the function

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

Suggested fix: drop `node` from `visitLiteral` and from its call sites

## `unused-local`

the value is computed and discarded, which either wastes the work or hides a missing use

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

Suggested fix: delete `neverRead`, or use it if the work computing it still matters

## `unused-private-member`

a private member can be used only from inside its class, so one that nothing in the class references cannot be reached

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

Suggested fix: delete `instance`; nothing inside `Settings` can reach it

## `unreachable-code`

this code cannot run, so whatever it was written to do is not happening

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

Suggested fix: delete it, or move it above the exit if it was meant to run

## `commented-out-code`

readers cannot tell whether the code still matters, so it is left in place, and version control already preserves it

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

Suggested fix: delete it; version control already preserves it

## `lazy-class`

the class costs a file, an import and a name, and does not do enough to justify them

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

Suggested fix: fold `Marker` into its only caller until it justifies its own file

## `data-class`

the behaviour that belongs with this data is implemented elsewhere, so the rules about the data are enforced everywhere except where the data is defined

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

Suggested fix: move the logic that operates on `Coordinate`'s fields onto `Coordinate`

## `speculative-generality`

it was built for a case that has not occurred, and until it does it is an extra indirection every reader has to follow. Interfaces are not reported: TypeScript interfaces are structural, so the absence of implementers says nothing about whether one is speculative

| | |
|---|---|
| Measures | abstract class with no subclass anywhere in the project |
| Reported as | info, verified |
| Reads | the import graph across every file |
| Fix | Collapse Hierarchy / Inline Class |
| Not in JavaScript | JavaScript has no abstract keyword, so no class declares itself waiting for a subclass |

From `fixtures/typescript/flow.ts`, line 220:

```ts

export abstract class Settled {
  run(): number {
```

Suggested fix: inline `Settled` into its single user until a second one arrives

## `empty-catch`

the failure is swallowed silently, so it first appears as wrong data elsewhere, with nothing in the logs to connect the two

| | |
|---|---|
| Measures | catch clause with an empty body |
| Reported as | error, verified |
| Reads | resolved scopes and symbols |
| Fix | Handle the Failure |
| Not in Go | Go has no exceptions and so no catch clause; an ignored error is a different shape this rule does not measure |

From `fixtures/typescript/hygiene.ts`, line 21:

```ts
    return total + 4999;
  } catch (error) {
  }
  return 0;
```

Suggested fix: log it, rethrow it, or write the reason ignoring it is correct

## `magic-number`

the number encodes a decision that is not recorded, so a reader cannot tell whether changing it is safe or what else depends on the same value

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

Suggested fix: name `3` once and refer to the name at all 4 sites

## `mixed-language-file`

a second language in the file gets none of the tooling the first language has, so nothing checks it until it fails at run time

| | |
|---|---|
| Measures | SQL or HTML markup inside a template literal |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Shared Module |

From `fixtures/typescript/conventions.ts`, line 5:

```ts
export function query(userId: string): string {
  return `SELECT id, name FROM users WHERE id = '${userId}'`;
}
```

Suggested fix: move the SQL into its own file, where its own tooling can read it

## `distant-declaration`

a reader has to keep the name in mind while reading everything in between, without knowing whether it will matter

| | |
|---|---|
| Measures | lines between a local's declaration and the line that first reads it |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Inline Variable |
| Tuned by | `thresholds.declaration_distance_lines` |

From `fixtures/typescript/queries.ts`, line 16:

```ts
export function summarise(rows: string[]): string {
  const separator = " | ";
  if (rows.length === 0) {
```

Suggested fix: move `separator` down to where it is first used

## `buried-configuration`

changing the program's target requires finding this line first, and nothing about its position indicates that it is a setting

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

Suggested fix: lift it to a named constant at the top of the module, or into config

## `journal-comment`

version control already records who changed the code and when, and unlike the comment it is not left behind by later edits

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

Suggested fix: delete it; `git log` and `git blame` record this and stay accurate

## `redundant-comment`

it takes space and attention to repeat what the name already says, and it has to be kept accurate when the name changes

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

Suggested fix: delete it, or say why `scoreOfUser` exists rather than what it is

## `cross-language-duplicate`

the same rule is written twice in two languages, so a change is likely to be made in only one and the two diverge

| | |
|---|---|
| Measures | one block's control-flow shape and constants found in another language |
| Reported as | info, low confidence |
| Reads | the syntax tree alone |
| Fix | Extract Shared Module |
| Default | off; name it under `rules.enabled` |

## `return-of-fresh-local`

the name is read once, by the line below it, and says nothing the expression it holds does not already say

| | |
|---|---|
| Measures | a local variable assigned on one line and returned on the next |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Inline Variable |

From `fixtures/typescript/flow.ts`, line 168:

```ts
export function freshTotal(rows: number[]): number {
  const total = rows.length * 2;
  return total;
```

Suggested fix: return the expression, or keep the name if it says something the expression does not

## `empty-method`

a caller receives no result and cannot distinguish it from work that succeeded, and a reader cannot tell whether the method is a stub or a decision

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

Suggested fix: give it a body, delete it, or state explicitly that doing nothing is intended

## `barrel-module`

it adds a name to every path through the project and answers no question of its own, while hiding from tools which file a name comes from

| | |
|---|---|
| Measures | a file whose every import comes straight back out of it |
| Reported as | info, verified |
| Reads | resolved scopes and symbols |
| Fix | Inline Delegation / Remove Dead Code |
| Not in Go | a package is the unit of import, and it has no index file |
| Not in Java | a class is imported by its own name |
| Not in C++ | a header is included by its own path |
| Not in Python | a package re-exports through `__all__`, which names rather than forwards |

From `fixtures/typescript/index.ts`, line 1:

```ts
export { add, describe } from "./clean";
export { tooManyParameters, deeplyNested, Overgrown, SameAsFeeder, Formatting, Catalogue, crowdedScope, widelyGuarded, manyPaths, spreadOut, loops, Spiral } from "./bloat";
```

Suggested fix: let callers import from the file that defines the name

## `passthrough-override`

it changes nothing and costs a reader a hop, and it suggests the type has its own behaviour for the member when it does not

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

Suggested fix: delete it, so the inherited member is used

## `duplicate-function`

one was copied from the other, and the next change to either has to find both or leave them inconsistent

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

Suggested fix: keep one and call it from the other, or delete the copy

## `dead-store`

the first write has no effect, so either a line that should read it is missing or the line that wrote it is unnecessary

| | |
|---|---|
| Measures | a value assigned twice with no read of the first |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/flow.ts`, line 57:

```ts
export function assigns(a: number, b: number): number {
  let found = 0;
  if ((found = a + b)) {
```

Suggested fix: delete the unread assignment, or read it

## `never-reassigned`

a reader has to allow for the possibility that the variable changes, and it never does, so the declaration draws attention it does not need

| | |
|---|---|
| Measures | a name declared as changeable that never changes |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Declare Immutable |
| Not in Python | the language has no word for a name that cannot change |
| Not in Go | a short declaration says nothing about whether the name moves |

From `fixtures/typescript/cost-lock-in-loop.ts`, line 18:

```ts
  let total = 0;
  for (let at = 0; at < rows.length; at++) {
    latches[at].lock();
```

Suggested fix: declare it as constant, so the compiler enforces it

## `discarded-expression`

either the line was meant to do something and does not, or it does nothing and should be removed, and a reader cannot tell which

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

Suggested fix: use the value, or delete the line

## `unused-import`

it makes the file appear to depend on something it does not use, and the dependency survives every search for who needs that module

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

Suggested fix: delete it; the module it names is not one this file depends on

## `rethrow-only-catch`

it reads as a decision about the failure and makes none, and in languages that rebuild the stack trace on rethrow it loses the original

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

Suggested fix: delete the handler so the failure propagates, or add handling specific to this level

## `placeholder-body`

it is a callable promise with nothing behind it, and every caller that trusts the signature finds out at run time

| | |
|---|---|
| Measures | a body whose only statement raises to signal that the implementation is not written |
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

Suggested fix: write it, or delete it until there is a caller that needs it

## `passthrough-function`

it adds a name and a stack frame between a caller and the function it wanted, and the indirection has no effect because it forwards unchanged

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

Suggested fix: let the caller call the target directly, unless this name marks a deliberate boundary

## `debug-output`

it was written to be read once by the person debugging, and it still prints for everyone else

| | |
|---|---|
| Measures | a write to the terminal outside a test, of the kind left behind after debugging |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/hygiene.ts`, line 64:

```ts
export function traced(): void {
  console.log("traced", everything);
}
```

Suggested fix: delete it, or route it through whatever this project logs with

## `duplicate-import`

two statements importing one module mean a reader has to check both to know what the file takes from it, and a merge can introduce one without anyone reviewing it

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

Suggested fix: fold these names into the first `./cycle-a` import

