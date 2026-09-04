import { charge } from "stripe";

export function collectB(amount: number) {
  return charge(amount);
}
