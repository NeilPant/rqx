import { find } from "./index";

// The call is to the barrel's export, and the cost is the function it re-exports.

// @complexity time=O(n*m) space=O(1) conf=verified rule=quadratic-via-call
export function countFound(items: string[], wanted: string[]): number {
  let found = 0;
  for (const target of wanted) {
    if (find(items, target) >= 0) {
      found += 1;
    }
  }
  return found;
}
