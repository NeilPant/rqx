# Dispensables

[Back to the catalogue](README.md)

## `duplicate-code`

a fix applied to one copy leaves the others wrong, and nothing in the code points from the copy you edited to the ones you did not

| | |
|---|---|
| Measures | identical normalized-AST block hash across sites |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Smell | Duplicate Code |
| Clean Code | G5 Duplication |
| Fix | Extract Method / Pull Up Method |
| Tuned by | `thresholds.clone_min_statements`, `thresholds.clone_min_nodes` |

From `fixtures/typescript/reconcile.ts`, line 33:

```ts
function first(rows: Row[]) {
  const total = sum(rows);
  record(total);
  notify(total);
  return total;
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
| Smell | Dead Code |
| Clean Code | F4 Dead Function |
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
| Smell | Dead Code |
| Clean Code | G9 Dead Code |
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
| Smell | Dead Code |
| Clean Code | G9 Dead Code |
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
| Smell | Dead Code |
| Clean Code | G9 Dead Code |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 4:

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
| Smell | Dead Code |
| Clean Code | F4 Dead Function |
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
| Smell | Dead Code |
| Clean Code | G9 Dead Code |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 24:

```ts
  return value;
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
| Smell | Comments |
| Clean Code | C5 Commented-Out Code |
| Fix | Remove Dead Code |
| Tuned by | `thresholds.commented_code_lines` |

From `fixtures/typescript/hygiene.ts`, line 7:

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
| Smell | Lazy Class |
| Clean Code | G12 Clutter |
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
| Smell | Data Class |
| Clean Code | G17 Misplaced Responsibility |
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
| Smell | Speculative Generality |
| Clean Code | G12 Clutter |
| Fix | Collapse Hierarchy / Inline Class |

From `fixtures/typescript/patterns.ts`, line 257:

```ts

export abstract class Importer {
  abstract read(): string;
```

rqx says: inline `Importer` into its single user until a second one arrives

## `empty-catch`

the failure is swallowed silently, so the first sign of it is wrong data somewhere far away with nothing in the logs to connect the two

| | |
|---|---|
| Measures | catch clause with an empty body |
| Reported as | error, verified |
| Reads | resolved scopes and symbols |
| Smell | no direct catalogue entry |
| Clean Code | G4 Overridden Safeties |
| Fix | Remove Dead Code |
| Not in Go | Go has no exceptions and so no catch clause; an ignored error is a different shape this rule does not measure |

From `fixtures/typescript/hygiene.ts`, line 17:

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
| Smell | Primitive Obsession |
| Clean Code | G25 Replace Magic Numbers with Named Constants |
| Fix | Replace Magic Number with Symbolic Constant / Extract Variable |
| Tuned by | `thresholds.magic_number_repeats` |

From `fixtures/typescript/hygiene.ts`, line 10:

```ts

  if (!applyDiscount && total > 4999 && cart.length > 12 && orbit()) {
    return total;
```

rqx says: name `4999` once and refer to the name at all 3 sites

## `mixed-language-file`

a second language in the file gets none of the tooling the first one has, so nothing checks it until it fails at run time

| | |
|---|---|
| Measures | SQL or HTML markup inside a template literal |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Smell | no direct catalogue entry |
| Clean Code | G1 Multiple Languages in One Source File |
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
| Smell | no direct catalogue entry |
| Clean Code | G10 Vertical Separation |
| Fix | Extract Variable / Inline Temp |
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
| Smell | no direct catalogue entry |
| Clean Code | G35 Keep Configurable Data at High Levels |
| Fix | Replace Magic Number with Symbolic Constant / Extract Variable |
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
| Smell | Comments |
| Clean Code | C1 Inappropriate Information |
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
| Smell | Comments |
| Clean Code | C3 Redundant Comment |
| Fix | Remove Dead Code |

From `fixtures/typescript/precision.ts`, line 10:

```ts

// user name
export const userName = "ada";
```

rqx says: delete it, or say why `userName` exists rather than what it is

