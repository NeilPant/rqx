import assert from "node:assert/strict";
import { add } from "./clean";

const sumIs = (got, want) => {
  expect(got).toBe(want);
};

it("checks with node assert", () => {
  assert.equal(add(1, 1), 2);
});

it("checks the chai way", () => {
  add(1, 1).should.equal(2);
});

it("fails by hand", () => {
  if (add(1, 0) !== 1) {
    assert.fail("add(1, 0) is not 1");
  }
});

it("checks through a helper", () => {
  sumIs(add(1, 1), 2);
});

it("runs a table", () => {
  for (const [left, right, want] of [[0, 0, 0], [1, 0, 1]]) {
    expect(add(left, right)).toBe(want);
  }
});

it("runs a named table", () => {
  const rows = [{ value: 0 }, { value: 1 }];
  for (const row of rows) {
    expect(add(row.value, 0)).toBe(row.value);
  }
});

it.each([[0, 0], [1, 1]])("adds %i", (value, want) => {
  expect(add(value, 0)).toBe(want);
});

it("decides three ways", () => {
  let total = add(0, 1);
  if (total > 1) {
    total = add(total, -1);
  } else {
    total = add(total, 1);
  }
  while (total < 2) {
    total = add(total, 1);
  }
  switch (total) {
    case 2:
      break;
    default:
      total = 0;
  }
  expect(total).toBe(2);
});
