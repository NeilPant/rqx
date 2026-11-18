import { find } from "./finder";

// The list searched is the list looped over, so the search is made once per element.

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
export function firstOccurrences(items: string[]): string[] {
  const firsts: string[] = [];
  for (let at = 0; at < items.length; at++) {
    if (find(items, items[at]) === at) {
      firsts.push(items[at]);
    }
  }
  return firsts;
}

// Only the position is passed on, which is one number whatever the list holds.

// @complexity time=O(n) space=O(1) conf=verified none=quadratic-via-call
export function positionOfFirst(items: string[]): number {
  let position = 0;
  for (const item of items) {
    position += item.length;
  }
  return find(items, "first") + position;
}
