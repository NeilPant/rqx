import { charge } from "stripe";
export function collectB(amount) {
  return charge(amount);
}
