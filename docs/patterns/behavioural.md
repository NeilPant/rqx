# Behavioural

[Back to the patterns](README.md)

## Chain of Responsibility

pass a request along a line of handlers until one takes it

| | |
|---|---|
| Cited as | Design Patterns §5.1 |
| Coverage | chain-of-responsibility-fits |

### `chain-of-responsibility-fits`

no single value decides which branch runs, so the order of the tests is the logic, and adding a case means working out where in the chain it goes

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | an if/else chain of three or more branches where every branch tests something different |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Conditional with Polymorphism / Extract Class |

From `fixtures/typescript/patterns.ts`, line 50:

```ts
export function route(request: Request) {
  if (isHealthCheck(request)) {
    return "ok";
  } else if (looksLikeUpload(request)) {
    return "stored";
  } else if (hasSession(request)) {
    return "page";
  }
  return "denied";
```

rqx says: give each test and its body a handler that either answers or passes the request on, so adding a 4 case is one new handler rather than an edit in the middle of the chain

## Command

make a request an object, so it can be queued, logged or undone

| | |
|---|---|
| Cited as | Design Patterns §5.2 |
| Coverage | command-fits |

### `command-fits`

the branches already agree on a shape, so the switch is a lookup table written as control flow, and nothing can queue, log or undo what it picks

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | a switch or chain whose every branch does nothing but call one function of the same shape |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Conditional with Polymorphism / Replace Parameter with Explicit Methods |

From `fixtures/typescript/patterns.ts`, line 78:

```ts
export function apply(action: Action) {
  switch (action.kind) {
    case "add":
      return addItem(action);
    case "remove":
      return removeItem(action);
    case "clear":
      return clearCart(action);
  }
  return 0;
```

rqx says: give each case an object with the call it already makes, and look it up by `action.kind` instead of branching on it

## Interpreter

give a grammar a class per rule and evaluate over the tree

| | |
|---|---|
| Cited as | Design Patterns §5.3 |
| Coverage | interpreter-found |

### `interpreter-found`

each rule of the grammar is a class that knows how to evaluate itself, so the tree runs by asking its own nodes

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | three or more subclasses of one base that each answer the same evaluating method |
| Confidence | low confidence |
| Reads | the import graph across every file |
| Default | off; name it under `[patterns] enabled` |
| Not in Go | the language has no inheritance, so a grammar has no base to hang its rules from |

From `fixtures/typescript/families.ts`, line 107:

```ts

export abstract class Expression {
  abstract eval(): number;
```

rqx says: `Expression` is the seam the family varies behind

## Iterator

walk a collection without exposing how it is stored

| | |
|---|---|
| Cited as | Design Patterns §5.4 |
| Coverage | iterator-found |

### `iterator-found`

callers walk it with the language's own loop and never learn how it is stored, so the storage can change without them

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class declaring the language's own iteration members beside the collection it walks |
| Confidence | verified |
| Reads | resolved scopes and symbols |
| Not in Go | there is no iteration protocol to implement, so a method named Next is a convention rather than a promise |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 217:

```ts

export class Pages {
  private readonly pages: string[] = [];
```

rqx says: callers walk `Pages` with the language's own loop and never learn how it stores what it holds

## Mediator

stop a set of objects referring to each other directly

| | |
|---|---|
| Cited as | Design Patterns §5.5 |
| Coverage | mediator-fits |

### `mediator-fits`

no member of the group can be read, tested or moved without the others, because each one already knows about all of them

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | three or more modules that all reach each other, directly or through the ones between |
| Confidence | measured threshold |
| Reads | the import graph across every file |
| Fix | Extract Class / Invert Dependency |

From `fixtures/typescript/cycle-a.ts`, line 1:

```ts
import { spin } from "./cycle-b";

```

rqx says: give the group one place that knows how its members talk, so each of the 3 only has to know that one

## Memento

capture an object's state so it can be put back later

| | |
|---|---|
| Cited as | Design Patterns §5.6 |
| Coverage | memento-found |

### `memento-found`

state is captured and put back without the caller learning what is in it, so the object keeps its own invariants

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a method that hands out a snapshot type paired with one that takes the same type back |
| Confidence | low confidence |
| Reads | resolved scopes and symbols |
| Default | off; name it under `[patterns] enabled` |
| Not in JavaScript | a method declares no return type, so nothing says it hands back itself or a copy |

From `fixtures/typescript/patterns.ts`, line 247:

```ts

export class Editor {
  private text = "";
```

rqx says: `Editor` puts its own state back without exposing it

## Observer

tell every dependant when one object changes

| | |
|---|---|
| Cited as | Design Patterns §5.7 |
| Coverage | observer-found, observer-without-removal |

### `observer-found`

the thing that changes does not know who cares, so a new dependant is a registration rather than an edit

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a collection of listeners, a method that adds to it, and a method that tells them all |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 224:

```ts

export class Feed {
  private readonly listeners: Array<(value: number) => void> = [];
```

rqx says: `Feed` tells its dependants without knowing them

### `observer-without-removal`

nothing that subscribes can ever be released, so the collection and everything it points at outlive the objects that registered

| | |
|---|---|
| Says | the pattern is half applied |
| Decided by | a collection of listeners with a way to subscribe and none to stop |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Add Parameter / Remove Setter |
| Not in JavaScript | a field declares no type, so nothing says what a class holds |

From `fixtures/typescript/patterns.ts`, line 237:

```ts

export class Ticker {
  private readonly handlers: Array<() => void> = [];
```

rqx says: give `Ticker` a way to unsubscribe, or every listener it takes lives as long as it does

## State

let an object change class as its state changes

| | |
|---|---|
| Cited as | Design Patterns §5.8 |
| Coverage | state-fits |

### `state-fits`

the value being switched on is also the value the branches change, so the transitions are scattered across every place that reads it

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | the same value switched on in more than one place, where a branch reassigns it |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Type Code with Polymorphic State / Replace Conditional with Polymorphism |

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

rqx says: give `this.state` a type per state and let each one return the next, so a transition is a return value rather than an assignment in 3 places

## Strategy

make a family of algorithms interchangeable behind one interface

| | |
|---|---|
| Cited as | Design Patterns §5.9 |
| Coverage | strategy-fits |

### `strategy-fits`

the cases are spelled out in several places, so a new one is found by searching rather than by the compiler refusing to build

| | |
|---|---|
| Says | the pattern is would fit |
| Decided by | the same value switched on in more than one place, where no branch reassigns it |
| Confidence | measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Conditional with Polymorphism / Replace Type Code with Polymorphic State |

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

rqx says: give `kind` a type per case behind one interface, so adding a case is one new class rather than 5 edits

## Template Method

fix the shape of an algorithm and let subclasses fill in the steps

| | |
|---|---|
| Cited as | Design Patterns §5.10 |
| Coverage | template-method-found |

### `template-method-found`

the order of the steps is fixed in one place and only the steps vary, so a subclass cannot get the sequence wrong

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a method with a body that calls two or more members the same class leaves abstract |
| Confidence | verified |
| Reads | resolved scopes and symbols |
| Not in Go | there are no abstract members for a base to leave unimplemented |
| Not in JavaScript | JavaScript has no abstract keyword to leave a member for a subclass |

From `fixtures/typescript/patterns.ts`, line 257:

```ts

export abstract class Importer {
  abstract read(): string;
```

rqx says: a subclass of `Importer` fills in the steps and cannot change their order

## Visitor

add an operation over a hierarchy without changing its classes

| | |
|---|---|
| Cited as | Design Patterns §5.11 |
| Coverage | visitor-found |

### `visitor-found`

an operation over the whole hierarchy lives in one class, so adding one touches nothing the hierarchy already does

| | |
|---|---|
| Says | the pattern is in use |
| Decided by | a class declaring three or more visit methods over one hierarchy |
| Confidence | verified |
| Reads | the import graph across every file |

From `fixtures/typescript/families.ts`, line 129:

```ts

export class Printer {
  visitLiteral(node: Literal) {
```

rqx says: `Printer` is the seam the family varies behind

