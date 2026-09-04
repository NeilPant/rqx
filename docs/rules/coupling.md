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

From `fixtures/typescript/hygiene.ts`, line 31:

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

