// Two functions in two files that call each other.
import { isOdd } from "./odd";

// @complexity time=O(n) space=O(n) conf=heuristic
export function isEven(n: number): boolean {
  return n === 0 ? true : isOdd(n - 1);
}
