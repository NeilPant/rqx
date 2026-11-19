// Code that resembles a warning and is not one. Every rule should stay silent here.

// unused-private-member: `#finish` is called from the arrow function a class field
// holds, which is part of the class too.
export class Job {
  #done = false;

  #finish() {
    this.#done = true;
  }

  onExit = () => {
    this.#finish();
  };

  isDone() {
    return this.#done;
  }
}
