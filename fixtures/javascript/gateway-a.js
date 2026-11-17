import { charge } from "stripe";
export function collectA(amount) {
  return charge(amount);
}
