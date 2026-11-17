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

Suggested fix: give `kind` a type per case and let each one carry its own behaviour, so adding a case is one new class rather than 5 edits

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

Suggested fix: make `scratch` a local in `run`, or move both into a class that owns that step

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

Suggested fix: if `Broken` cannot honour `run`, it is not a subtype — split the interface so it only promises what it can do

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

Suggested fix: move what the base needs from `CachedRegistry` behind a method the subclass implements, so the base stops naming its own children

## `inappropriate-static`

a static can never be overridden, so behaviour that clearly belongs to the instance is frozen for every subclass and every test that wants to fake it

| | |
|---|---|
| Measures | static method taking an instance of its own class as its first parameter |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Move Method |
| Not in Go | Go has no methods on the type itself, only on values of it, so the static-on-own-type shape cannot be written |
| Not in JavaScript | a parameter declares no type, so nothing says it is an instance of its own class |

From `fixtures/typescript/shapes.ts`, line 15:

```ts
  run() { this.scratch = "on"; return this.scratch; }
  static build(other: Engine) { return other; }
}
```

Suggested fix: make `build` an instance method on `Engine`

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

Suggested fix: set `opened` in the constructor, or split the stages into objects that cannot be used before they are ready

## `alternative-classes`

callers have to learn both interfaces to do one job, and a fix applied to one of them silently leaves the other wrong

| | |
|---|---|
| Measures | two classes with the same method count and arities and no shared name |
| Reported as | info, low confidence |
| Reads | resolved scopes and symbols |
| Fix | Rename Method / Extract Superclass / Move Method |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/intimacy.ts`, line 18:

```ts

export class CsvWriter {
  open(path: string): void { void path; }
```

Suggested fix: give `CsvWriter` and `JsonEmitter` one interface, or one superclass, so a caller learns the job once

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

Suggested fix: narrow what `Overgrown` promises: an interface for what callers use, and the rest kept inside

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
| Not in JavaScript | JavaScript has no abstract keyword to leave a member for a subclass |

From `fixtures/typescript/flow.ts`, line 202:

```ts

export abstract class Settled {
  run(): number {
    return 1;
  }
}

```

Suggested fix: drop the word, or give it the member a subclass has to write

## `split-cohesion`

each group is a type nobody has separated yet, so a change to one of them is read against the fields of all of them

| | |
|---|---|
| Measures | groups of methods inside one type that share no state with each other |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Class / Move Method |
| Not in Go | methods are attached to a type from outside it |
| Not in Python | an attribute is written into the instance, not declared on the type |

From `fixtures/typescript/conventions.ts`, line 17:

```ts

export class Ledger {
  private entryList: number[] = [];
  private label = "ledger";

  record(entry: number): void {
    this.entryList.push(entry);
    return this.label;
  }
}

```

Suggested fix: give each group its own type, and let one of them hold the other

## `method-ignores-instance`

it needs nothing the type holds, so the type is a namespace for it and every caller has to build one to reach it

| | |
|---|---|
| Measures | a method that never mentions the instance it hangs off |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Move Method |
| Default | off; name it under `rules.enabled` |
| Not in Go | a method names its receiver, and one that does not is a function |

From `fixtures/typescript/bloat.ts`, line 21:

```ts
  a11 = 1; a12 = 1; a13 = 1; a14 = 1; a15 = 1; a16 = 1; a17 = 1; a18 = 1; a19 = 1;
  m1() {} m2() {} m3() {}
}
```

Suggested fix: move it to whatever it is about, or make it a function of its own

## `assign-once-field`

nothing after construction moves it, so every reader has to check that for themselves before they can rely on it

| | |
|---|---|
| Measures | a changeable field only the constructor ever writes |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Encapsulate Field |
| Default | off; name it under `rules.enabled` |
| Not in Python | an attribute is written into the instance, not declared on the type |
| Not in Go | a struct field is written by whatever holds the struct |

From `fixtures/typescript/bloat.ts`, line 19:

```ts
export class Overgrown {
  a1 = 1; a2 = 1; a3 = 1; a4 = 1; a5 = 1; a6 = 1; a7 = 1; a8 = 1; a9 = 1; a10 = 1;
  a11 = 1; a12 = 1; a13 = 1; a14 = 1; a15 = 1; a16 = 1; a17 = 1; a18 = 1; a19 = 1;
```

Suggested fix: make it final, so every reader can see it cannot move

## `single-method-field`

it is that method's local kept where every other method can reach it, so every one of them has to be read before it can be changed

| | |
|---|---|
| Measures | a field only one method of its type ever reads |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Class / Inline Variable |
| Not in Go | a struct field is read by whatever holds the struct |
| Not in C++ | the methods that read it are in another file |
| Not in Python | an attribute is written into the instance, not declared on the type |

From `fixtures/typescript/patterns.ts`, line 155:

```ts
export class ReportBuilder {
  private title = "";
  withTitle(title: string): ReportBuilder {
```

Suggested fix: make it that method's own, or move the method to whatever the field belongs with

## `deep-inheritance`

answering what one method does means reading every type above it, and a change to any of them reaches every type below

| | |
|---|---|
| Measures | types above one type in its own hierarchy |
| Reported as | warning, verified |
| Reads | the import graph across every file |
| Fix | Replace Inheritance with Composition / Collapse Hierarchy |
| Tuned by | `thresholds.inheritance_depth` |
| Not in Go | there is no inheritance to climb |

From `fixtures/typescript/hierarchy.ts`, line 40:

```ts
class Level3 extends Level2 {}
export class Level4 extends Level3 {}

```

Suggested fix: give it what it needs as a collaborator, so the answer is one object away rather than four types up

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

Suggested fix: use what it inherits from, or give it the difference that justifies it

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

Suggested fix: make them functions in a module, or give the type the state that earns it

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

Suggested fix: take what it needs as an argument, and do the work in a method a caller chooses to call

