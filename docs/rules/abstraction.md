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

rqx says: give `kind` a type per case and let each one carry its own behaviour, so adding a case is one new class rather than 5 edits

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

## `abstract-nothing`

the word says a subclass has work to do and there is none, so the only thing it stops is somebody constructing it

| | |
|---|---|
| Measures | a type declared abstract with nothing left for anyone to fill in |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Collapse Hierarchy / Inline Class |
| Not in Go | there is no inheritance and no abstract keyword |
| Not in C++ | abstractness is a member with no body, which the type never declares |
| Not in Python | abstractness is a base class it inherits, not a word on the type |

From `fixtures/typescript/flow.ts`, line 202:

```ts

export abstract class Settled {
  run(): number {
    return 1;
  }
}

```

rqx says: drop the word, or give it the member a subclass has to write

## `empty-subclass`

it is a second name for the thing it came from, and a reader has to open it to find that out

| | |
|---|---|
| Measures | a type that inherits and declares nothing of its own |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Collapse Hierarchy / Inline Class |
| Not in Go | there is no inheritance for a type to add nothing to |

From `fixtures/typescript/bloat.ts`, line 24:

```ts

export class SameAsFeeder extends Overgrown {}

```

rqx says: use what it inherits from, or give it the difference that justifies it

## `static-only-class`

nothing is ever built from it, so the type is a namespace wearing a class's syntax and nothing can be substituted for it in a test

| | |
|---|---|
| Measures | a type whose every method belongs to the type rather than to an instance |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Inline Class / Move Method |
| Not in Go | there are no classes to hold the methods |

From `fixtures/typescript/bloat.ts`, line 26:

```ts

export class Formatting {
  static pad(text: string): string { return text; }
  static trim(text: string): string { return text; }
}

```

rqx says: make them functions in a module, or give the type the state that earns it

## `working-constructor`

the object cannot exist without whatever that work needs, so every test of it has to stand the work up first

| | |
|---|---|
| Measures | calls made while an object is being built, beyond recording what it was given |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method / Separate Query from Modifier |
| Not in Go | a value is built by a literal, which can only assign |

From `fixtures/typescript/bloat.ts`, line 33:

```ts
  private rows: number[];
  constructor(path: string) {
    this.rows = loadRows(path);
  }
  size(): number { return this.rows.length; }
```

rqx says: take what it needs as an argument, and do the work in a method a caller chooses to call

