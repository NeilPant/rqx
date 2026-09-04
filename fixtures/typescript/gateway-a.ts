import { charge } from "stripe";

export function collectA(amount: number) {
  return charge(amount);
}
