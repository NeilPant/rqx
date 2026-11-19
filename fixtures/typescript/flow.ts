export function restated(ready: boolean): boolean {
  if (ready) {
    return true;
  } else {
    return false;
  }
}

export function compared(ready: boolean, count: number): number {
  if (ready === true) {
    return 1;
  }
  if (count === count) {
    return 2;
  }
  return 0;
}

export function sameEitherWay(count: number): number {
  if (count > 10) {
    return 99;
  } else {
    return 99;
  }
}

export function decidedNothing(count: number): void {
  if (count > 10) {
  }
}

export function buriedExits(a: number, b: number): number {
  const scale = a * b;
  for (const step of [1, 2, 3]) {
    if (step > scale) {
      return 1;
    }
    while (step > 0) {
      if (a > b) {
        return 3;
      }
      if (b > a) {
        return 4;
      }
      if (a === b) {
        return 5;
      }
      if (scale === 0) {
        return 6;
      }
    }
  }
  return 0;
}

// assignment-in-condition must not fire: a second pair of parentheses, or a
// comparison of the result, says the assignment is meant.
export function assignsOnPurpose(a: number, b: number): number {
  let found = 0;
  if ((found = a * b)) {
    return found;
  }
  if ((found = a - b) > 0) {
    return found;
  }
  return 0;
}

export function assigns(a: number, b: number): number {
  let found = 0;
  if (found = a + b) {
    return found;
  }
  return 0;
}

export function wideSwitch(kind: string): number {
  switch (kind) {
    case "a":
      return 1;
    case "b":
      return 2;
    case "c":
      return 3;
    case "d":
      return 4;
    case "e":
      return 5;
    case "f":
      return 6;
    case "g":
      return 7;
    case "h":
      return 8;
    case "i":
      return 9;
    case "j":
      return 10;
    case "k":
      return 11;
    case "l":
      return 12;
    case "m":
      return 13;
    default:
      return 0;
  }
}

export function deeplyNestedCallbacks(): void {
  setTimeout(() => {
    setTimeout(() => {
      setTimeout(() => {
        setTimeout(() => {
          globalThis.console.log("deep");
        }, 1);
      }, 1);
    }, 1);
  }, 1);
}

export function trailingJump(items: number[]): number {
  let total = 0;
  for (const item of items) {
    total += item;
    continue;
  }
  return total;
}

export function twoArmed(kind: string): number {
  switch (kind) {
    case "one":
      return 1;
    default:
      return 0;
  }
}

export function settled(balance: number): boolean {
  return balance === 0.1;
}

export function dropped(total: number, other: number): number {
  total === other;
  return total;
}

export function tierOf(score: number): string {
  return score > 90 ? "gold" : score > 50 ? "silver" : "bronze";
}

export function admits(rank: number, guests: number, staff: number): boolean {
  return rank > 0 && guests > 0 || staff > 0;
}

export function changed(before: string, after: string): boolean {
  return !(before === after);
}

export function labelOf(state: number): string {
  switch (state) {
    case 0:
      return "new";
    case 1:
      return "open";
    case 2:
      return "closed";
  }
  return "unknown";
}

export function firstOf(items: number[]): number {
  for (const item of items) {
    return item;
  }
  return 0;
}

export function freshTotal(rows: number[]): number {
  const total = rows.length * 2;
  return total;
}

export function asksTwice(kind: string): number {
  if (kind === "a") {
    return 1;
  } else if (kind === "b") {
    return 2;
  } else if (kind === "a") {
    return 3;
  }
  return 0;
}

export function joined(rows: string[]): string {
  let out = "";
  for (const row of rows) {
    out += row;
  }
  return out;
}

export function fallsThrough(kind: number): number {
  switch (kind) {
    case 1:
      globalThis.console.warn("one");
    case 2:
      return 2;
    default:
      return 0;
  }
}

// silent-fallthrough must not fire: the first arm says it falls through, and the
// second returns on both sides of its if/else.
export function fallsThroughOnPurpose(kind: number): number {
  switch (kind) {
    case 1:
      globalThis.console.warn("one");
    // falls through
    case 2:
      if (kind === 2) {
        return 2;
      } else {
        return 1;
      }
    default:
      return 0;
  }
}

export abstract class Settled {
  run(): number {
    return 1;
  }
}
