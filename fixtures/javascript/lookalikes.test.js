import { scaled } from "./lookalikes";

// ambient-dependency: a test file sets the environment and reads the clock to
// arrange its cases, in setup and in helpers alike.
beforeEach(() => {
  process.env.TZ = "UTC";
});

function stamped() {
  return Date.now();
}

it("scales the time it is given", () => {
  const now = stamped();
  expect(scaled(now)).toBe(now * 2);
});

// self-asserting-test: each side runs the code, so the check holds it to answering
// the same twice.
it("scales the same value the same way twice", () => {
  expect(scaled(1)).toBe(scaled(1));
});
