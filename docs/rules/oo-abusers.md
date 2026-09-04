# Object-Orientation Abusers

[Back to the catalogue](README.md)

## `type-code-switch`

the same set of cases is now spelled out in several places, so adding a case means finding all of them and the compiler will not remind you

| | |
|---|---|
| Measures | switch or if/else chain branching on the same discriminant in more than one place |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Smell | Switch Statements |
| Clean Code | G23 Prefer Polymorphism to If/Else or Switch/Case |
| Fix | Replace Conditional with Polymorphism / Replace Type Code with Subclasses / Replace Type Code with State/Strategy |

From `fixtures/typescript/patterns.ts`, line 5:

```ts
  open() {
    switch (this.state) {
      case "closed":
        this.state = "open";
        return true;
      case "open":
        return false;
      default:
        return false;
    }
  }
```

rqx says: give `this.state` a type per case and let each one carry its own behaviour, so adding a case is one new class rather than 3 edits

## `temporary-field`

the field is empty for most of the object's life, so every other method has to know it must not look at it

| | |
|---|---|
| Measures | instance field written and read by exactly one method, never in the constructor |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Smell | Temporary Field |
| Clean Code | G8 Too Much Information |
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
| Smell | Refused Bequest |
| Clean Code | G7 Base Classes Depending on Their Derivatives |
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
| Smell | no direct catalogue entry |
| Clean Code | G7 Base Classes Depending on Their Derivatives |
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
| Smell | no direct catalogue entry |
| Clean Code | G18 Inappropriate Static |
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
| Smell | no direct catalogue entry |
| Clean Code | G31 Hidden Temporal Couplings |
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
| Smell | Alternative Classes with Different Interfaces |
| Clean Code | G5 Duplication |
| Fix | Rename Method / Extract Superclass / Move Method |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/intimacy.ts`, line 18:

```ts

export class CsvWriter {
  open(path: string): void { void path; }
```

rqx says: give `CsvWriter` and `JsonEmitter` one interface, or one superclass, so a caller learns the job once

