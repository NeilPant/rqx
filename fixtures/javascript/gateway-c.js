import { charge } from "stripe";
export function collectC(amount) {
  return charge(amount);
}
