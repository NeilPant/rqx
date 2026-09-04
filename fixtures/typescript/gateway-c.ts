import { charge } from "stripe";

export function collectC(amount: number) {
  return charge(amount);
}
