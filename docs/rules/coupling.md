# Coupling

[Back to the catalogue](README.md)

## `circular-dependency`

no module in the cycle can be understood, tested or loaded on its own, and the initialisation order that makes it work today is accidental

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

rqx says: break the cycle by moving what they share into a module both can import, or invert the weaker of the two dependencies behind an interface

## `layer-violation`

the layer the project put underneath now depends on the one above it, so the thing it was separated from cannot be replaced without changing it

| | |
|---|---|
| Measures | an import pointing from an inner layer to an outer one |
| Reported as | error, verified |
| Reads | the import graph across every file |
| Fix | Invert Dependency |

## `forbidden-import`

the boundary was written down and this import crosses it, which is the one kind of architectural drift a reader cannot infer from the code

| | |
|---|---|
| Measures | an import the project's own rules say this subtree may not make |
| Reported as | error, verified |
| Reads | the import graph across every file |
| Fix | Invert Dependency |

## `message-chain`

this line depends on the shape of every object along the chain, so a change to any one of them breaks a caller that never mentioned it

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

rqx says: ask the nearest object for what you need and let it do the reaching

## `feature-envy`

the method wants to live on the other class, and while it does not, both classes have to stay in step for it to keep working

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

rqx says: move `reconcileBatch` onto `row`, where the data it wants already lives

## `middle-man`

every call pays for a hop that adds nothing, and every new method on the delegate needs a twin here before anyone can reach it

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

rqx says: let callers talk to the delegate directly and delete the pass-throughs in `ReportFactory`

## `inappropriate-intimacy`

neither class can be read, tested or moved without the other coming with it, so the pair behaves as one class written in two places

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

rqx says: move what they share into one of them, or behind an interface both use

## `global-accessor`

it is the shared value wearing a function's clothes, and every caller is still reading the same one thing

| | |
|---|---|
| Measures | a function whose whole body hands back something the file keeps at module level |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Encapsulate Field / Invert Dependency |

From `fixtures/typescript/performance.ts`, line 112:

```ts

export function tabsOpen(): number {
  return openTabs;
```

rqx says: hand the value to whoever needs it, so a caller can decide what it is

## `many-collaborators`

each one is a reason for this type to change, and a test of it has to stand up all of them first

| | |
|---|---|
| Measures | other types one type has to know about |
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

rqx says: give one of them the work that needs the rest

## `deep-relative-import`

a path that climbs this far is reaching across the shape of the project rather than through it, and it breaks the moment either end is moved

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

rqx says: move what `../../../shared/far` reaches for somewhere both ends can import, or turn the path into one the project resolves by name

## `mutable-module-state`

every function that reads it takes an argument nobody passed, so two calls with the same arguments can answer differently and no signature says why

| | |
|---|---|
| Measures | a binding at file level that anything in the file can write to |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Encapsulate Field / Extract Class |

From `fixtures/typescript/intimacy.ts`, line 30:

```ts

export let openSessions = 0;

```

rqx says: fix it at its declaration, or give it an owner that decides who writes it

## `ambient-dependency`

the answer depends on something no caller can set, so the only way to test the branch that reads it is to change the machine

| | |
|---|---|
| Measures | a body reaching for the clock, the environment or a random source |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Parameter Object / Invert Dependency |

From `fixtures/typescript/flow.ts`, line 102:

```ts
        setTimeout(() => {
          globalThis.console.log("deep");
        }, 1);
```

rqx says: take it as an argument, so a test can hand it whatever it needs

## `self-built-collaborators`

nothing can be put in its place, so every test of the owner stands up the real collaborator and everything behind it

| | |
|---|---|
| Measures | a field filled with something the object constructed itself |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Invert Dependency / Introduce Parameter Object |
| Not in Go | a value is built by a literal, which names no type to stand in for |

From `fixtures/typescript/intimacy.ts`, line 37:

```ts
  constructor() {
    this.writer = new CsvWriter();
  }
```

rqx says: accept it as an argument, so a caller can decide what it is

## `import-time-work`

it happens before any caller has asked for anything, in whatever order the imports happen to resolve, and nothing can stop it

| | |
|---|---|
| Measures | a call that runs the moment the file is loaded |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Separate Query from Modifier |
| Not in Go | a file holds declarations, and only `init` runs on load |
| Not in Java | a file holds declarations, and only a static block runs on load |
| Not in C++ | a translation unit holds declarations |

From `fixtures/typescript/intimacy.ts`, line 32:

```ts

warmTheCache();

```

rqx says: put it behind a function the caller chooses to call

## `reaches-into-private`

the other type is free to change it and has said so, so this line breaks on a change nobody could have known would reach it

| | |
|---|---|
| Measures | reading a member another type marked as its own business |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Encapsulate Delegation / Move Method |
| Default | off; name it under `[rules] enabled` |
| Not in Go | the package boundary is the language's, and the compiler holds it |
| Not in Java | the compiler refuses it |
| Not in C++ | the compiler refuses it |

From `fixtures/typescript/intimacy.ts`, line 43:

```ts
  peek(other: Cart & { _items: string[] }): number {
    return other._items.length;
  }
```

rqx says: ask the other type for what this line wanted, and let it decide how

## `wildcard-import`

nothing in the file says which names came from where, so a reader has to open the other module to find out and a new export there can shadow a local one silently

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

rqx says: name the handful of things this file actually uses

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
export { tooManyParameters, deeplyNested, Overgrown, SameAsFeeder, Formatting, Catalogue, crowdedScope, widelyGuarded, manyPaths, spreadOut, loops } from "./bloat";
```

rqx says: give it one collaborator that knows the rest, or move what it is orchestrating to the things being orchestrated

## `wide-fan-in`

every change here reaches this many places, so the module is load bearing whether or not anybody decided it should be

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

rqx says: split it along the seams its importers actually use, so a change reaches only the ones that asked for it

