import { orbit } from "./cycle-a";

export function withDeadTail(used: number, dead: string) {
  orbit();
  return used;
}

export function withDeadMiddle(dead: string, used: number) {
  return used;
}

export function respectsUnderscore(used: number, _deliberate: string) {
  return used;
}

export function twoDeadOnOneLine(used: number, deadA: string, deadB: string) {
  return used;
}
