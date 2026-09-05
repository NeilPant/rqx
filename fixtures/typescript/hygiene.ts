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
