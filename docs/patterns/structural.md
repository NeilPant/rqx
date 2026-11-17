# Structural

[Back to the patterns](README.md)

## Adapter

make one interface look like the one a caller expects

| | |
|---|---|
| Cited as | Design Patterns §4.1 |
| Coverage | adapter-found |

### `adapter-found`

it makes something written against one interface usable where another is expected, and the translation lives in one place

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class implementing one interface while forwarding most of its methods to a field of an unrelated type |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Not in Go | a type here declares no interface, so nothing says what it can stand in for |
| Not in JavaScript | JavaScript declares no types, so nothing says what this class can stand in for |

From `fixtures/typescript/patterns.ts`, line 134:

```ts

export class SmsNotifier implements Notifier {
  constructor(private readonly gateway: SmsGateway) {}
```

rqx says: `SmsNotifier` translates one interface into another; keep the translation here rather than at its call sites

## Bridge

let an abstraction and its implementation vary independently

| | |
|---|---|
| Cited as | Design Patterns §4.2 |
| Coverage | bridge-fits |

### `bridge-fits`

the two things varying are multiplied into subclasses, so adding one value to either axis means writing a class for every value of the other

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | four or more subclasses whose names cover two independent axes exactly |
| Confidence | measured threshold |
| Reads | the import graph across every file |
| Fix | Replace Inheritance with Composition / Extract Interface |
| Not in Go | the language has no inheritance, so there is no hierarchy to split in two |

From `fixtures/typescript/families.ts`, line 79:

```ts

export abstract class Shape {
  abstract draw(): string;
```

rqx says: give `Shape` a field for the second axis, so the two vary apart instead of multiplying

## Composite

treat one object and a tree of them the same way

| | |
|---|---|
| Cited as | Design Patterns §4.3 |
| Coverage | composite-found |

### `composite-found`

one of these and a tree of them answer to the same interface, so callers never branch on which they were handed

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class holding a collection whose element type is the class itself or something it implements |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 144:

```ts

export class Menu {
  private readonly items: Menu[] = [];
```

rqx says: one `Menu` and a tree of them answer the same way

## Decorator

add behaviour to one object without touching its class

| | |
|---|---|
| Cited as | Design Patterns §4.4 |
| Coverage | decorator-found, decorator-not-substitutable |

### `decorator-found`

the class can stand in for what it wraps, so callers of the interface get the extra behaviour without knowing it is there

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class that implements an interface, holds one of the same interface, forwards to it and adds behaviour of its own |
| Confidence | verified |
| Reads | resolved scopes and symbols |
| Not in Go | a type here declares no interface, so nothing says what it can stand in for |
| Not in JavaScript | JavaScript declares no types, so nothing says what this class can stand in for |

From `fixtures/typescript/patterns.ts`, line 102:

```ts

export class LoggingNotifier implements Notifier {
  constructor(private readonly inner: Notifier) {}
```

rqx says: `LoggingNotifier` can stand in for `Notifier`

### `decorator-not-substitutable`

it wraps the thing but cannot be passed where the thing is expected, so every caller has to know which of the two it is holding

| | |
|---|---|
| Says | the pattern is half applied |
| Decided by | a class forwarding most of its methods to something it holds while declaring no interface of its own |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Extract Interface / Replace Inheritance with Composition |
| Not in Go | a type here declares no interface, so nothing says what it can stand in for |
| Not in Python | nothing in the language declares an interface, so wrapping without one is ordinary composition |
| Not in JavaScript | nothing in the language declares an interface, so wrapping without one is ordinary composition |

From `fixtures/typescript/patterns.ts`, line 267:

```ts

export class RetryingGateway {
  constructor(private readonly gateway: SmsGateway) {}
```

rqx says: give `RetryingGateway` the interface `SmsGateway` answers to, or callers have to know which of the two they are holding

## Facade

one entrance to a subsystem, so callers do not learn its parts

| | |
|---|---|
| Cited as | Design Patterns §4.5 |
| Coverage | facade-fits |

### `facade-fits`

every one of those files is written against someone else's API, so the blast radius of that dependency changing is the number of files, not one

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | three or more first-party files importing the same third-party package directly |
| Confidence | measured threshold |
| Reads | the import graph across every file |
| Fix | Extract Shared Module / Encapsulate Delegation |

From `fixtures/typescript/gateway-a.ts`, line 1:

```ts
import { charge } from "stripe";

```

rqx says: put `stripe` behind one module of your own, so replacing it is one edit rather than 3 edits

## Flyweight

share one object between many uses instead of copying it

| | |
|---|---|
| Cited as | Design Patterns §4.6 |
| Coverage | flyweight-found |

### `flyweight-found`

instances are handed out from a pool rather than made, so the same value is one object however many places hold it

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a static collection of the class's own type beside a static method that returns one |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Not in Go | a type here has no static members, so a shared pool is a package variable rather than part of the type |
| Not in Python | a class attribute and an instance default are written the same way, so nothing says the pool is shared |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 210:

```ts

export class Glyph {
  private static pool: Glyph[] = [];
```

rqx says: the same `Glyph` is one object wherever it is held

## Proxy

stand in for another object and control reaching it

| | |
|---|---|
| Cited as | Design Patterns §4.7 |
| Coverage | proxy-found |

### `proxy-found`

it controls reaching the thing it stands for without changing what the thing does, which is the whole of what a proxy is

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class implementing an interface it also holds, where every method forwards and none adds anything |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Not in Go | a type here declares no interface, so nothing says what it can stand in for |
| Not in JavaScript | JavaScript declares no types, so nothing says what this class can stand in for |

From `fixtures/typescript/patterns.ts`, line 115:

```ts

export class GuardedNotifier implements Notifier {
  constructor(private readonly inner: Notifier) {}
```

rqx says: `GuardedNotifier` adds nothing to `Notifier`

