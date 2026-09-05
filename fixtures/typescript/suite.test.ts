import { add } from "./clean";

describe.skip("pricing", () => {
  it("discounts", () => { expect(add(1, 2)).toBe(3); });
});

it.only("ships", () => { expect(add(1, 1)).toBe(2); });

it("checks nothing", () => { const value = add(1, 1); });

it("waits on the clock", async () => {
  await new Promise((resolve) => setTimeout(resolve, 2000));
  expect(add(1, 1)).toBe(2);
});

it("branches on the input", () => {
  if (add(1, 1) === 2) {
    expect(add(1, 1)).toBe(2);
  }
});

it("asserts the same thing twice", () => {
  expect(add(1, 1)).toBe(2);
  expect(add(1, 1)).toBe(2);
});

it("catches the failure itself", () => {
  try {
    add(1, 1);
  } catch (error) {
    expect(error).toBeDefined();
  }
});

it("prints while it runs", () => {
  console.log("here");
  expect(add(1, 1)).toBe(2);
});

it("reads the store", async () => {
  const rows = await db.query("rows");
  expect(rows).toBeDefined();
});

it("works", () => {
  expect(add(1, 1)).toBe(2);
});

it("counts three ways", () => {
  expect(add(1, 1)).toBe(2);
  expect(add(2, 2)).toBe(4);
  expect(add(3, 3)).toBe(6);
});

it("holds the code to itself", () => {
  expect(add(1, 1)).toBe(add(1, 1));
});

it("expects a number from nowhere", () => {
  expect(add(2400, 2599)).toBe(4999);
});
