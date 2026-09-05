import * as everything from "./cycle-b";
import { neverRead } from "./cycle-c";
import { orbit } from "./cycle-a";
import type { Far } from "../../../shared/far";
import { spinAgain } from "./cycle-a";

export function pricing(cart: number[], applyDiscount: boolean): number {
  let neverRead = cart.length;
  let total = cart.reduce((a, b) => a + b, 0);

  // const legacy = total * 0.2;
  // return legacy + shipping;

  if (!applyDiscount && total > 4999 && cart.length > 12 && orbit()) {
    return total;
  } else {
    total = total * 4999;
  }
  try {
    return total + 4999;
  } catch (error) {
  }
  return 0;
}

export function unreachable(value: number) {
  // rqx-ignore magic-number
  const seed = 1;
  return value + seed;
  value = 1;
}

// @ts-ignore
export const suppressed = 1;

export function ship(order: { customer: { address: { city: string } } }) {
  return order.customer.address.city.trim().toUpperCase();
}

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.

export function nothingHappens(): void {
}

export function forwardOrbit(seed: number): number {
  return orbit(seed);
}

export function stillToWrite(): number {
  throw new Error("not implemented");
}

export function reraises(): number {
  try {
    return orbit(1);
  } catch (error) {
    throw error;
  }
}

export function traced(): void {
  console.log("traced", everything);
}

export function maybeCount(rows: number[]): number | undefined {
  if (rows.length === 0) {
    return;
  }
  return rows.length;
}

export function closesOut(rows: number[]): number {
  try {
    return rows.length;
  } finally {
    return 0;
  }
}

export function refuses(): number {
  throw new Error("no");
}

export function catchesEverything(rows: number[]): number {
  try {
    return rows.length;
  } catch (error: any) {
    return error ? 0 : 0;
  }
}

export function stops(code: number): void {
  process.exit(code);
}

export function requests(url: string): string {
  return fetchPage(url, true);
}

function fetchPage(url: string, retry: boolean): string {
  return retry ? url : url;
}

export function narrated(rows: number[]): number {
  // set the total
  let total = 0;
  // walk the rows
  for (const row of rows) total += row;
  // halve it
  total = total / 2;
  // give it back
  return total;
}

// Works out the score for the given user id, and gives back the score it
// worked out, having looked at the id that was handed in by whoever called
// it, which is the id of the user whose score is wanted here. The id is not
// checked, because whoever calls this has already checked it, and checking
// twice would be slower for no benefit anybody has ever measured. If the id
// is wrong the answer is wrong, which is the same as it has always been in
// every version of this that anybody has written since the beginning.
export function scoreOf(userId: number): number {
  return userId;
}

/** Score of user id */
export function scoreOfUser(userId: number): number {
  return userId;
}

export function guardedAdd(a: number, b: number, c: number): number {
  if (a < 0) return 0;
  if (b < 0) return 0;
  if (c < 0) return 0;
  return a + b + c;
}

export function netted(rows: number[]): number {
  try {
    let total = 0;
    for (const row of rows) total += row;
    return total;
  } catch (error) {
    return 0;
  }
}
