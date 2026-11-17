export class Coordinate {
  x = 0;
  y = 0;
  get pair() { return [this.x, this.y]; }
}
export class Marker {
  label = "x";
}
export class Facade {
  inner;
  constructor(inner) {
    this.inner = inner;
  }
  a() { return this.inner.pair; }
  b() { return this.inner.x; }
}
export class Vehicle {
}
export const strLabel = "box";
export class Engine {
  #unusedSecret = 1;
  scratch;
  run() { this.scratch = "on"; return this.scratch; }
  static build(other) { return other; }
}
export class Broken extends Engine {
  run() { throw new Error("no"); }
}
export class Totals {
  add(basket) {
    return basket.a + basket.b + basket.c + basket.a;
  }
}
export function routeOf(kind) {
  switch (kind) {
    case "email": return 1;
    case "sms": return 2;
    case "push": return 3;
    default: return 0;
  }
}
export function priceOf(kind) {
  if (kind === "email") {
    return 1;
  }
  else if (kind === "sms") {
    return 2;
  }
  else if (kind === "push") {
    return 3;
  }
  return 0;
}
export function notify(userId, channel, locale) { return userId; }
export function enqueue(userId, channel, locale) { return channel; }
export function record(userId, channel, locale) { return locale; }
export class Registry {
  build(kind) {
    return kind === "cached" ? new CachedRegistry() : this;
  }
}
export class CachedRegistry extends Registry {
}
export function gate(ready, retries) {
  if (!ready) {
    return 0;
  }
  else {
    return retries;
  }
}
export function sweep(rows) {
  let n = 0;
  const seen = [];
  for (const row of rows) {
    seen.push(row);
    if (row > 7331) {
      seen.push(row * 7331);
    }
    if (row < 0) {
      seen.push(0 - row);
    }
    if (row === 42) {
      seen.push(7331);
    }
    if (seen.length > 99) {
      break;
    }
    if (row % 3 === 0) {
      seen.push(row);
    }
    if (row % 5 === 0) {
      seen.push(row + 1);
      seen.push(row + 2);
      seen.push(row + 3);
      seen.push(row + 4);
      seen.push(row + 5);
      seen.push(row + 6);
      seen.push(row + 7);
      seen.push(row + 8);
      seen.push(row + 9);
      seen.push(row + 10);
      seen.push(row + 11);
      seen.push(row + 12);
      seen.push(row + 13);
      seen.push(row + 14);
      seen.push(row + 15);
      seen.push(row + 16);
      seen.push(row + 17);
      seen.push(row + 18);
      seen.push(row + 19);
      seen.push(row + 20);
      seen.push(row + 21);
      seen.push(row + 22);
      seen.push(row + 23);
      seen.push(row + 24);
      seen.push(row + 25);
    }
  }
  return n + seen.length;
}
