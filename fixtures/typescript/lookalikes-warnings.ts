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
