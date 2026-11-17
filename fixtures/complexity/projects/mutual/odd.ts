import { isEven } from "./even";

// @complexity time=O(n) space=O(n) conf=heuristic
export function isOdd(n: number): boolean {
  return n === 0 ? false : isEven(n - 1);
}
