import { orbit } from "./cycle-a";

export function reachable() {
  return orbit();
}

export function neverImported(value: number) {
  return value * 2;
}

export interface AlsoNeverImported {
  id: string;
}
