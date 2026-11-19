// Code that resembles a warning and is not one. Every rule should stay silent here.

// working-constructor: handing the message to the parent records it and does no work.
export class Refusal extends Error {
  constructor(reason: string) {
    super(reason);
    this.name = "Refusal";
  }

  describe(): string {
    return "refused: " + this.message;
  }
}

// self-built-collaborators, working-constructor: a set is where the object keeps its
// data, not a collaborator a test would replace, and making an empty one is not work.
export class Visits {
  private readonly seen: Set<string>;

  constructor() {
    this.seen = new Set();
  }

  // example-value: a comment shows an example; it is never a value that ships.
  // `page` is a full address, e.g. https://example.com/path.
  isFirstVisit(page: string): boolean {
    const first = !this.seen.has(page);
    this.seen.add(page);
    return first;
  }
}
