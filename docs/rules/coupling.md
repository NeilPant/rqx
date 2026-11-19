# Coupling

[Back to the catalogue](README.md)

## `circular-dependency`

no module in the cycle can be understood, tested or loaded on its own, and the initialisation order that works today is accidental

| | |
|---|---|
| Measures | strongly connected component of more than one module in the import graph |
| Reported as | warning, verified |
| Reads | the import graph across every file |
| Fix | Extract Shared Module / Invert Dependency |

From `fixtures/typescript/cycle-a.ts`, line 1:

```ts
import { spin } from "./cycle-b";

```

Suggested fix: break the cycle by moving what they share into a module both can import, or invert the weaker of the two dependencies behind an interface

## `layer-violation`

a lower layer depends on a layer above it, so what the lower layer was separated from cannot be replaced without changing it

| | |
|---|---|
| Measures | an import pointing from an inner layer to an outer one |
| Reported as | error, verified |
| Reads | the import graph across every file |
| Fix | Invert Dependency |

## `forbidden-import`

the project declared this boundary and the import crosses it, which is a kind of architectural drift a reader cannot infer from the code

| | |
|---|---|
| Measures | an import the project's own rules say this subtree may not make |
| Reported as | error, verified |
| Reads | the import graph across every file |
| Fix | Invert Dependency |

## `message-chain`

the line depends on the shape of every object along the chain, so a change to any of them breaks a caller that does not mention it

| | |
|---|---|
| Measures | chained member access depth beyond the configured limit |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Encapsulate Delegation / Extract Method |
| Tuned by | `thresholds.message_chain_depth` |

From `fixtures/typescript/hygiene.ts`, line 37:

```ts
export function ship(order: { customer: { address: { city: string } } }) {
  return order.customer.address.city.trim().toUpperCase();
}
```

Suggested fix: ask the nearest object for what is needed, and let it do the reaching

## `feature-envy`

the method belongs on the other class, and while it stays here both classes have to change together to keep it working

| | |
|---|---|
| Measures | method touching another object's members more than its own |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Move Method / Extract Method |
| Tuned by | `thresholds.feature_envy_ratio` |

From `fixtures/typescript/reconcile.ts`, line 7:

```ts

  async reconcileBatch(batch: Batch, tolerance: number, dryRun: boolean, retries: number, verbose: boolean) {
    let settled = 0;
```

Suggested fix: move `reconcileBatch` onto `row`, where the data it wants already lives

## `middle-man`

every call pays for a hop that adds nothing, and every new method on the delegate needs a matching method here before callers can reach it

| | |
|---|---|
| Measures | class where most methods do nothing but delegate |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Inline Delegation / Inline Class |

From `fixtures/typescript/lifecycle.ts`, line 24:

```ts

export class ReportFactory {
  private rows: string[] = [];
```

Suggested fix: let callers talk to the delegate directly and delete the pass-throughs in `ReportFactory`

## `inappropriate-intimacy`

neither class can be read, tested or moved without the other, so the pair behaves as one class written in two places

| | |
|---|---|
| Measures | two classes in one file that each reference the other by name |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Move Method / Extract Class / Encapsulate Delegation |

From `fixtures/typescript/intimacy.ts`, line 5:

```ts
  total(): number {
    return Checkout.rate * this.items.length;
  }
```

Suggested fix: move what they share into one of them, or behind an interface both use

## `global-accessor`

it is a shared value behind a function call, and every caller still reads the same single instance

| | |
|---|---|
| Measures | a function whose whole body hands back something the file keeps at module level |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Encapsulate Field / Invert Dependency |

From `fixtures/typescript/performance.ts`, line 116:

```ts

export function tabsOpen(): number {
  return openTabs;
```

Suggested fix: pass the value to the code that needs it, so a caller can choose it

## `many-collaborators`

each collaborator is a reason for this type to change, and a test of it has to set up all of them first

| | |
|---|---|
| Measures | other types one type references |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Class / Invert Dependency |
| Tuned by | `thresholds.class_collaborators` |
| Not in Go | there are no types that hold behaviour to collect them |

From `fixtures/typescript/families.ts`, line 61:

```ts

export class DarkKit extends Kit {
  button(): Button {
    return new FlatButton();
  }
  panel(): Panel {
    return new FlatPanel();
  }
}

```

Suggested fix: give one collaborator the work that needs the others

## `deep-relative-import`

a path that climbs this far reaches across the project's structure instead of through it, and it breaks when either end moves

| | |
|---|---|
| Measures | levels a relative import climbs before it descends |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Shared Module / Invert Dependency |
| Tuned by | `thresholds.relative_climb` |
| Not in Go | an import is a module path, so there is nothing relative to climb |
| Not in Java | an import is a package name, and packages do not nest by path |

From `fixtures/typescript/hygiene.ts`, line 4:

```ts
import { orbit } from "./cycle-a";
import type { Far } from "../../../shared/far";
import { spinAgain } from "./cycle-a";
```

Suggested fix: move what `../../../shared/far` reaches for somewhere both ends can import, or turn the path into one the project resolves by name

## `mutable-module-state`

every function that reads it takes an argument that no caller passed, so two calls with the same arguments can return different results and no signature says why

| | |
|---|---|
| Measures | a binding at file level that the file writes again after declaring it |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Encapsulate Field / Extract Class |

From `fixtures/typescript/intimacy.ts`, line 30:

```ts

export let openSessions = 0;

```

Suggested fix: give it an owner that controls who writes it, and pass readers what they need

## `ambient-dependency`

the result depends on something no caller can set, so the only way to test the branch that reads it is to change the machine

| | |
|---|---|
| Measures | a body reaching for the clock, the environment or a random source |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Add Parameter / Invert Dependency |

From `fixtures/typescript/flow.ts`, line 115:

```ts
        setTimeout(() => {
          globalThis.console.log("deep");
        }, 1);
```

Suggested fix: take it as an argument, so a test can hand it whatever it needs

## `self-built-collaborators`

nothing can be substituted for them, so every test of the owner has to set up the real collaborator and everything behind it

| | |
|---|---|
| Measures | a field filled with something the object constructed itself |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Invert Dependency / Add Parameter |
| Not in Go | a value is built by a literal, which names no type to stand in for |

From `fixtures/typescript/intimacy.ts`, line 37:

```ts
  constructor() {
    this.writer = new CsvWriter();
  }
```

Suggested fix: accept it as an argument, so a caller can decide what it is

## `import-time-work`

it runs before any caller has asked for anything, in whatever order the imports resolve, and nothing can prevent it

| | |
|---|---|
| Measures | a call that runs the moment the file is loaded |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Not in Go | a file holds declarations, and only `init` runs on load |
| Not in Java | a file holds declarations, and only a static block runs on load |
| Not in C++ | a translation unit holds declarations |

From `fixtures/typescript/intimacy.ts`, line 32:

```ts

warmTheCache();

```

Suggested fix: put it behind a function the caller chooses to call

## `reaches-into-private`

the other type is free to change the member and has declared it private, so this line breaks on a change nobody could have known would reach it

| | |
|---|---|
| Measures | reading a member another type marks as private |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Encapsulate Delegation / Move Method |
| Default | off; name it under `rules.enabled` |
| Not in Go | the package boundary is the language's, and the compiler holds it |
| Not in Java | the compiler refuses it |
| Not in C++ | the compiler refuses it |

From `fixtures/typescript/intimacy.ts`, line 43:

```ts
  peek(other: Cart & { _items: string[] }): number {
    return other._items.length;
  }
```

Suggested fix: ask the other type for what this line needs, and let it decide how to provide it

## `wildcard-import`

nothing in the file says which names come from where, so a reader has to open the other module to find out, and a new export there can shadow a local name without any warning

| | |
|---|---|
| Measures | an import that brings in every name a module offers |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 1:

```ts
import * as everything from "./cycle-b";
import { neverRead } from "./cycle-c";
```

Suggested fix: import only the names this file uses

## `wide-fan-out`

a file that reaches into this many places cannot be read, moved or tested without most of the project coming with it

| | |
|---|---|
| Measures | modules a single file imports |
| Reported as | info, measured threshold |
| Reads | the import graph across every file |
| Fix | Extract Shared Module / Invert Dependency |
| Tuned by | `thresholds.fan_out` |

From `fixtures/typescript/index.ts`, line 1:

```ts
export { add, describe } from "./clean";
export { tooManyParameters, deeplyNested, Overgrown, SameAsFeeder, Formatting, Catalogue, crowdedScope, widelyGuarded, manyPaths, spreadOut, loops, Spiral } from "./bloat";
```

Suggested fix: give it one collaborator that knows the rest, or move what it is orchestrating to the things being orchestrated

## `wide-fan-in`

every change here reaches this many places, so the module is heavily depended on whether or not that was intended

| | |
|---|---|
| Measures | modules that import a single file |
| Reported as | info, measured threshold |
| Reads | the import graph across every file |
| Fix | Extract Interface |
| Tuned by | `thresholds.fan_in` |

From `fixtures/typescript/clean.ts`, line 1:

```ts
export function add(first: number, second: number): number {
  return first + second;
```

Suggested fix: split it along the seams its importers actually use, so a change reaches only the ones that asked for it

