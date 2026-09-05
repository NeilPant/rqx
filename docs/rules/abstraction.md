# Abstraction

[Back to the catalogue](README.md)

## `type-code-switch`

the same set of cases is now spelled out in several places, so adding a case means finding all of them and the compiler will not remind you

| | |
|---|---|
| Measures | switch or if/else chain branching on the same discriminant in more than one place |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Conditional with Polymorphism / Replace Type Code with Subclasses / Replace Type Code with Polymorphic State |

From `fixtures/typescript/flow.ts`, line 65:

```ts
export function wideSwitch(kind: string): number {
  switch (kind) {
    case "a":
      return 1;
    case "b":
      return 2;
    case "c":
    default:
      return 0;
  }
}
```

rqx says: give `kind` a type per case and let each one carry its own behaviour, so adding a case is one new class rather than 2 edits

## `temporary-field`

the field is empty for most of the object's life, so every other method has to know it must not look at it

| | |
|---|---|
| Measures | instance field written and read by exactly one method, never in the constructor |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Class / Extract Method |

From `fixtures/typescript/shapes.ts`, line 13:

```ts
  private unusedSecret = 1;
  scratch?: string;
  run() { this.scratch = "on"; return this.scratch; }
```

rqx says: make `scratch` a local in `run`, or move both into a class that owns that step

## `refused-bequest`

the subclass is inheriting an interface it cannot honour, so a caller holding the base type has no safe way to know which methods actually work

| | |
|---|---|
| Measures | override whose body only throws or is empty |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Push Down Method / Extract Interface / Collapse Hierarchy |
| Not in Go | nothing is inherited in Go, so nothing can be refused: a type either has a method or does not |

From `fixtures/typescript/shapes.ts`, line 17:

```ts
}
export class Broken extends Engine { run(): string { throw new Error("no"); } }
export class Totals {
```

rqx says: if `Broken` cannot honour `run`, it is not a subtype — split the interface so it only promises what it can do

## `base-depends-on-derived`

the base cannot be deployed, tested or understood without its children, which is the entire benefit of having a base class given away

| | |
|---|---|
| Measures | base class referencing one of its own subclasses by name |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Invert Dependency / Push Down Method |
| Not in Go | Go has no inheritance: embedding is a field, so no type is a base another derives from |

From `fixtures/typescript/shapes.ts`, line 45:

```ts
  build(kind: string) {
    return kind === "cached" ? new CachedRegistry() : this;
  }
```

rqx says: move what the base needs from `CachedRegistry` behind a method the subclass implements, so the base stops naming its own children

## `inappropriate-static`

a static can never be overridden, so behaviour that clearly belongs to the instance is frozen for every subclass and every test that wants to fake it

| | |
|---|---|
| Measures | static method taking an instance of its own class as its first parameter |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Move Method |
| Not in Go | Go has no methods on the type itself, only on values of it, so the static-on-own-type shape cannot be written |

From `fixtures/typescript/shapes.ts`, line 15:

```ts
  run() { this.scratch = "on"; return this.scratch; }
  static build(other: Engine) { return other; }
}
```

rqx says: make `build` an instance method on `Engine`

## `temporal-coupling`

the order the methods must be called in is real but unwritten, so the only way to learn it is to call them wrongly first

| | |
|---|---|
| Measures | a field no constructor sets that two or more methods test before acting |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Introduce Assertion / Extract Class |

From `fixtures/typescript/lifecycle.ts`, line 1:

```ts
export class Connection {
  private socket?: string;
```

rqx says: set `opened` in the constructor, or split the stages into objects that cannot be used before they are ready

## `alternative-classes`

callers have to learn both interfaces to do one job, and a fix applied to one of them silently leaves the other wrong

| | |
|---|---|
| Measures | two classes with the same method count and arities and no shared name |
| Reported as | info, low confidence |
| Reads | resolved scopes and symbols |
| Fix | Rename Method / Extract Superclass / Move Method |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/intimacy.ts`, line 18:

```ts

export class CsvWriter {
  open(path: string): void { void path; }
```

rqx says: give `CsvWriter` and `JsonEmitter` one interface, or one superclass, so a caller learns the job once

## `wide-surface`

every public member is a promise to keep working, so the surface is what the type costs to change rather than what it does

| | |
|---|---|
| Measures | count of members a type exposes outside itself |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Interface / Extract Class |
| Tuned by | `thresholds.class_public_members` |

From `fixtures/typescript/bloat.ts`, line 18:

```ts

export class Overgrown {
  a1 = 1; a2 = 1; a3 = 1; a4 = 1; a5 = 1; a6 = 1; a7 = 1; a8 = 1; a9 = 1; a10 = 1;
```

rqx says: narrow what `Overgrown` promises: an interface for what callers use, and the rest kept inside

