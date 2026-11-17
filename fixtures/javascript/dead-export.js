import { orbit } from "./cycle-a";
export function reachable() {
  return orbit();
}
export function neverImported(value) {
  return value * 2;
}
