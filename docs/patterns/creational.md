# Creational

[Back to the patterns](README.md)

## Abstract Factory

make families of related objects without naming their concrete classes

| | |
|---|---|
| Cited as | Design Patterns §3.1 |
| Coverage | abstract-factory-found |

### `abstract-factory-found`

the whole family is chosen once, so nothing downstream can mix a part from one family with a part from another

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class whose every method returns an abstraction, implemented by two or more others |
| Confidence | measured threshold |
| Reads | the import graph across every file |
| Not in Go | the language has no inheritance, so nothing can implement the family twice |
| Not in JavaScript | JavaScript has no abstract keyword to leave a member for a subclass |

From `fixtures/typescript/families.ts`, line 56:

```ts

export abstract class Kit {
  abstract button(): Button;
```

Suggested fix: `Kit` is the seam the family varies behind

## Builder

separate constructing an object from what it ends up being

| | |
|---|---|
| Cited as | Design Patterns §3.2 |
| Coverage | builder-found, builder-without-build |

### `builder-found`

the object is assembled a step at a time and handed over once, so no caller ever holds a half-built one

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | three or more methods returning the class itself, and one returning something else |
| Confidence | verified |
| Reads | resolved scopes and symbols |
| Not in JavaScript | a method declares no return type, so nothing says it hands back itself or a copy |

From `fixtures/typescript/patterns.ts`, line 154:

```ts

export class ReportBuilder {
  private title = "";
```

Suggested fix: `ReportBuilder` is assembled a step at a time

### `builder-without-build`

the chain never ends in the thing being built, so callers reach past it for the state it was supposed to hand over

| | |
|---|---|
| Says | the pattern is half applied |
| Decided by | three or more methods returning the class itself and none returning anything else |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Separate Query from Modifier |
| Not in JavaScript | a method declares no return type, so nothing says it hands back itself or a copy |

From `fixtures/typescript/patterns.ts`, line 171:

```ts

export class QueryBuilder {
  private clauses = 0;
```

Suggested fix: give `QueryBuilder` one method that returns what it built, so callers stop reaching for its state

## Factory Method

let a subclass decide which class to instantiate

| | |
|---|---|
| Cited as | Design Patterns §3.3 |
| Coverage | factory-method-found |

### `factory-method-found`

which concrete class is made is decided by the subclass rather than by the caller, so a new one is a new subclass and no edit anywhere else

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a method returning a type other classes implement, on a class two or more others extend |
| Confidence | measured threshold |
| Reads | the import graph across every file |
| Not in Go | the language has no inheritance, so nothing can override the decision |
| Not in JavaScript | JavaScript has no abstract keyword to leave a member for a subclass |

From `fixtures/typescript/families.ts`, line 17:

```ts

export abstract class Logistics {
  abstract createTransport(): Transport;
```

Suggested fix: `Logistics` is the seam the family varies behind

## Prototype

make new objects by copying an existing one

| | |
|---|---|
| Cited as | Design Patterns §3.4 |
| Coverage | prototype-found |

### `prototype-found`

a new one is made from an existing one, so the caller never has to know which concrete class it is holding

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a method named for copying that returns the class it is declared on |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Not in JavaScript | a method declares no return type, so nothing says it hands back itself or a copy |

From `fixtures/typescript/patterns.ts`, line 185:

```ts

export class Palette {
  colours = 3;
```

Suggested fix: a new `Palette` is made from an existing one

## Singleton

one instance, reachable from everywhere

| | |
|---|---|
| Cited as | Design Patterns §3.5 |
| Coverage | singleton-found, singleton-escapable |

### `singleton-found`

one instance exists and the constructor enforces it, which is also why nothing that depends on it can be tested against a different one

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a non-public constructor beside a static field of the class's own type |
| Confidence | verified |
| Reads | resolved scopes and symbols |
| Not in Go | there are no constructors to make private; a single instance is a package-level variable |
| Not in Python | nothing in the language stops a second call to the constructor |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 194:

```ts

export class Settings {
  private static instance: Settings;
```

Suggested fix: everything that reaches `Settings` shares one instance, and nothing depending on it can be tested against another

### `singleton-escapable`

the shared instance is a convention rather than a guarantee, so some code is using the one and some is making its own

| | |
|---|---|
| Says | the pattern is half applied |
| Decided by | a static field of the class's own type beside a constructor anything can still call |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Hide Method |
| Not in Go | there are no constructors to make private; a single instance is a package-level variable |
| Not in Python | nothing in the language stops a second call to the constructor |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 202:

```ts

export class Clock {
  private static current: Clock;
```

Suggested fix: make `Clock`'s constructor private, or stop calling the shared instance the only one

