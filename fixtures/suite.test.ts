import { add } from "./clean";

describe.skip("pricing", () => {
  it("discounts", () => { expect(add(1, 2)).toBe(3); });
});

it.only("ships", () => { expect(add(1, 1)).toBe(2); });

it("checks nothing", () => { const value = add(1, 1); });
