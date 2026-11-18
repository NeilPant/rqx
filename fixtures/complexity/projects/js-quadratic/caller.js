import { find } from "./finder.js";

// The list searched is the list looped over, so the search is made once per element.

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
export function firstOccurrences(items) {
  const firsts = [];
  for (let at = 0; at < items.length; at++) {
    if (find(items, items[at]) === at) {
      firsts.push(items[at]);
    }
  }
  return firsts;
}

// Each word is looked for in a fixed list of four, so the search costs the same on every pass.

// @complexity time=O(n) space=O(1) conf=verified none=quadratic-via-call
export function countStopWords(words) {
  let hits = 0;
  for (const word of words) {
    if (find(["a", "an", "the", "of"], word) >= 0) {
      hits += 1;
    }
  }
  return hits;
}
