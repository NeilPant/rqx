import { orbit } from "./cycle-a";
export function withDeadTail(used, dead) {
  orbit();
  return used;
}
export function withDeadMiddle(dead, used) {
  return used;
}
export function respectsUnderscore(used, _deliberate) {
  return used;
}
export function twoDeadOnOneLine(used, deadA, deadB) {
  return used;
}
