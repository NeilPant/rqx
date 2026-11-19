export function restated(ready) {
  if (ready) {
    return true;
  }
  else {
    return false;
  }
}
export function compared(ready, count) {
  if (ready === true) {
    return 1;
  }
  if (count === count) {
    return 2;
  }
  return 0;
}
export function sameEitherWay(count) {
  if (count > 10) {
    return 99;
  }
  else {
    return 99;
  }
}
export function decidedNothing(count) {
  if (count > 10) {
  }
}
export function buriedExits(a, b) {
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
export function assignsOnPurpose(a, b) {
  let found = 0;
  if ((found = a * b)) {
    return found;
  }
  if ((found = a - b) > 0) {
    return found;
  }
  return 0;
}

export function assigns(a, b) {
  let found = 0;
  if (found = a + b) {
    return found;
  }
  return 0;
}
export function wideSwitch(kind) {
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
export function deeplyNestedCallbacks() {
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
export function trailingJump(items) {
  let total = 0;
  for (const item of items) {
    total += item;
    continue;
  }
  return total;
}
export function twoArmed(kind) {
  switch (kind) {
    case "one":
      return 1;
    default:
      return 0;
  }
}
export function settled(balance) {
  return balance === 0.1;
}
export function dropped(total, other) {
  total === other;
  return total;
}
export function tierOf(score) {
  return score > 90 ? "gold" : score > 50 ? "silver" : "bronze";
}
export function admits(rank, guests, staff) {
  return rank > 0 && guests > 0 || staff > 0;
}
export function changed(before, after) {
  return !(before === after);
}
export function labelOf(state) {
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
export function firstOf(items) {
  for (const item of items) {
    return item;
  }
  return 0;
}
export function freshTotal(rows) {
  const total = rows.length * 2;
  return total;
}
export function asksTwice(kind) {
  if (kind === "a") {
    return 1;
  }
  else if (kind === "b") {
    return 2;
  }
  else if (kind === "a") {
    return 3;
  }
  return 0;
}
export function joined(rows) {
  let out = "";
  for (const row of rows) {
    out += row;
  }
  return out;
}
export function fallsThrough(kind) {
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
export function fallsThroughOnPurpose(kind) {
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
export class Settled {
  run() {
    return 1;
  }
}
