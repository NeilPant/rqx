import { scaled } from "./lookalikes";

// ambient-dependency: a test file sets the environment and reads the clock to
// arrange its cases, in setup and in helpers alike.
beforeEach(() => {
  process.env.TZ = "UTC";
});

function stamped(): number {
  return Date.now();
}

it("scales the time it is given", () => {
  const now = stamped();
  expect(scaled(now)).toBe(now * 2);
});
